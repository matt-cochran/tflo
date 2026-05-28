//! Streaming state machine for [`Pattern`] matching, plus the iterator
//! adapter that exposes it.
//!
//! The runtime is forward-only and per-key by construction: callers that
//! need per-entity isolation key their stream upstream (via
//! `tflo_core::keyed::*` or just by routing events themselves). Each
//! [`MatchPatternIter`] holds one runtime instance and one
//! `Vec<PartialMatch<E>>` of in-flight partial matches.
//!
//! ## Bounded-state guarantee
//!
//! A new partial match is opened **only** on a positive `when` hit, and an
//! existing partial match is dropped the moment its `within` deadline
//! passes without the required step firing. With a bounded `within`, the
//! number of in-flight partials is bounded by event rate within the
//! window. The hard cap is [`MAX_IN_FLIGHT`] — when exceeded, the oldest
//! partial match is dropped (poka-yoke against runaway buffers on
//! misconfigured patterns).

use crate::matched::Match;
use crate::pattern::Pattern;

/// Maximum number of simultaneous in-flight partial matches per pattern
/// per stream. When this cap is reached, the oldest partial match is
/// dropped to bound memory. Patterns whose semantics tolerate more should
/// either tighten the `within` bound or split into multiple narrower
/// patterns.
pub const MAX_IN_FLIGHT: usize = 1024;

/// One in-flight partial match.
struct PartialMatch<E> {
    /// Events captured so far, paired with their step name.
    captures: Vec<(String, E)>,
    /// Index of the step we're awaiting next.
    next_step: usize,
    /// Event-time deadline for the awaited step. `None` means "no bound."
    deadline_ts: Option<i64>,
}

/// The iterator adapter produced by [`PatternIter::match_pattern`].
///
/// Each `next()` pulls events from the underlying iterator, drives the
/// pattern's state machine, and yields one output value per successful
/// match. On underlying-iterator exhaustion the runtime drains any
/// deadline-resolved negative matches.
pub struct MatchPatternIter<I, E, M>
where
    I: Iterator<Item = E>,
    E: Clone + 'static,
    M: 'static,
{
    inner: I,
    pattern: Pattern<E, M>,
    in_flight: Vec<PartialMatch<E>>,
    pending: std::collections::VecDeque<M>,
    last_ts: i64,
    exhausted: bool,
    drained_negatives: bool,
}

impl<I, E, M> MatchPatternIter<I, E, M>
where
    I: Iterator<Item = E>,
    E: Clone + 'static,
    M: 'static,
{
    fn new(inner: I, pattern: Pattern<E, M>) -> Self {
        Self {
            inner,
            pattern,
            in_flight: Vec::new(),
            pending: std::collections::VecDeque::new(),
            last_ts: i64::MIN,
            exhausted: false,
            drained_negatives: false,
        }
    }

    /// Process one event through the runtime. Pushes any newly completed
    /// matches onto `self.pending`.
    fn process_event(&mut self, event: E) {
        let Some(ts) = self.pattern.timestamp_of(&event) else {
            // No timestamp_fn configured — treat every event as ts=0.
            // Patterns without a timestamp are usable but `within` bounds
            // collapse to "immediate next event."
            self.process_at(event, 0);
            return;
        };
        self.process_at(event, ts);
    }

    fn process_at(&mut self, event: E, ts: i64) {
        self.last_ts = ts;

        // 1) Advance watermark: resolve any deadlines that have passed.
        //    Negative deadlines that pass without a kill = success.
        //    Positive deadlines that pass = drop.
        let mut i = 0;
        while i < self.in_flight.len() {
            let drop_or_emit = match self.in_flight[i].deadline_ts {
                Some(deadline) if ts > deadline => {
                    // Deadline passed — resolve.
                    let step_idx = self.in_flight[i].next_step;
                    let step = &self.pattern.steps()[step_idx];
                    if step.is_negative() {
                        // Negative timer fired without the matching event:
                        // emit a successful match.
                        let pm = self.in_flight.remove(i);
                        self.complete(pm);
                        Resolution::Removed
                    } else {
                        // Positive deadline expired without a match: drop.
                        self.in_flight.remove(i);
                        Resolution::Removed
                    }
                }
                _ => Resolution::Kept,
            };
            if matches!(drop_or_emit, Resolution::Kept) {
                i = i.saturating_add(1);
            }
        }

        // 2) Try advancing existing partial matches with this event.
        //    Walk in reverse so removal indices stay stable.
        let mut indices_to_remove: Vec<usize> = Vec::new();
        let mut completions: Vec<PartialMatch<E>> = Vec::new();
        for idx in (0..self.in_flight.len()).rev() {
            let (step_idx, is_negative, step_name, pred_matches) = {
                let pm = &self.in_flight[idx];
                let step_idx = pm.next_step;
                let step = &self.pattern.steps()[step_idx];
                let pred = step.predicate();
                (
                    step_idx,
                    step.is_negative(),
                    step.name().to_string(),
                    pred(&event),
                )
            };
            if !pred_matches {
                continue;
            }
            if is_negative {
                indices_to_remove.push(idx);
                continue;
            }
            // Positive step matched: capture and advance.
            let new_next_step = step_idx.saturating_add(1);
            let new_deadline = self.next_deadline_for(new_next_step, ts);
            {
                let pm = &mut self.in_flight[idx];
                pm.captures.push((step_name, event.clone()));
                pm.next_step = new_next_step;
                pm.deadline_ts = new_deadline;
            }
            if new_next_step >= self.pattern.steps().len() {
                completions.push(self.in_flight.remove(idx));
            }
        }
        for idx in indices_to_remove {
            self.in_flight.remove(idx);
        }
        for pm in completions {
            self.complete(pm);
        }

        // 3) Open a new partial match if `when` predicate matches.
        //    `when` is steps[0]; a brand-new partial match has
        //    next_step = 1 (waiting on step 1's predicate).
        let when = &self.pattern.steps()[0];
        if when.predicate()(&event) {
            let new_pm = PartialMatch {
                captures: vec![(when.name().to_string(), event.clone())],
                next_step: 1,
                deadline_ts: self.next_deadline_for(1, ts),
            };
            if self.pattern.steps().len() == 1 {
                // Single-step pattern (just `when`) — emit immediately.
                self.complete(new_pm);
            } else {
                self.in_flight.push(new_pm);
                // Bound memory — drop oldest if over the cap.
                if self.in_flight.len() > MAX_IN_FLIGHT {
                    self.in_flight.remove(0);
                }
            }
        }
    }

    fn next_deadline_for(&self, step_idx: usize, ref_ts: i64) -> Option<i64> {
        if step_idx >= self.pattern.steps().len() {
            return None;
        }
        self.pattern.steps()[step_idx]
            .within_ms()
            .map(|ms| ref_ts.saturating_add(ms))
    }

    fn complete(&mut self, pm: PartialMatch<E>) {
        let m = Match::new(self.pattern.name_str(), pm.captures);
        if let Some(out) = self.pattern.emit_with(&m) {
            self.pending.push_back(out);
        }
    }

    /// On end-of-stream, fire any pending negative-step matches whose
    /// deadlines have not yet been reached. Semantically: a never-arriving
    /// closing event is the same as one that never came before the
    /// deadline. Without this, abandoned_cart-style patterns would silently
    /// drop their last match when the underlying iterator runs out.
    fn drain_pending_negatives(&mut self) {
        let mut i = 0;
        while i < self.in_flight.len() {
            let step_idx = self.in_flight[i].next_step;
            let step = &self.pattern.steps()[step_idx];
            if step.is_negative() {
                let pm = self.in_flight.remove(i);
                self.complete(pm);
            } else {
                i = i.saturating_add(1);
            }
        }
    }
}

enum Resolution {
    Kept,
    Removed,
}

impl<I, E, M> Iterator for MatchPatternIter<I, E, M>
where
    I: Iterator<Item = E>,
    E: Clone + 'static,
    M: 'static,
{
    type Item = M;

    fn next(&mut self) -> Option<M> {
        loop {
            if let Some(out) = self.pending.pop_front() {
                return Some(out);
            }
            if self.exhausted {
                if !self.drained_negatives {
                    self.drain_pending_negatives();
                    self.drained_negatives = true;
                    continue;
                }
                return None;
            }
            match self.inner.next() {
                Some(e) => self.process_event(e),
                None => self.exhausted = true,
            }
        }
    }
}

/// Iterator extension trait that enables `.match_pattern(p)` on any
/// `Iterator<Item = E>`.
///
/// Mirrors the style of `tflo-cel`'s `.cel_filter()` and `tflo-rhai`'s
/// `.rhai_filter()` so the surface stays consistent across the crate
/// family.
pub trait PatternIter: Iterator + Sized {
    /// Apply a compiled [`Pattern`] to this iterator, yielding the emit
    /// closure's output per successful match.
    fn match_pattern<M>(
        self,
        pattern: Pattern<Self::Item, M>,
    ) -> MatchPatternIter<Self, Self::Item, M>
    where
        Self::Item: Clone + 'static,
        M: 'static,
    {
        MatchPatternIter::new(self, pattern)
    }
}

impl<I: Iterator + Sized> PatternIter for I {}
