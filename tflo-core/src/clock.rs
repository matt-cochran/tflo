//! Clock — the swappable time source a *driver* reads to advance the engine's
//! pure `tick(now)` entry point.
//!
//! The engine never reads a clock itself: that would break deterministic replay
//! and cross-tier parity (the whole point is time-as-input). Instead the **host
//! driver** reads a [`Clock`] and feeds `now` to `tick`. Which clock it reads is
//! swappable — [`ManualClock`] (deterministic, the default, for tests / replay /
//! host-supplied event-time) or [`SystemClock`] (wall-clock, for production
//! native hosts). In the browser the driver is the tflo-react provider's
//! coalesced `setTimeout`, reading the page clock — the same role, host-side.

use std::sync::atomic::{AtomicI64, Ordering};

/// A source of "now" in epoch milliseconds. A driver calls [`now_ms`](Clock::now_ms)
/// and passes the result to the engine's `tick`.
pub trait Clock: Send + Sync {
    /// The current time in epoch milliseconds.
    fn now_ms(&self) -> i64;
}

/// Deterministic, manually-advanced clock — **the default**. Time is an explicit
/// input, so a `ManualClock`-driven run is byte-identical across replays and
/// across tiers. Use it for tests, replay, and host-driven ticking where the
/// host supplies authoritative event-time.
#[derive(Debug)]
pub struct ManualClock {
    now: AtomicI64,
}

impl ManualClock {
    /// A clock pinned at `start_ms`.
    #[must_use]
    pub fn new(start_ms: i64) -> Self {
        Self {
            now: AtomicI64::new(start_ms),
        }
    }

    /// Jump to an absolute time (ms).
    pub fn set(&self, ms: i64) {
        self.now.store(ms, Ordering::SeqCst);
    }

    /// Advance by `delta_ms` and return the new time.
    pub fn advance(&self, delta_ms: i64) -> i64 {
        self.now.fetch_add(delta_ms, Ordering::SeqCst).saturating_add(delta_ms)
    }
}

impl Default for ManualClock {
    fn default() -> Self {
        Self::new(0)
    }
}

impl Clock for ManualClock {
    fn now_ms(&self) -> i64 {
        self.now.load(Ordering::SeqCst)
    }
}

/// Wall-clock adapter for production native hosts. **Not** for the pure CEP
/// engine (which must stay clock-free); a driver owns this and feeds `tick`.
#[derive(Debug, Default, Clone, Copy)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now_ms(&self) -> i64 {
        use std::time::{SystemTime, UNIX_EPOCH};
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
            .unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manual_clock_is_deterministic() {
        let c = ManualClock::new(1_000);
        assert_eq!(c.now_ms(), 1_000);
        assert_eq!(c.advance(500), 1_500);
        assert_eq!(c.now_ms(), 1_500);
        c.set(42);
        assert_eq!(c.now_ms(), 42);
    }

    #[test]
    fn system_clock_is_after_2020() {
        // Sanity: a real wall-clock adapter returns a plausible epoch-ms value.
        let c = SystemClock;
        assert!(c.now_ms() > 1_577_836_800_000); // 2020-01-01
    }
}
