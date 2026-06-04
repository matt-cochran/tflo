//! Pluggable key→shard ownership for distributed keyed execution.
//!
//! `ShardRouter` is the **central distribution capability** asked for by
//! Phase 1: a small trait in core that connector crates (Kafka, etc.)
//! implement to drive sharded ownership. The lifecycle hooks
//! (`on_assign`/`on_revoke` — the actually-async, runtime-specific bits)
//! live in the connector, not here. This module exposes only:
//!
//! - the [`ShardRouter`] trait,
//! - a monotonic [`AssignmentEpoch`] used to fence stale events,
//! - a [`LocalShard`] default impl (all keys local — current behavior).
//!
//! ## Why an epoch counter is mandatory
//!
//! During a rebalance the consumer's view of ownership lags reality: an
//! event tagged with a now-revoked key may arrive a few milliseconds after
//! the rebalance callback completes. Without a monotonic version, two
//! workers can both believe they own the key and both update state →
//! divergence. The fence: every event is stamped with the
//! [`AssignmentEpoch`] at intake; if the current epoch is greater, the
//! event is dropped (and counted).

use std::sync::atomic::{AtomicU64, Ordering};

/// Monotonic version of "which keys do I own."
///
/// Bumped by the router on every assignment change. Compared
/// `<`-style on event intake: if the event's stamped epoch is less than
/// the router's current epoch, the event predates the most recent
/// rebalance and must be discarded as stale.
#[derive(Debug)]
pub struct AssignmentEpoch(AtomicU64);

impl AssignmentEpoch {
    /// Construct a fresh epoch at value 0.
    #[must_use]
    pub const fn new() -> Self {
        Self(AtomicU64::new(0))
    }

    /// Current epoch — read with `Acquire` ordering so events crossing into
    /// processing observe the most recent assignment.
    #[must_use]
    pub fn get(&self) -> u64 {
        self.0.load(Ordering::Acquire)
    }

    /// Bump the epoch — used by routers when their assignment changes.
    /// Returns the new value.
    pub fn bump(&self) -> u64 {
        // SAFETY: `fetch_add(1)` returns the previous value; `+ 1` would
        // only overflow after 2^64 rebalances (≈ centuries at any
        // plausible rate). Saturating is functionally equivalent at that
        // ceiling and avoids the lint without changing observable
        // behavior.
        self.0.fetch_add(1, Ordering::AcqRel).saturating_add(1)
    }
}

impl Default for AssignmentEpoch {
    fn default() -> Self {
        Self::new()
    }
}

/// Why an event was dropped on intake.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DropReason {
    /// Event arrived for a key this worker no longer owns (post-rebalance
    /// race).
    StaleEpoch,
    /// Event arrived for a key this worker never owned.
    NotOwned,
}

/// Pluggable key→shard ownership.
///
/// The contract is **minimal on purpose**: implementations only have to
/// answer "do I own this key, right now, at this epoch?" The async
/// lifecycle (rebalance callbacks, pre-warm load, flush-on-revoke) lives
/// in the connector that drives the router, since those concerns are
/// inherently runtime-coupled.
///
/// Implementations live in connector crates. `tflo-core` ships only
/// [`LocalShard`] (own everything) as the default, preserving existing
/// single-process behavior.
pub trait ShardRouter<K>: Send + Sync {
    /// Does this router currently own the given key?
    fn owns(&self, key: &K) -> bool;

    /// Read the router's current assignment epoch.
    ///
    /// Events should stamp this on intake and processing code should
    /// re-read at decision time; an event whose stamped value is strictly
    /// less than the current epoch is stale and must be dropped.
    fn assignment_epoch(&self) -> u64;
}

/// Default router: own every key. Equivalent to today's single-process
/// behavior — keep this as the no-op opt-out from sharding.
#[derive(Debug, Default)]
pub struct LocalShard {
    epoch: AssignmentEpoch,
}

impl LocalShard {
    /// Construct.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            epoch: AssignmentEpoch::new(),
        }
    }

    /// Bump the epoch — useful in tests that simulate a rebalance.
    pub fn simulate_rebalance(&self) -> u64 {
        self.epoch.bump()
    }
}

impl<K> ShardRouter<K> for LocalShard {
    fn owns(&self, _key: &K) -> bool {
        true
    }
    fn assignment_epoch(&self) -> u64 {
        self.epoch.get()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_shard_owns_everything() {
        let s = LocalShard::new();
        assert!(<LocalShard as ShardRouter<u32>>::owns(&s, &42));
        assert!(<LocalShard as ShardRouter<String>>::owns(&s, &"k".into()));
    }

    #[test]
    fn epoch_bumps_monotonically() {
        let e = AssignmentEpoch::new();
        assert_eq!(e.get(), 0);
        assert_eq!(e.bump(), 1);
        assert_eq!(e.bump(), 2);
        assert_eq!(e.get(), 2);
    }

    #[test]
    fn local_shard_rebalance_bumps_epoch() {
        let s = LocalShard::new();
        assert_eq!(<LocalShard as ShardRouter<u32>>::assignment_epoch(&s), 0);
        let n = s.simulate_rebalance();
        assert_eq!(n, 1);
        assert_eq!(<LocalShard as ShardRouter<u32>>::assignment_epoch(&s), 1);
    }
}
