//! Generic streaming state machine for event-pattern matching.
//!
//! This is the **single** implementation of the matching logic. The
//! crate's public Pattern API (Arc-based, multi-thread safe) and the
//! WASM bindings (Rc-based, single-threaded, JS callbacks) both wrap
//! [`Runtime`] with their own type parameters for predicates, emit, and
//! timestamp extraction.
//!
//! ## Type parameters
//!
//! - `E` — event type. Must be `Clone + 'static` because captured events
//!   are stored in `Match<E>` and a single event may belong to multiple
//!   in-flight partial matches.
//! - `M` — emit-output type. The user's chosen result of a successful
//!   match.
//! - `P` — predicate-storage type. Implements [`Predicate<E>`]. The
//!   default `tflo-cep::Pattern` uses `ArcPredicate<E>`; `tflo-cep-wasm`
//!   uses `JsPredicate`.
//! - `Em` — emit-callback type. Implements [`EmitCallback<E, M>`].
//! - `Ts` — timestamp-extractor type. Implements [`TimestampCallback<E>`].
//!
//! Each trait is intentionally a *callable* abstraction without any
//! `Send + Sync` bound — the bounds (and threading model) are determined
//! by the concrete types each consumer plugs in. The engine itself
//! makes no assumption about threading.

use crate::matched::Match;

/// Maximum simultaneous in-flight partial matches per [`Runtime`].
/// Mirrored by the WASM runtime so the bounded-by-construction
/// guarantee is identical on both sides.
pub const MAX_IN_FLIGHT: usize = 1024;

/// A predicate evaluable against `&E`. Implementations decide their
/// own storage strategy (Arc, Rc, custom).
pub trait Predicate<E>: 'static {
    /// Test the event against this predicate.
    fn evaluate(&self, event: &E) -> bool;
}

/// An emit callback that turns a successful [`Match<E>`] into the
/// user's output type `M`.
pub trait EmitCallback<E, M>: 'static {
    /// Produce the emit value for a successful match.
    fn emit(&self, m: &Match<E>) -> M;
}

/// A timestamp extractor.
pub trait TimestampCallback<E>: 'static {
    /// Read the event-time (milliseconds) from `event`.
    fn timestamp(&self, event: &E) -> i64;
}

/// One compiled step in a pattern.
pub struct Step<E, P> {
    /// Step name — used by `Match::at("name")`.
    pub name: String,
    /// The predicate.
    pub predicate: P,
    /// Optional time bound (milliseconds) — `None` means "no bound."
    pub within_ms: Option<i64>,
    /// `true` for `not_then` steps (negative terminal).
    pub is_negative: bool,
    /// PhantomData to keep `E` in the type.
    _marker: core::marker::PhantomData<fn(&E) -> bool>,
}

impl<E, P> Step<E, P> {
    /// Construct a positive step.
    pub fn positive(name: impl Into<String>, predicate: P, within_ms: Option<i64>) -> Self {
        Self {
            name: name.into(),
            predicate,
            within_ms,
            is_negative: false,
            _marker: core::marker::PhantomData,
        }
    }

    /// Construct a negative (terminal) step.
    pub fn negative(name: impl Into<String>, predicate: P, within_ms: Option<i64>) -> Self {
        Self {
            name: name.into(),
            predicate,
            within_ms,
            is_negative: true,
            _marker: core::marker::PhantomData,
        }
    }
}

/// One in-flight partial match.
struct PartialMatch<E> {
    captures: Vec<(String, E)>,
    next_step: usize,
    deadline_ts: Option<i64>,
}

/// A compiled pattern — the immutable description of what the runtime
/// is matching. Construct with [`Compiled::new`].
pub struct Compiled<E, M, P, Em, Ts> {
    /// Pattern name — used in the `Match<E>` passed to emit.
    pub name: String,
    /// Steps in order; index 0 is the `when` step.
    pub steps: Vec<Step<E, P>>,
    /// Emit callback.
    pub emit_fn: Em,
    /// Optional timestamp extractor — `None` means "treat every event as ts=0."
    pub timestamp_fn: Option<Ts>,
    /// PhantomData for M.
    _marker: core::marker::PhantomData<fn() -> M>,
}

impl<E, M, P, Em, Ts> Compiled<E, M, P, Em, Ts> {
    /// Construct a compiled pattern. Callers are responsible for
    /// well-formedness (non-empty steps, terminal `not_then`, etc.) —
    /// the public builder in `tflo-cep::Pattern` validates that.
    pub fn new(
        name: impl Into<String>,
        steps: Vec<Step<E, P>>,
        emit_fn: Em,
        timestamp_fn: Option<Ts>,
    ) -> Self {
        Self {
            name: name.into(),
            steps,
            emit_fn,
            timestamp_fn,
            _marker: core::marker::PhantomData,
        }
    }
}

/// The streaming matching state machine.
///
/// Push events with [`push`](Self::push); collect emitted values from
/// the return value. Call [`flush`](Self::flush) on end-of-stream to
/// drain any negative-step partial matches whose deadlines have not
/// elapsed in event-time.
pub struct Runtime<E, M, P, Em, Ts> {
    compiled: Compiled<E, M, P, Em, Ts>,
    in_flight: Vec<PartialMatch<E>>,
}

impl<E, M, P, Em, Ts> Runtime<E, M, P, Em, Ts>
where
    E: Clone + 'static,
    M: 'static,
    P: Predicate<E>,
    Em: EmitCallback<E, M>,
    Ts: TimestampCallback<E>,
{
    /// Construct a fresh runtime from a compiled pattern.
    pub fn new(compiled: Compiled<E, M, P, Em, Ts>) -> Self {
        Self {
            compiled,
            in_flight: Vec::new(),
        }
    }

    /// Reset to the just-constructed state. The compiled pattern is
    /// preserved; in-flight partial matches are dropped.
    pub fn reset(&mut self) {
        self.in_flight.clear();
    }

    /// Push one event through the state machine. Returns any signals
    /// emitted by this event (deadline-resolved negatives, completed
    /// positives, or single-step `when`-only patterns).
    pub fn push(&mut self, event: E) -> Vec<M> {
        let ts = self
            .compiled
            .timestamp_fn
            .as_ref()
            .map(|f| f.timestamp(&event))
            .unwrap_or(0);
        self.push_at(event, ts)
    }

    /// Push with an externally-supplied timestamp. Useful when callers
    /// have a more authoritative time source than the configured
    /// timestamp extractor (or none has been configured).
    pub fn push_at(&mut self, event: E, ts: i64) -> Vec<M> {
        let mut out = Vec::new();
        self.advance_deadlines(ts, &mut out);
        self.advance_partials(&event, ts, &mut out);
        self.try_open_new(&event, ts, &mut out);
        out
    }

    /// Drain pending negative-step matches whose deadlines have not
    /// elapsed yet. Semantically: end-of-stream is treated like a
    /// deadline that never closed.
    pub fn flush(&mut self) -> Vec<M> {
        let mut out = Vec::new();
        let mut i = 0;
        while i < self.in_flight.len() {
            let step_idx = self.in_flight[i].next_step;
            let step = &self.compiled.steps[step_idx];
            if step.is_negative {
                let pm = self.in_flight.remove(i);
                self.complete(pm, &mut out);
            } else {
                i = i.saturating_add(1);
            }
        }
        out
    }

    fn next_deadline_for(&self, step_idx: usize, ref_ts: i64) -> Option<i64> {
        if step_idx >= self.compiled.steps.len() {
            return None;
        }
        self.compiled.steps[step_idx]
            .within_ms
            .map(|ms| ref_ts.saturating_add(ms))
    }

    fn complete(&self, pm: PartialMatch<E>, out: &mut Vec<M>) {
        let m = Match::new(self.compiled.name.clone(), pm.captures);
        out.push(self.compiled.emit_fn.emit(&m));
    }

    fn advance_deadlines(&mut self, ts: i64, out: &mut Vec<M>) {
        let mut i = 0;
        while i < self.in_flight.len() {
            let expired = self.in_flight[i]
                .deadline_ts
                .is_some_and(|d| ts > d);
            if !expired {
                i = i.saturating_add(1);
                continue;
            }
            let step_idx = self.in_flight[i].next_step;
            let is_negative = self.compiled.steps[step_idx].is_negative;
            if is_negative {
                let pm = self.in_flight.remove(i);
                self.complete(pm, out);
            } else {
                self.in_flight.remove(i);
            }
        }
    }

    fn advance_partials(&mut self, event: &E, ts: i64, out: &mut Vec<M>) {
        let mut to_remove: Vec<usize> = Vec::new();
        let mut completions: Vec<PartialMatch<E>> = Vec::new();
        for idx in (0..self.in_flight.len()).rev() {
            let pm = &self.in_flight[idx];
            let step_idx = pm.next_step;
            let step = &self.compiled.steps[step_idx];
            let matches = step.predicate.evaluate(event);
            if !matches {
                continue;
            }
            if step.is_negative {
                to_remove.push(idx);
                continue;
            }
            let new_next_step = step_idx.saturating_add(1);
            let new_deadline = self.next_deadline_for(new_next_step, ts);
            let step_name = step.name.clone();
            {
                let pm = &mut self.in_flight[idx];
                pm.captures.push((step_name, event.clone()));
                pm.next_step = new_next_step;
                pm.deadline_ts = new_deadline;
            }
            if new_next_step >= self.compiled.steps.len() {
                completions.push(self.in_flight.remove(idx));
            }
        }
        for idx in to_remove {
            self.in_flight.remove(idx);
        }
        for pm in completions {
            self.complete(pm, out);
        }
    }

    fn try_open_new(&mut self, event: &E, ts: i64, out: &mut Vec<M>) {
        let total_steps = self.compiled.steps.len();
        let when_step = &self.compiled.steps[0];
        if !when_step.predicate.evaluate(event) {
            return;
        }
        let when_name = when_step.name.clone();
        let new_pm = PartialMatch {
            captures: vec![(when_name, event.clone())],
            next_step: 1,
            deadline_ts: self.next_deadline_for(1, ts),
        };
        if total_steps == 1 {
            self.complete(new_pm, out);
        } else {
            self.in_flight.push(new_pm);
            if self.in_flight.len() > MAX_IN_FLIGHT {
                self.in_flight.remove(0);
            }
        }
    }
}
