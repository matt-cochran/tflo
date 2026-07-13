// The state machine indexes `steps[next_step]` and partial-match captures
// at positions the engine itself controls. Every index is bounded by
// invariants the same module enforces (next_step ≤ steps.len(), captures
// grow monotonically). Per-site `.get(...)` would obscure the engine's
// own contracts; allowing here with rationale is clearer.
#![allow(
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    clippy::missing_const_for_fn,
    reason = "engine internal: indices are bounded by state-machine invariants \
              enforced in this module; saturating `usize` ops on small step \
              counts cannot overflow."
)]

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

/// Why a partial match was discarded before it could complete.
///
/// Both variants represent *silent loss made observable*: a partial that
/// would otherwise vanish without a trace. Wire a handler with
/// [`Runtime::set_drop_handler`] (or read [`Runtime::dropped_count`]) to
/// surface them.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum DropReason {
    /// The bounded in-flight set ([`MAX_IN_FLIGHT`]) overflowed and the
    /// oldest partial was evicted to make room for a newly-opened one.
    MaxInFlight,
    /// A positive step's `within` deadline elapsed (in event-time) before
    /// the step matched, so the partial was abandoned.
    Deadline,
    /// An event arrived behind the released event-time frontier of the optional
    /// reorder buffer (later than `max_lateness` out of order), so it could not
    /// be matched in event-time order and was dropped.
    Late,
}

/// A predicate evaluable against `&E`. Implementations decide their
/// own storage strategy (Arc, Rc, custom).
pub trait Predicate<E>: 'static {
    /// Test the event against this predicate.
    fn evaluate(&self, event: &E) -> bool;

    /// Context-aware evaluation for non-initial steps: in addition to the
    /// event, the predicate sees the events captured so far in the *current*
    /// partial match (in capture order, as `(step_name, event)` pairs). This
    /// is what enables cross-step correlation — e.g. "a `scroll` whose
    /// `productId` equals the `view`'s". The default ignores the context and
    /// delegates to [`evaluate`](Self::evaluate), so existing predicates are
    /// unaffected. The initial `when` step always uses [`evaluate`](Self::evaluate)
    /// (it has no prior captures).
    fn evaluate_in_context(&self, event: &E, _captures: &[(String, E)]) -> bool {
        self.evaluate(event)
    }
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

/// How a positive step consumes the event stream relative to the previous
/// capture.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Contiguity {
    /// Skip-till-match: events that don't satisfy the step are skipped; the
    /// step matches the next *eventual* event that does. This is the default
    /// (`then`) semantics.
    #[default]
    Eventually,
    /// Strict next: the event *immediately* following the previous capture must
    /// satisfy the step, otherwise the partial match is discarded. Lets a
    /// pattern express "B must directly follow A" / "no event between A and B".
    Next,
}

/// A `repeated(min..=max, pred)` quantifier on a step: the step matches its
/// predicate between `min` and `max` times (capturing each), then advances.
/// Invariant (enforced by the builder): `1 <= min <= max`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct RepeatSpec {
    /// Minimum number of matches required before the step can complete.
    pub min: usize,
    /// Maximum number of matches consumed before the step completes greedily.
    pub max: usize,
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
    /// Stream-contiguity for this (positive) step. Ignored for negatives.
    pub contiguity: Contiguity,
    /// Optional `repeated(min..=max)` quantifier. `None` = match exactly once.
    pub repeat: Option<RepeatSpec>,
    /// Optional **interior-negation guard** (positive steps only). While the
    /// partial skips events waiting for this step, any event that matches
    /// `forbidden` (and is not itself this step's match) kills the partial.
    /// This expresses "A then B with NO C in between" — a `not_between(C)`
    /// guard on the B step — which a *terminal* `not_then` cannot.
    pub forbidden: Option<P>,
    /// `PhantomData` to keep `E` in the type.
    _marker: core::marker::PhantomData<fn(&E) -> bool>,
}

impl<E, P> Step<E, P> {
    /// Construct a positive step (default: eventually-contiguous, match once).
    pub fn positive(name: impl Into<String>, predicate: P, within_ms: Option<i64>) -> Self {
        Self {
            name: name.into(),
            predicate,
            within_ms,
            is_negative: false,
            contiguity: Contiguity::Eventually,
            repeat: None,
            forbidden: None,
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
            contiguity: Contiguity::Eventually,
            repeat: None,
            forbidden: None,
            _marker: core::marker::PhantomData,
        }
    }

    /// Set this step's stream-contiguity (builder-style).
    #[must_use]
    pub fn with_contiguity(mut self, c: Contiguity) -> Self {
        self.contiguity = c;
        self
    }

    /// Attach a `repeated(min..=max)` quantifier (builder-style).
    #[must_use]
    pub fn with_repeat(mut self, spec: RepeatSpec) -> Self {
        self.repeat = Some(spec);
        self
    }

    /// Attach an interior-negation guard (builder-style). See [`Step::forbidden`].
    #[must_use]
    pub fn with_forbidden(mut self, predicate: P) -> Self {
        self.forbidden = Some(predicate);
        self
    }
}

/// One in-flight partial match.
struct PartialMatch<E> {
    captures: Vec<(String, E)>,
    next_step: usize,
    deadline_ts: Option<i64>,
    /// Matches accumulated at the current `repeated` step (0 for plain steps).
    repeat_count: usize,
}

/// What to do with a partial match after processing one event.
enum Disposition {
    /// Keep it in flight (advanced, still repeating, or skipped this event).
    Keep,
    /// Discard it (cancelled negative, or broken `Next`-contiguity run).
    Remove,
    /// It reached the final step — emit and drop.
    Complete,
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
    /// `PhantomData` for M.
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
    /// Optional observer invoked once per dropped partial (see [`DropReason`]).
    on_drop: Option<Box<dyn FnMut(DropReason)>>,
    /// Monotonic count of partials dropped over this runtime's lifetime.
    dropped: usize,
    /// Optional event-time reorder buffer (opt-in). When set, `push`/`tick`/`flush`
    /// route events through it so the matcher sees them in event-time order even
    /// when they *arrive* out of order; events later than `max_lateness` out of
    /// order drop as [`DropReason::Late`]. The matching algorithm is unchanged —
    /// the buffer only reorders the *input*.
    reorder: Option<tflo_core::reorder::ReorderBuffer<E>>,
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
            on_drop: None,
            dropped: 0,
            reorder: None,
        }
    }

    /// Enable an event-time **reorder buffer** tolerating up to `max_lateness_ms`
    /// of out-of-order arrival. Events are released to the matcher in event-time
    /// order; the watermark advances off [`tick`](Self::tick) so a quiet stream
    /// still drains. Opt-in (off by default) — reordering trades latency for
    /// out-of-order correctness, which most in-order live streams don't need.
    #[must_use]
    pub fn with_reorder(mut self, max_lateness_ms: i64) -> Self {
        self.reorder = Some(tflo_core::reorder::ReorderBuffer::new(max_lateness_ms));
        self
    }

    /// Reset to the just-constructed state. The compiled pattern is
    /// preserved; in-flight partial matches are dropped.
    ///
    /// The drop handler and lifetime drop counter are **not** cleared —
    /// they observe the runtime across resets. (Resetting does not itself
    /// count as dropping partials.)
    pub fn reset(&mut self) {
        self.in_flight.clear();
    }

    /// Register a handler invoked once for every partial match the runtime
    /// discards (see [`DropReason`]). Replaces any previously-set handler.
    /// The handler fires *in addition to* incrementing
    /// [`dropped_count`](Self::dropped_count).
    pub fn set_drop_handler(&mut self, f: impl FnMut(DropReason) + 'static) {
        self.on_drop = Some(Box::new(f));
    }

    /// Total number of partial matches dropped over this runtime's lifetime,
    /// across both [`DropReason`] variants. Monotonic; survives [`reset`](Self::reset).
    #[must_use]
    pub fn dropped_count(&self) -> usize {
        self.dropped
    }

    /// Record one dropped partial: bump the lifetime counter and notify the
    /// handler if one is set. Call this at *every* site that silently
    /// discards an in-flight partial.
    fn note_drop(&mut self, reason: DropReason) {
        self.dropped = self.dropped.saturating_add(1);
        if let Some(handler) = self.on_drop.as_mut() {
            handler(reason);
        }
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
    ///
    /// With a reorder buffer enabled ([`with_reorder`](Self::with_reorder)), the
    /// event is buffered and only released to the matcher once event-time order is
    /// safe; an event too far out of order drops as [`DropReason::Late`].
    pub fn push_at(&mut self, event: E, ts: i64) -> Vec<M> {
        let Some(buf) = self.reorder.as_mut() else {
            let mut out = Vec::new();
            self.match_one(&event, ts, &mut out);
            return out;
        };
        let released = buf.step(ts, event);
        let late = buf.take_late().len();
        for _ in 0..late {
            self.note_drop(DropReason::Late);
        }
        let mut out = Vec::new();
        for (ts2, ev2) in released {
            self.match_one(&ev2, ts2, &mut out);
        }
        out
    }

    /// Run one event through the matcher at `ts` (deadlines → partials → open).
    /// The pure matching step; the reorder buffer, when present, only governs
    /// *which* events reach here and *in what order*.
    fn match_one(&mut self, event: &E, ts: i64, out: &mut Vec<M>) {
        self.advance_deadlines(ts, out);
        self.advance_partials(event, ts, out);
        self.try_open_new(event, ts, out);
    }

    /// Drain pending negative-step matches whose deadlines have not
    /// elapsed yet. Semantically: end-of-stream is treated like a
    /// deadline that never closed.
    pub fn flush(&mut self) -> Vec<M> {
        let mut out = Vec::new();
        // Drain the reorder buffer first (end-of-stream releases everything left,
        // in event-time order), then resolve pending negatives/repeats.
        if let Some(buf) = self.reorder.as_mut() {
            let released = buf.flush();
            for (ts2, ev2) in released {
                self.match_one(&ev2, ts2, &mut out);
            }
        }
        let mut i = 0;
        while i < self.in_flight.len() {
            let step_idx = self.in_flight[i].next_step;
            let step = &self.compiled.steps[step_idx];
            let is_last = step_idx.saturating_add(1) >= self.compiled.steps.len();
            // A terminal `repeated` step that has met its `min` completes at
            // end-of-stream (the run simply ended) — same treatment a negative
            // gets when its deadline never closes.
            let satisfied_repeat = !step.is_negative
                && is_last
                && step
                    .repeat
                    .is_some_and(|spec| self.in_flight[i].repeat_count >= spec.min);
            if step.is_negative || satisfied_repeat {
                let pm = self.in_flight.remove(i);
                self.complete(pm, &mut out);
            } else {
                i = i.saturating_add(1);
            }
        }
        out
    }

    /// Advance logical time to `now` **without** consuming an event, firing any
    /// deadline-reached derived matches. This is what makes "A then B within T,
    /// else fire" self-driving: a negative step whose window has closed emits its
    /// match on absence, and a positive partial whose window closed is dropped
    /// observably — exactly the `advance_deadlines`
    /// behaviour `push_at` runs, but driven by a clock rather than an event.
    ///
    /// Purity is preserved: `now` is an explicit input, so the engine reads no
    /// wall clock and a `tick`-driven run is byte-identical to an event-driven or
    /// batch run over the same logical timeline (replay + cross-tier parity).
    /// Idempotent: a second `tick` at the same `now` (or a later `push_at`) finds
    /// the already-fired partials gone, so nothing double-fires.
    pub fn tick(&mut self, now: i64) -> Vec<M> {
        let mut out = Vec::new();
        // With a reorder buffer, advancing the clock advances the watermark first,
        // releasing any events that have now aged past `max_lateness` — in
        // event-time order — before the deadline sweep.
        if let Some(buf) = self.reorder.as_mut() {
            let released = buf.advance_watermark(now);
            for (ts2, ev2) in released {
                self.match_one(&ev2, ts2, &mut out);
            }
        }
        self.advance_deadlines(now, &mut out);
        out
    }

    /// The earliest pending deadline across all in-flight partials, or `None` if
    /// nothing is waiting on a timer. A driver schedules its next [`tick`](Self::tick)
    /// for this instant (the browser/host clock adapter coalesces to this one
    /// wake-up). O(in-flight); with the bounded in-flight set this is cheap.
    #[must_use]
    pub fn next_deadline(&self) -> Option<i64> {
        let match_dl = self.in_flight.iter().filter_map(|pm| pm.deadline_ts).min();
        // A buffered event needs a tick when it ages past the lateness window, so
        // the driver must also wake for the buffer's next release instant.
        let release = self
            .reorder
            .as_ref()
            .and_then(tflo_core::reorder::ReorderBuffer::next_release_clock);
        match (match_dl, release) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (a, b) => a.or(b),
        }
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
            let expired = self.in_flight[i].deadline_ts.is_some_and(|d| ts > d);
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
                // A positive step's `within` window closed before it matched —
                // the partial is abandoned. Make that loss observable.
                self.in_flight.remove(i);
                self.note_drop(DropReason::Deadline);
            }
        }
    }

    fn advance_partials(&mut self, event: &E, ts: i64, out: &mut Vec<M>) {
        // Reverse order so `remove(idx)` never shifts an index we still need.
        for idx in (0..self.in_flight.len()).rev() {
            match self.advance_one(idx, event, ts) {
                Disposition::Keep => {}
                Disposition::Remove => {
                    self.in_flight.remove(idx);
                }
                Disposition::Complete => {
                    let pm = self.in_flight.remove(idx);
                    self.complete(pm, out);
                }
            }
        }
    }

    /// Process one event against the partial at `idx`, applying repeat counting
    /// and `Next`/`Eventually` contiguity. The inner loop handles the single
    /// re-process a `repeated` step needs: when a non-matching event ends a
    /// satisfied repetition, that same event is offered to the following step.
    /// It is bounded — only a repeat early-exit loops, and the next step can
    /// itself early-exit at most once (a fresh repeat needs `min >= 1` matches).
    fn advance_one(&mut self, idx: usize, event: &E, ts: i64) -> Disposition {
        loop {
            let pm = &self.in_flight[idx];
            let step_idx = pm.next_step;
            let step = &self.compiled.steps[step_idx];
            // Non-initial steps see the captures so far, enabling cross-step
            // correlation (e.g. `B.id == A.id`). The default impl ignores them.
            let matches = step.predicate.evaluate_in_context(event, &pm.captures);
            let is_negative = step.is_negative;
            let contiguity = step.contiguity;
            let repeat = step.repeat;
            let step_name = step.name.clone();
            // Interior-negation guard: an event that doesn't satisfy this step
            // but matches its `forbidden` predicate kills the partial ("no C
            // between"). Evaluated in-context so the guard can correlate (e.g.
            // a success on the SAME card). Negatives carry no guard.
            let forbidden_matches = step
                .forbidden
                .as_ref()
                .is_some_and(|f| f.evaluate_in_context(event, &pm.captures));

            if is_negative {
                // A matching event cancels the (negative) partial; otherwise the
                // deadline path decides it. Negatives don't repeat.
                return if matches {
                    Disposition::Remove
                } else {
                    Disposition::Keep
                };
            }

            // The awaited positive takes precedence (it closes the interval); a
            // forbidden event seen while still waiting kills the partial.
            if !matches && forbidden_matches {
                return Disposition::Remove;
            }

            match repeat {
                Some(spec) => {
                    if matches {
                        let pm = &mut self.in_flight[idx];
                        pm.captures.push((step_name, event.clone()));
                        pm.repeat_count = pm.repeat_count.saturating_add(1);
                        // Greedy cap reached → this event is consumed; advance.
                        if pm.repeat_count >= spec.max {
                            return self.advance_step(idx, step_idx, ts);
                        }
                        return Disposition::Keep; // still repeating; event consumed
                    }
                    // Non-matching event.
                    if pm.repeat_count >= spec.min {
                        // Repetition satisfied → advance, then REPROCESS this
                        // (still-unconsumed) event against the following step.
                        match self.advance_step(idx, step_idx, ts) {
                            Disposition::Keep => continue,
                            Disposition::Remove => return Disposition::Remove,
                            Disposition::Complete => return Disposition::Complete,
                        }
                    }
                    // Not enough matches yet.
                    return if contiguity == Contiguity::Next {
                        Disposition::Remove // a broken contiguous run dies
                    } else {
                        Disposition::Keep // eventually: wait for more matches
                    };
                }
                None => {
                    if matches {
                        let pm = &mut self.in_flight[idx];
                        pm.captures.push((step_name, event.clone()));
                        return self.advance_step(idx, step_idx, ts);
                    }
                    return if contiguity == Contiguity::Next {
                        Disposition::Remove // strict next: intervening event kills it
                    } else {
                        Disposition::Keep // eventually: skip and wait
                    };
                }
            }
        }
    }

    /// Advance the partial at `idx` past `from_step` to the next step, resetting
    /// the repeat counter and (re)arming the deadline. Returns `Complete` if the
    /// pattern is now fully matched.
    fn advance_step(&mut self, idx: usize, from_step: usize, ts: i64) -> Disposition {
        let new_next = from_step.saturating_add(1);
        let new_deadline = self.next_deadline_for(new_next, ts);
        {
            let pm = &mut self.in_flight[idx];
            pm.next_step = new_next;
            pm.repeat_count = 0;
            pm.deadline_ts = new_deadline;
        }
        if new_next >= self.compiled.steps.len() {
            Disposition::Complete
        } else {
            Disposition::Keep
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
            repeat_count: 0,
        };
        if total_steps == 1 {
            self.complete(new_pm, out);
        } else {
            self.in_flight.push(new_pm);
            if self.in_flight.len() > MAX_IN_FLIGHT {
                // Bounded-by-construction eviction: the oldest partial is
                // dropped to make room. Make that loss observable.
                self.in_flight.remove(0);
                self.note_drop(DropReason::MaxInFlight);
            }
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod ctx_tests {
    use super::*;

    #[derive(Clone, Debug)]
    struct Ev {
        kind: &'static str,
        id: &'static str,
        ts: i64,
    }

    // One predicate type for all steps (the engine uses a single `P`). The
    // `then` variant correlates across steps via the captures.
    enum Pred {
        WhenView,
        ScrollSameProduct,
    }
    impl Predicate<Ev> for Pred {
        fn evaluate(&self, e: &Ev) -> bool {
            match self {
                Self::WhenView => e.kind == "view",
                Self::ScrollSameProduct => e.kind == "scroll",
            }
        }
        fn evaluate_in_context(&self, e: &Ev, caps: &[(String, Ev)]) -> bool {
            match self {
                Self::WhenView => self.evaluate(e),
                // a `scroll` whose product id equals the captured `view`'s id
                Self::ScrollSameProduct => {
                    e.kind == "scroll" && caps.first().is_some_and(|(_, v)| v.id == e.id)
                }
            }
        }
    }

    struct EmitId;
    impl EmitCallback<Ev, String> for EmitId {
        fn emit(&self, m: &Match<Ev>) -> String {
            m.first().id.to_string()
        }
    }
    struct Ts;
    impl TimestampCallback<Ev> for Ts {
        fn timestamp(&self, e: &Ev) -> i64 {
            e.ts
        }
    }

    // "engaged" = view, then a scroll on the SAME product within 5s. Two
    // products are interleaved through a SINGLE runtime; cross-step
    // correlation keeps each product's match independent — no partitioning.
    #[test]
    fn cross_step_correlation_matches_same_product_only() {
        let steps = vec![
            Step::positive("view", Pred::WhenView, None),
            Step::positive("scroll", Pred::ScrollSameProduct, Some(5000)),
        ];
        let compiled = Compiled::new("engaged", steps, EmitId, Some(Ts));
        let mut rt = Runtime::new(compiled);

        let mut out = Vec::new();
        out.extend(rt.push(Ev {
            kind: "view",
            id: "P1",
            ts: 0,
        }));
        out.extend(rt.push(Ev {
            kind: "view",
            id: "P2",
            ts: 100,
        }));
        out.extend(rt.push(Ev {
            kind: "scroll",
            id: "P2",
            ts: 200,
        })); // matches P2
        out.extend(rt.push(Ev {
            kind: "scroll",
            id: "P1",
            ts: 300,
        })); // matches P1

        assert_eq!(out, vec!["P2".to_string(), "P1".to_string()]);
    }

    // Without correlation, a scroll on a DIFFERENT product must not match.
    #[test]
    fn cross_step_correlation_rejects_other_product() {
        let steps = vec![
            Step::positive("view", Pred::WhenView, None),
            Step::positive("scroll", Pred::ScrollSameProduct, Some(5000)),
        ];
        let compiled = Compiled::new("engaged", steps, EmitId, Some(Ts));
        let mut rt = Runtime::new(compiled);

        let mut out = Vec::new();
        out.extend(rt.push(Ev {
            kind: "view",
            id: "P1",
            ts: 0,
        }));
        out.extend(rt.push(Ev {
            kind: "scroll",
            id: "P2",
            ts: 100,
        })); // different product
        out.extend(rt.flush());

        assert!(out.is_empty());
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod quant_tests {
    use super::*;

    #[derive(Clone, Debug)]
    struct Ev {
        kind: &'static str,
        ts: i64,
    }

    /// A predicate that matches one event `kind`.
    struct Kind(&'static str);
    impl Predicate<Ev> for Kind {
        fn evaluate(&self, e: &Ev) -> bool {
            e.kind == self.0
        }
    }

    struct CountEmit;
    impl EmitCallback<Ev, usize> for CountEmit {
        fn emit(&self, m: &Match<Ev>) -> usize {
            m.len()
        }
    }
    struct Ts;
    impl TimestampCallback<Ev> for Ts {
        fn timestamp(&self, e: &Ev) -> i64 {
            e.ts
        }
    }

    fn run(steps: Vec<Step<Ev, Kind>>, events: &[(&'static str, i64)]) -> Vec<usize> {
        let compiled = Compiled::new("p", steps, CountEmit, Some(Ts));
        let mut rt = Runtime::new(compiled);
        let mut out = Vec::new();
        for &(kind, ts) in events {
            out.extend(rt.push(Ev { kind, ts }));
        }
        out.extend(rt.flush());
        out
    }

    // ── Contiguity ───────────────────────────────────────────────────────

    #[test]
    fn strict_next_dies_on_intervening_event() {
        // a, then(Next) b — an event between a and b breaks the match.
        let steps = vec![
            Step::positive("a", Kind("a"), None),
            Step::positive("b", Kind("b"), None).with_contiguity(Contiguity::Next),
        ];
        // a, x, b → the `x` is the immediate next event after `a`, not `b` → dies.
        assert!(run(steps, &[("a", 0), ("x", 1), ("b", 2)]).is_empty());
    }

    #[test]
    fn strict_next_matches_when_immediate() {
        let steps = vec![
            Step::positive("a", Kind("a"), None),
            Step::positive("b", Kind("b"), None).with_contiguity(Contiguity::Next),
        ];
        assert_eq!(run(steps, &[("a", 0), ("b", 1)]), vec![2]);
    }

    #[test]
    fn eventually_skips_intervening_event() {
        // Default contiguity: the `x` between `a` and `b` is skipped → matches.
        let steps = vec![
            Step::positive("a", Kind("a"), None),
            Step::positive("b", Kind("b"), None),
        ];
        assert_eq!(run(steps, &[("a", 0), ("x", 1), ("b", 2)]), vec![2]);
    }

    // ── repeated(min..=max) ──────────────────────────────────────────────

    fn rep(min: usize, max: usize) -> RepeatSpec {
        RepeatSpec { min, max }
    }

    #[test]
    fn repeated_exactly_n_terminal() {
        // a, then exactly 3 b's (terminal). 4 captures (a + 3 b) on the 3rd b.
        let steps = vec![
            Step::positive("a", Kind("a"), None),
            Step::positive("b", Kind("b"), None).with_repeat(rep(3, 3)),
        ];
        assert_eq!(
            run(steps, &[("a", 0), ("b", 1), ("b", 2), ("b", 3)]),
            vec![4]
        );
        // Only 2 b's → never reaches min=3 → no emit.
        let steps = vec![
            Step::positive("a", Kind("a"), None),
            Step::positive("b", Kind("b"), None).with_repeat(rep(3, 3)),
        ];
        assert!(run(steps, &[("a", 0), ("b", 1), ("b", 2)]).is_empty());
    }

    #[test]
    fn repeated_caps_at_max_then_advances() {
        // a, 2..=2 b, then c. A third b after the cap is skipped; c completes.
        let steps = vec![
            Step::positive("a", Kind("a"), None),
            Step::positive("b", Kind("b"), None).with_repeat(rep(2, 2)),
            Step::positive("c", Kind("c"), None),
        ];
        // a b b (cap) b(skipped) c → captures a,b,b,c = 4.
        assert_eq!(
            run(steps, &[("a", 0), ("b", 1), ("b", 2), ("b", 3), ("c", 4)]),
            vec![4]
        );
    }

    #[test]
    fn repeated_early_exit_reprocesses_terminating_event() {
        // a, 1..=3 b, then c. The `c` ends the b-run (count=2 >= min=1) AND is
        // matched as the next step in the SAME push → a,b,b,c = 4 captures.
        let steps = vec![
            Step::positive("a", Kind("a"), None),
            Step::positive("b", Kind("b"), None).with_repeat(rep(1, 3)),
            Step::positive("c", Kind("c"), None),
        ];
        assert_eq!(
            run(steps, &[("a", 0), ("b", 1), ("b", 2), ("c", 3)]),
            vec![4]
        );
    }

    // ── Observable drops (C2) ────────────────────────────────────────────

    #[test]
    fn max_in_flight_overflow_is_observed() {
        use std::cell::RefCell;
        use std::rc::Rc;

        // Two positive steps so opened partials stay in flight (a single-step
        // pattern would complete-and-drop immediately, never accumulating).
        let steps = vec![
            Step::positive("a", Kind("a"), None),
            Step::positive("b", Kind("b"), None),
        ];
        let compiled = Compiled::new("p", steps, CountEmit, Some(Ts));
        let mut rt = Runtime::new(compiled);

        let reasons: Rc<RefCell<Vec<DropReason>>> = Rc::new(RefCell::new(Vec::new()));
        let sink = reasons.clone();
        rt.set_drop_handler(move |r| sink.borrow_mut().push(r));

        // Open MAX_IN_FLIGHT + 5 partials: each "a" opens one and none ever
        // see their "b", so the in-flight set saturates and then evicts.
        let overflow = 5;
        for ts in 0..(MAX_IN_FLIGHT + overflow) as i64 {
            let _ = rt.push(Ev { kind: "a", ts });
        }

        assert_eq!(rt.dropped_count(), overflow);
        let got = reasons.borrow();
        assert_eq!(got.len(), overflow);
        assert!(got.iter().all(|&r| r == DropReason::MaxInFlight));
    }

    #[test]
    fn positive_deadline_drop_is_observed() {
        use std::cell::RefCell;
        use std::rc::Rc;

        // a, then b within 100ms. If b never arrives in time the partial is
        // dropped on the deadline — and must be observed.
        let steps = vec![
            Step::positive("a", Kind("a"), None),
            Step::positive("b", Kind("b"), Some(100)),
        ];
        let compiled = Compiled::new("p", steps, CountEmit, Some(Ts));
        let mut rt = Runtime::new(compiled);

        let reasons: Rc<RefCell<Vec<DropReason>>> = Rc::new(RefCell::new(Vec::new()));
        let sink = reasons.clone();
        rt.set_drop_handler(move |r| sink.borrow_mut().push(r));

        let _ = rt.push(Ev { kind: "a", ts: 0 }); // opens the partial, deadline = 100
        // A later event past the deadline triggers the deadline sweep; it is
        // not a `b`, so the partial expires rather than completing.
        let out = rt.push(Ev { kind: "x", ts: 500 });
        assert!(out.is_empty());

        assert_eq!(rt.dropped_count(), 1);
        assert_eq!(*reasons.borrow(), vec![DropReason::Deadline]);
    }

    #[test]
    fn no_drop_handler_still_counts() {
        // Handler-less runtimes must still count drops (the read accessor is
        // independent of the callback).
        let steps = vec![
            Step::positive("a", Kind("a"), None),
            Step::positive("b", Kind("b"), Some(100)),
        ];
        let compiled = Compiled::new("p", steps, CountEmit, Some(Ts));
        let mut rt = Runtime::new(compiled);
        let _ = rt.push(Ev { kind: "a", ts: 0 });
        let _ = rt.push(Ev { kind: "x", ts: 500 });
        assert_eq!(rt.dropped_count(), 1);
    }

    #[test]
    fn repeated_terminal_flushes_when_min_met() {
        // a, then 2..=5 b (terminal). Stream ends after 3 b's with no closing
        // event → flush completes it (count=3 >= min=2). 4 captures.
        let steps = vec![
            Step::positive("a", Kind("a"), None),
            Step::positive("b", Kind("b"), None).with_repeat(rep(2, 5)),
        ];
        assert_eq!(
            run(steps, &[("a", 0), ("b", 1), ("b", 2), ("b", 3)]),
            vec![4]
        );
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tick_tests {
    use super::*;

    #[derive(Clone, Debug)]
    struct Ev {
        kind: &'static str,
        ts: i64,
    }

    struct Kind(&'static str);
    impl Predicate<Ev> for Kind {
        fn evaluate(&self, e: &Ev) -> bool {
            e.kind == self.0
        }
    }
    struct EmitFirstTs;
    impl EmitCallback<Ev, i64> for EmitFirstTs {
        fn emit(&self, m: &Match<Ev>) -> i64 {
            m.first().ts
        }
    }
    struct Ts;
    impl TimestampCallback<Ev> for Ts {
        fn timestamp(&self, e: &Ev) -> i64 {
            e.ts
        }
    }

    /// "abandoned" = `a` then NO `b` within 5s — the negative-within shape the
    /// self-driving clock exists to fire on absence.
    fn abandoned() -> Compiled<Ev, i64, Kind, EmitFirstTs, Ts> {
        Compiled::new(
            "abandoned",
            vec![
                Step::positive("a", Kind("a"), None),
                Step::negative("b", Kind("b"), Some(5_000)),
            ],
            EmitFirstTs,
            Some(Ts),
        )
    }

    #[test]
    fn tick_fires_abandonment_on_absence_with_no_event() {
        let mut rt = Runtime::new(abandoned());
        // `a` opens the partial; nothing else ever arrives.
        assert!(rt.push(Ev { kind: "a", ts: 0 }).is_empty());
        // The pending timer is the negative step's window close.
        assert_eq!(rt.next_deadline(), Some(5_000));
        // A clock tick past the deadline fires the abandonment — no event needed.
        assert_eq!(rt.tick(6_000), vec![0]);
        // The partial is gone; no timer pending.
        assert_eq!(rt.next_deadline(), None);
    }

    #[test]
    fn tick_is_idempotent_and_does_not_double_fire() {
        let mut rt = Runtime::new(abandoned());
        let _ = rt.push(Ev { kind: "a", ts: 0 });
        assert_eq!(rt.tick(6_000), vec![0]);
        // A second tick, and a later real event, emit nothing more.
        assert!(rt.tick(7_000).is_empty());
        assert!(
            rt.push(Ev {
                kind: "b",
                ts: 8_000
            })
            .is_empty()
        );
    }

    #[test]
    fn next_deadline_reports_the_earliest_across_partials() {
        let mut rt = Runtime::new(abandoned());
        let _ = rt.push(Ev { kind: "a", ts: 0 }); // deadline 5_000
        let _ = rt.push(Ev {
            kind: "a",
            ts: 1_000,
        }); // deadline 6_000
        assert_eq!(rt.next_deadline(), Some(5_000));
    }

    /// Replay equivalence: a `tick(now)` is exactly a no-op event at `now`.
    /// Driving the deadline with a trailing (non-matching) event vs. with a clock
    /// tick yields byte-identical output — the property replay + cross-tier parity
    /// rest on.
    #[test]
    fn tick_driven_equals_event_driven() {
        // Event-driven: a trailing non-matching event advances time past the deadline.
        let mut a = Runtime::new(abandoned());
        let mut ev_out = Vec::new();
        ev_out.extend(a.push(Ev { kind: "a", ts: 0 }));
        ev_out.extend(a.push(Ev {
            kind: "z",
            ts: 6_000,
        }));
        ev_out.extend(a.flush());

        // Tick-driven: same logical timeline, no trailing event.
        let mut b = Runtime::new(abandoned());
        let mut tick_out = Vec::new();
        tick_out.extend(b.push(Ev { kind: "a", ts: 0 }));
        tick_out.extend(b.tick(6_000));
        tick_out.extend(b.flush());

        assert_eq!(ev_out, tick_out);
        assert_eq!(ev_out, vec![0]);
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod reorder_tests {
    use super::*;
    use std::cell::RefCell;
    use std::rc::Rc;

    #[derive(Clone, Debug)]
    struct Ev {
        kind: &'static str,
        ts: i64,
    }
    struct Kind(&'static str);
    impl Predicate<Ev> for Kind {
        fn evaluate(&self, e: &Ev) -> bool {
            e.kind == self.0
        }
    }
    struct CountEmit;
    impl EmitCallback<Ev, usize> for CountEmit {
        fn emit(&self, m: &Match<Ev>) -> usize {
            m.len()
        }
    }
    struct Ts;
    impl TimestampCallback<Ev> for Ts {
        fn timestamp(&self, e: &Ev) -> i64 {
            e.ts
        }
    }

    // "a then b" — b must follow a in EVENT-TIME.
    fn ab() -> Compiled<Ev, usize, Kind, CountEmit, Ts> {
        Compiled::new(
            "ab",
            vec![
                Step::positive("a", Kind("a"), None),
                Step::positive("b", Kind("b"), None),
            ],
            CountEmit,
            Some(Ts),
        )
    }

    #[test]
    fn out_of_order_without_reorder_misses() {
        // Arrival order b@10 then a@0 (out of order). The arrival-order matcher
        // sees b first (no partial open) and never matches — the silent bug.
        let mut rt = Runtime::new(ab());
        let mut out = Vec::new();
        out.extend(rt.push(Ev { kind: "b", ts: 10 }));
        out.extend(rt.push(Ev { kind: "a", ts: 0 }));
        out.extend(rt.flush());
        assert!(out.is_empty());
    }

    #[test]
    fn out_of_order_with_reorder_matches() {
        // Same out-of-order arrival, but the reorder buffer releases in event-time
        // order (a@0, b@10) → the pattern matches.
        let mut rt = Runtime::new(ab()).with_reorder(100);
        let mut out = Vec::new();
        out.extend(rt.push(Ev { kind: "b", ts: 10 })); // buffered
        out.extend(rt.push(Ev { kind: "a", ts: 0 })); // buffered
        out.extend(rt.flush()); // released a@0 then b@10
        assert_eq!(out, vec![2]);
    }

    #[test]
    fn event_behind_frontier_drops_as_late() {
        let mut rt = Runtime::new(ab()).with_reorder(0); // greedy release
        let lates = Rc::new(RefCell::new(0usize));
        let l = Rc::clone(&lates);
        rt.set_drop_handler(move |r| {
            if r == DropReason::Late {
                *l.borrow_mut() += 1;
            }
        });
        let _ = rt.push(Ev { kind: "a", ts: 100 }); // releases, frontier = 100
        let _ = rt.push(Ev { kind: "b", ts: 50 }); // 50 < 100 → late drop
        assert_eq!(*lates.borrow(), 1);
        assert_eq!(rt.dropped_count(), 1);
    }
}
