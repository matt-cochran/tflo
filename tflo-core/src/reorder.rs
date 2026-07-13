//! Standalone event-time **reorder buffer** — releases out-of-order events in
//! timestamp order within an allowed-lateness window, so a downstream consumer
//! (the CEP matcher, an operator, anything) sees events in event-time order even
//! when they *arrive* out of order (offline sync, multi-source, retries, replay).
//!
//! The logic is lifted from the proven `KeyedGraphState` reorder path
//! (`keyed.rs`) into a reusable, graph-free unit: a `BinaryHeap` ordered by
//! `(ts, seq)` (deterministic — ties break by arrival order), a released
//! frontier (`last_ts`), and a watermark = `max_ts_seen - max_lateness`. Events
//! behind the frontier are dropped (recorded as **late**). The watermark also
//! advances off an injected clock ([`advance_watermark`](ReorderBuffer::advance_watermark)),
//! so a quiet stream still releases buffered events once they age past the window.
//!
//! `KeyedGraphState` keeps its own (graph-coupled) copy for now; this is the
//! single reusable abstraction going forward.

use std::cmp::Reverse;
use std::collections::BinaryHeap;

/// Maximum allowed lateness (24h). A ceiling bounds the heap and prevents
/// adversarial input from arbitrarily delaying watermark-driven release.
pub const MAX_LATENESS_MS: i64 = 24 * 60 * 60 * 1000;

/// Heap entry ordered by `(ts, seq)` — smallest `ts` first (via `Reverse`), ties
/// broken by arrival order for deterministic, replay-stable release.
#[derive(Debug)]
struct PendingEntry<E> {
    ts: i64,
    seq: u64,
    record: E,
}

impl<E> PartialEq for PendingEntry<E> {
    fn eq(&self, other: &Self) -> bool {
        self.ts == other.ts && self.seq == other.seq
    }
}
impl<E> Eq for PendingEntry<E> {}
impl<E> PartialOrd for PendingEntry<E> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
impl<E> Ord for PendingEntry<E> {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.ts.cmp(&other.ts).then(self.seq.cmp(&other.seq))
    }
}

/// An event-time reorder buffer. Feed it `(ts, event)`; it returns the events
/// that are now safe to release, **in timestamp order**. `max_lateness_ms` is how
/// long it waits for stragglers — `0` releases greedily (no reordering), larger
/// values trade latency for out-of-order tolerance.
#[derive(Debug)]
pub struct ReorderBuffer<E> {
    pending: BinaryHeap<Reverse<PendingEntry<E>>>,
    pending_seq: u64,
    /// Released frontier — the ts of the most recently released event.
    last_ts: Option<i64>,
    max_ts_seen: Option<i64>,
    max_lateness_ms: i64,
    late: Vec<(i64, E)>,
}

impl<E> ReorderBuffer<E> {
    /// A buffer tolerating up to `max_lateness_ms` of out-of-order arrival
    /// (clamped to `[0, MAX_LATENESS_MS]`).
    #[must_use]
    pub fn new(max_lateness_ms: i64) -> Self {
        Self {
            pending: BinaryHeap::new(),
            pending_seq: 0,
            last_ts: None,
            max_ts_seen: None,
            max_lateness_ms: max_lateness_ms.clamp(0, MAX_LATENESS_MS),
            late: Vec::new(),
        }
    }

    /// Buffer one event and return any events now safe to release, in ts order.
    /// An event behind the released frontier can never be emitted in order — it is
    /// dropped and recorded (see [`take_late`](Self::take_late)).
    pub fn step(&mut self, ts: i64, record: E) -> Vec<(i64, E)> {
        if self.last_ts.is_some_and(|last| ts < last) {
            self.late.push((ts, record));
            return Vec::new();
        }
        let seq = self.pending_seq;
        self.pending_seq = self.pending_seq.saturating_add(1);
        self.pending.push(Reverse(PendingEntry { ts, seq, record }));
        self.max_ts_seen = Some(self.max_ts_seen.map_or(ts, |m| m.max(ts)));
        let watermark = self
            .max_ts_seen
            .unwrap_or(ts)
            .saturating_sub(self.max_lateness_ms);
        self.drain_until(watermark)
    }

    /// Advance the watermark to clock time `now` (no new event) and release
    /// everything that has aged past the lateness window. This is how the injected
    /// clock drives release on a quiet stream.
    pub fn advance_watermark(&mut self, now: i64) -> Vec<(i64, E)> {
        self.max_ts_seen = Some(self.max_ts_seen.map_or(now, |m| m.max(now)));
        let watermark = now.saturating_sub(self.max_lateness_ms);
        self.drain_until(watermark)
    }

    /// Drain everything remaining (end of stream), in ts order.
    pub fn flush(&mut self) -> Vec<(i64, E)> {
        let mut out = Vec::with_capacity(self.pending.len());
        while let Some(Reverse(entry)) = self.pending.pop() {
            self.last_ts = Some(entry.ts);
            out.push((entry.ts, entry.record));
        }
        out
    }

    /// The earliest buffered event's ts, if any — the next instant a watermark
    /// advance could release work (for `next_deadline`-style scheduling).
    #[must_use]
    pub fn next_release_ts(&self) -> Option<i64> {
        self.pending.peek().map(|Reverse(e)| e.ts)
    }

    /// The CLOCK time at which the earliest buffered event ages past the lateness
    /// window and is released (`earliest_ts + max_lateness`). A driver schedules
    /// its next tick for this, so a quiet stream still drains on time.
    #[must_use]
    pub fn next_release_clock(&self) -> Option<i64> {
        self.pending
            .peek()
            .map(|Reverse(e)| e.ts.saturating_add(self.max_lateness_ms))
    }

    /// Whether any events are currently buffered.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.pending.is_empty()
    }

    /// Drain the late records dropped behind the frontier (opt-in side output for
    /// observability — e.g. a `DropReason::Late` counter).
    pub fn take_late(&mut self) -> Vec<(i64, E)> {
        std::mem::take(&mut self.late)
    }

    fn drain_until(&mut self, watermark: i64) -> Vec<(i64, E)> {
        let mut released = Vec::new();
        while self
            .pending
            .peek()
            .is_some_and(|Reverse(e)| e.ts <= watermark)
        {
            let Some(Reverse(entry)) = self.pending.pop() else {
                break;
            };
            self.last_ts = Some(entry.ts);
            released.push((entry.ts, entry.record));
        }
        released
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Collect just the ts of released events.
    fn ts(v: Vec<(i64, &'static str)>) -> Vec<i64> {
        v.into_iter().map(|(t, _)| t).collect()
    }

    #[test]
    fn lateness_zero_releases_greedily_no_reorder() {
        let mut b = ReorderBuffer::new(0);
        assert_eq!(ts(b.step(0, "a")), vec![0]);
        assert_eq!(ts(b.step(10, "b")), vec![10]);
    }

    #[test]
    fn buffers_then_releases_in_event_time_order() {
        // lateness 100: out-of-order arrivals are reordered within the window.
        let mut b = ReorderBuffer::new(100);
        assert!(b.step(50, "a").is_empty()); // buffered (watermark 50-100 < 50)
        assert!(b.step(40, "b").is_empty()); // earlier, still buffered
        assert!(b.step(60, "c").is_empty());
        // Now a far-future event pushes the watermark past 40/50/60.
        let released = ts(b.step(200, "d"));
        assert_eq!(released, vec![40, 50, 60]); // event-time order, not arrival
    }

    #[test]
    fn event_behind_frontier_is_dropped_as_late() {
        let mut b = ReorderBuffer::new(0);
        let _ = b.step(100, "a"); // releases, frontier = 100
        assert!(b.step(50, "late").is_empty()); // 50 < 100 → dropped
        let late = b.take_late();
        assert_eq!(late.len(), 1);
        assert_eq!(late[0].0, 50);
    }

    #[test]
    fn clock_watermark_releases_quiet_stream() {
        let mut b = ReorderBuffer::new(100);
        assert!(b.step(50, "a").is_empty()); // buffered
        // No further events; the clock advances to 200 → 50 ages out (200-100=100 >= 50).
        assert_eq!(ts(b.advance_watermark(200)), vec![50]);
    }

    #[test]
    fn flush_drains_remaining_in_order() {
        let mut b = ReorderBuffer::new(1000);
        let _ = b.step(30, "a");
        let _ = b.step(10, "b");
        let _ = b.step(20, "c");
        assert_eq!(ts(b.flush()), vec![10, 20, 30]);
        assert!(b.is_empty());
    }
}
