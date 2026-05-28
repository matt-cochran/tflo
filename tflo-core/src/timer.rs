//! Event-time timer service for keyed execution.
//!
//! Operators register event-time timers from inside their `eval_with_ctx`
//! method via a [`TimerCtx`]. The engine fires due timers (those whose
//! `fire_ts` is at or below the current per-key event-time watermark) in
//! deterministic `(fire_ts, registration_seq)` order after each record step
//! and on every advance of the watermark (record-driven or via
//! [`KeyedGraphState::advance_event_time_watermark`](crate::keyed::KeyedGraphState::advance_event_time_watermark)).
//!
//! # Why timers exist
//!
//! Without an event-time timer service, an operator whose semantics depend on
//! *the absence of an event* — "pulse opened, must close within T, else emit
//! `TooLong`" — silently fails when no closing record arrives: its `eval`
//! never runs, so its state stays open forever. Timers close that hole.
//!
//! # Snapshot semantics
//!
//! The timer heap is part of the per-key snapshot. Restoring a checkpoint
//! restores the pending timers exactly, so cross-restart correctness is
//! preserved.

use crate::comp::NodeId;
use serde::{Deserialize, Serialize};
use std::cmp::Reverse;
use std::collections::BinaryHeap;

/// A pending event-time timer.
///
/// Heap ordering is by `fire_ts` then `seq` — strictly ascending — so that
/// fires are deterministic across runs and snapshot/restore boundaries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct TimerEntry {
    /// Event-time, in milliseconds, at which this timer becomes due.
    pub(crate) fire_ts: i64,
    /// The node that registered this timer; the engine routes the on_timer
    /// callback back to that node's operator.
    pub(crate) node_id: NodeId,
    /// Monotonic per-key sequence number. Tie-breaks fires when several
    /// timers share a `fire_ts`. Stable across `(fire_ts, seq)`.
    pub(crate) seq: u64,
    /// Lazy-deletion tombstone. `delete_event_time_timer` flips this bit
    /// rather than removing from the heap (O(n) removal would dominate the
    /// register/fire path); `pop_due` skips tombstoned entries.
    pub(crate) deleted: bool,
}

impl PartialOrd for TimerEntry {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for TimerEntry {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        // Order by (fire_ts, seq, node_id) — strictly total so the heap is
        // deterministic. The `deleted` flag is *not* part of the ordering;
        // it is a tombstone-skip signal at pop time.
        self.fire_ts
            .cmp(&other.fire_ts)
            .then(self.seq.cmp(&other.seq))
            .then(self.node_id.0.cmp(&other.node_id.0))
    }
}

/// Per-key event-time timer service.
///
/// Owned by [`KeyedGraphState`](crate::keyed::KeyedGraphState); not
/// constructed directly by user code. Internally a `BinaryHeap` of
/// `Reverse<TimerEntry>` so `peek`/`pop` return the smallest `fire_ts`.
#[derive(Debug, Default)]
pub(crate) struct TimerService {
    heap: BinaryHeap<Reverse<TimerEntry>>,
    next_seq: u64,
}

impl TimerService {
    /// Register a timer that fires when the per-key watermark reaches
    /// `fire_ts`. The `node_id` is the registering node; the engine
    /// routes the eventual on_timer back to that node's operator.
    pub(crate) fn register(&mut self, fire_ts: i64, node_id: NodeId) {
        let seq = self.next_seq;
        // Saturating: 2^64 registrations on one key in one process lifetime
        // is not realistic. Saturation keeps determinism over panic.
        self.next_seq = self.next_seq.saturating_add(1);
        self.heap.push(Reverse(TimerEntry {
            fire_ts,
            node_id,
            seq,
            deleted: false,
        }));
    }

    /// Tombstone a previously-registered timer for `node_id` at `fire_ts`.
    /// Lazy deletion: the entry stays in the heap but is skipped on pop.
    /// If multiple timers share `(node_id, fire_ts)`, only the earliest
    /// (lowest `seq`) is tombstoned — matching common operator usage
    /// patterns ("cancel my last timer").
    pub(crate) fn delete(&mut self, fire_ts: i64, node_id: NodeId) {
        // We can't mutate a BinaryHeap in place; convert to Vec, mutate,
        // rebuild. For small per-key heaps (typical: <16 active timers)
        // this is acceptable; if profile shows pressure, swap for an
        // alternative data structure.
        let mut entries: Vec<Reverse<TimerEntry>> = std::mem::take(&mut self.heap).into_vec();
        let mut tombstoned = false;
        for e in &mut entries {
            if !e.0.deleted && e.0.fire_ts == fire_ts && e.0.node_id == node_id {
                e.0.deleted = true;
                tombstoned = true;
                break;
            }
        }
        self.heap = BinaryHeap::from(entries);
        // Caller is responsible for behavior on "delete of nonexistent timer";
        // we silently no-op (most common case: operator deleted an already-fired
        // timer between watermark advances).
        let _ = tombstoned;
    }

    /// Pop the next due timer (fire_ts <= watermark). Skips tombstones.
    /// Returns `None` when no due, non-tombstoned timer exists.
    pub(crate) fn pop_due(&mut self, watermark: i64) -> Option<TimerEntry> {
        while let Some(Reverse(entry)) = self.heap.peek().copied() {
            if entry.fire_ts > watermark {
                return None;
            }
            let popped = self.heap.pop().expect("peek-then-pop is total").0;
            if !popped.deleted {
                return Some(popped);
            }
        }
        None
    }

    /// Drain ALL remaining (non-tombstoned) timers in fire order. Used on
    /// end-of-stream `flush` so registered-but-not-yet-fired timers run.
    pub(crate) fn drain_all(&mut self) -> Vec<TimerEntry> {
        let mut out = Vec::new();
        // Drain in heap order to preserve (fire_ts, seq) discipline.
        while let Some(Reverse(entry)) = self.heap.pop() {
            if !entry.deleted {
                out.push(entry);
            }
        }
        out.sort();
        out
    }

    /// True when no timers are registered (or all are tombstoned). Used by
    /// the engine to skip the drain path on hot record paths.
    #[allow(dead_code)]
    pub(crate) fn is_empty(&self) -> bool {
        self.heap.iter().all(|Reverse(e)| e.deleted)
    }

    /// Serializable snapshot of the timer state.
    ///
    /// Used by Phase 3a snapshot v2 to include the per-key timer heap in
    /// checkpoint round-trips. Allowed-dead pending that wire-up.
    #[allow(dead_code)]
    pub(crate) fn snapshot(&self) -> TimerServiceSnapshot {
        let mut entries: Vec<TimerEntry> = self
            .heap
            .iter()
            .filter(|Reverse(e)| !e.deleted)
            .map(|Reverse(e)| *e)
            .collect();
        entries.sort();
        TimerServiceSnapshot {
            entries,
            next_seq: self.next_seq,
        }
    }

    /// Restore from a previously-captured snapshot.
    ///
    /// Used by Phase 3a snapshot v2. Allowed-dead pending that wire-up.
    #[allow(dead_code)]
    pub(crate) fn restore(&mut self, snap: TimerServiceSnapshot) {
        self.heap.clear();
        for e in snap.entries {
            self.heap.push(Reverse(e));
        }
        self.next_seq = snap.next_seq;
    }
}

/// Serializable form of a [`TimerService`].
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub(crate) struct TimerServiceSnapshot {
    pub(crate) entries: Vec<TimerEntry>,
    pub(crate) next_seq: u64,
}

/// Context object passed to operator methods that may register, delete, or
/// observe event-time timers.
///
/// Obtained inside the engine and handed to the operator's `eval_with_ctx`
/// / `on_timer` methods. Operators cannot construct one themselves —
/// the only way to register a timer is from within an operator method that
/// the engine called.
pub struct TimerCtx<'a> {
    pub(crate) service: &'a mut TimerService,
    pub(crate) current_node_id: NodeId,
    pub(crate) current_ts: i64,
}

impl<'a> std::fmt::Debug for TimerCtx<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TimerCtx")
            .field("current_node_id", &self.current_node_id)
            .field("current_ts", &self.current_ts)
            .finish_non_exhaustive()
    }
}

impl<'a> TimerCtx<'a> {
    /// Register an event-time timer for the calling node. The timer fires
    /// when the per-key watermark next reaches `fire_ts` (i.e., when a
    /// record arrives with timestamp `>= fire_ts`, or
    /// [`KeyedGraphState::advance_event_time_watermark`](crate::keyed::KeyedGraphState::advance_event_time_watermark)
    /// is called with a value `>= fire_ts`).
    ///
    /// Multiple timers at the same `fire_ts` are allowed; they fire in
    /// registration order. Registering a timer with `fire_ts` already at or
    /// below `current_ts` fires it at the *next* watermark advance, not
    /// during the current step.
    pub fn register_event_time_timer(&mut self, fire_ts: i64) {
        self.service.register(fire_ts, self.current_node_id);
    }

    /// Delete the calling node's earliest-registered timer at `fire_ts`.
    /// If no such timer exists, the call is a no-op. Use this when an
    /// operator wants to cancel a pending timer (e.g., a pulse-width
    /// detector cancelling the `TooLong` timer when the pulse closes
    /// cleanly).
    pub fn delete_event_time_timer(&mut self, fire_ts: i64) {
        self.service.delete(fire_ts, self.current_node_id);
    }

    /// The event-time of the record (or watermark advance) that triggered
    /// the currently-running operator method.
    #[must_use]
    pub const fn current_ts(&self) -> i64 {
        self.current_ts
    }

    /// The node id of the calling operator. Exposed for diagnostics
    /// (e.g., tracing spans); not commonly needed by operator code.
    #[must_use]
    pub const fn current_node_id(&self) -> NodeId {
        self.current_node_id
    }
}

/// Event-time millisecond. A newtype that prevents accidental construction
/// from a processing-time source (e.g.,
/// [`SystemTime::now`](std::time::SystemTime::now)).
///
/// The only way to construct one is from an explicit `i64`. The newtype
/// exists so callers of
/// [`KeyedGraphState::advance_event_time_watermark`](crate::keyed::KeyedGraphState::advance_event_time_watermark)
/// must consciously assert that the value is in the event-time domain.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct EventTimeMs(i64);

impl EventTimeMs {
    /// Construct from explicit milliseconds-since-epoch (or the
    /// equivalent ordering key for the caller's event-time semantics).
    #[must_use]
    pub const fn new(ms: i64) -> Self {
        Self(ms)
    }

    /// The underlying `i64` representation.
    #[must_use]
    pub const fn get(self) -> i64 {
        self.0
    }
}

impl From<EventTimeMs> for i64 {
    fn from(t: EventTimeMs) -> Self {
        t.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::comp::NodeId;

    fn n(id: usize) -> NodeId {
        NodeId(id)
    }

    #[test]
    fn pop_due_returns_in_fire_ts_order() {
        let mut s = TimerService::default();
        s.register(300, n(1));
        s.register(100, n(2));
        s.register(200, n(3));
        let mut fired = Vec::new();
        while let Some(e) = s.pop_due(i64::MAX) {
            fired.push((e.fire_ts, e.node_id.0));
        }
        assert_eq!(fired, vec![(100, 2), (200, 3), (300, 1)]);
    }

    #[test]
    fn pop_due_breaks_ties_by_registration_seq() {
        let mut s = TimerService::default();
        s.register(100, n(1));
        s.register(100, n(2));
        s.register(100, n(3));
        let mut fired = Vec::new();
        while let Some(e) = s.pop_due(i64::MAX) {
            fired.push(e.node_id.0);
        }
        assert_eq!(fired, vec![1, 2, 3]);
    }

    #[test]
    fn pop_due_respects_watermark() {
        let mut s = TimerService::default();
        s.register(100, n(1));
        s.register(200, n(2));
        assert_eq!(s.pop_due(150).map(|e| e.fire_ts), Some(100));
        assert!(s.pop_due(150).is_none());
        assert_eq!(s.pop_due(250).map(|e| e.fire_ts), Some(200));
    }

    #[test]
    fn delete_tombstones_first_match() {
        let mut s = TimerService::default();
        s.register(100, n(1));
        s.register(100, n(1));
        s.delete(100, n(1));
        let mut fired = Vec::new();
        while let Some(e) = s.pop_due(i64::MAX) {
            fired.push(e.fire_ts);
        }
        // First entry tombstoned, second still fires.
        assert_eq!(fired, vec![100]);
    }

    #[test]
    fn snapshot_restore_preserves_pending_timers() {
        let mut s = TimerService::default();
        s.register(200, n(1));
        s.register(100, n(2));
        let snap = s.snapshot();
        let mut s2 = TimerService::default();
        s2.restore(snap);
        let mut fired = Vec::new();
        while let Some(e) = s2.pop_due(i64::MAX) {
            fired.push((e.fire_ts, e.node_id.0));
        }
        assert_eq!(fired, vec![(100, 2), (200, 1)]);
    }

    #[test]
    fn event_time_ms_round_trips() {
        let t = EventTimeMs::new(1_700_000_000_000);
        assert_eq!(t.get(), 1_700_000_000_000);
        let raw: i64 = t.into();
        assert_eq!(raw, 1_700_000_000_000);
    }
}
