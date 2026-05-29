//! `KafkaShardRouter` — `ShardRouter` impl driven by Kafka consumer-group
//! rebalance callbacks. Extracted from `lib.rs` via structureos `move`.

#[cfg(feature = "async")]
use crate::{RebalanceEvent, TopicPartition};
#[cfg(feature = "async")]
use std::sync::{Arc, Mutex};

/// `ShardRouter` impl driven by Kafka consumer-group rebalance callbacks.
///
/// The router is constructed with a **required**
/// [`tflo_core::state::AsyncStateStore`] reference (no default) — the
/// compile-time poka-yoke against the most common production mistake of
/// using a sharded router without durable state.
///
/// Owned partitions are tracked in-memory; `owns()` checks membership by
/// `(topic, partition)`. The `AssignmentEpoch` increments on every
/// `apply_rebalance` call, providing the rebalance-race fence described
/// in Phase 1.
#[cfg(feature = "async")]
pub struct KafkaShardRouter<S: tflo_core::state::AsyncStateStore> {
    state_store: Arc<S>,
    owned: Arc<Mutex<std::collections::HashSet<TopicPartition>>>,
    epoch: tflo_core::shard::AssignmentEpoch,
    /// Diagnostic counter: events dropped because the stamped epoch was
    /// strictly less than the router's current epoch.
    pub events_dropped_stale_epoch: std::sync::atomic::AtomicU64,
}

#[cfg(feature = "async")]
impl<S: tflo_core::state::AsyncStateStore> std::fmt::Debug for KafkaShardRouter<S> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("KafkaShardRouter")
            .field("epoch", &self.epoch.get())
            .field(
                "events_dropped_stale_epoch",
                &self
                    .events_dropped_stale_epoch
                    .load(std::sync::atomic::Ordering::Relaxed),
            )
            .finish()
    }
}

#[cfg(feature = "async")]
impl<S: tflo_core::state::AsyncStateStore> KafkaShardRouter<S> {
    /// Construct with a required state store. Use this in production.
    #[must_use]
    pub fn new(state_store: Arc<S>) -> Self {
        Self {
            state_store,
            owned: Arc::new(Mutex::new(std::collections::HashSet::new())),
            epoch: tflo_core::shard::AssignmentEpoch::new(),
            events_dropped_stale_epoch: std::sync::atomic::AtomicU64::new(0),
        }
    }

    /// Test-only constructor that takes a no-op store. The long name is
    /// deliberate — calling it in production should be obvious in review.
    #[must_use]
    #[doc(hidden)]
    pub fn new_with_in_memory_store_for_testing(state_store: Arc<S>) -> Self {
        Self::new(state_store)
    }

    /// The state store this router was constructed with.
    pub const fn state_store(&self) -> &Arc<S> {
        &self.state_store
    }

    /// Read the current set of owned partitions.
    ///
    /// # Errors
    ///
    /// Returns an error string when the internal mutex is poisoned.
    pub fn owned_partitions(&self) -> Result<Vec<TopicPartition>, String> {
        Ok(self
            .owned
            .lock()
            .map_err(|e| format!("ownership mutex poisoned: {e}"))?
            .iter()
            .cloned()
            .collect())
    }

    /// Apply a rebalance event: update the owned set and bump the
    /// assignment epoch. Bump happens **before** the future returned by
    /// any subsequent `on_revoke` flush completes, so stale-epoch events
    /// in flight cannot race past the new ownership.
    ///
    /// # Errors
    ///
    /// Returns an error string when the internal mutex is poisoned.
    pub fn apply_rebalance(&self, event: &RebalanceEvent) -> Result<(), String> {
        let mut guard = self
            .owned
            .lock()
            .map_err(|e| format!("ownership mutex poisoned: {e}"))?;
        match event {
            RebalanceEvent::Assigned(parts) => {
                for p in parts {
                    let _ = guard.insert(p.clone());
                }
            }
            RebalanceEvent::Revoked(parts) => {
                for p in parts {
                    let _ = guard.remove(p);
                }
            }
        }
        // Bump *after* the ownership change so consumers re-checking
        // post-rebalance observe the new ownership at the new epoch.
        let _ = self.epoch.bump();
        Ok(())
    }

    /// Drop counter — bump when an event arrives stamped with an older
    /// epoch than the router's current epoch.
    pub fn record_stale_event(&self) {
        self.events_dropped_stale_epoch
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }
}

#[cfg(feature = "async")]
impl<S: tflo_core::state::AsyncStateStore> tflo_core::shard::ShardRouter<TopicPartition>
    for KafkaShardRouter<S>
{
    fn owns(&self, key: &TopicPartition) -> bool {
        self.owned.lock().map(|g| g.contains(key)).unwrap_or(false)
    }
    fn assignment_epoch(&self) -> u64 {
        self.epoch.get()
    }
}
