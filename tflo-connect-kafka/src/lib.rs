#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]
//! Kafka adapter for tflo keyed execution — **Phase 2 contracts**.
//!
//! # Design
//!
//! Following the contracts-in-core / impls-in-separate-crates rule, this
//! crate exposes a small *Kafka-client-shaped* trait surface
//! ([`KafkaConsumer`] / [`KafkaProducer`] / [`RebalanceListener`]) plus the
//! concrete [`KafkaShardRouter`] (driving
//! [`tflo_core::shard::ShardRouter`]). Users plug in their preferred
//! client implementation. An optional `rdkafka-backend` feature wires
//! `rdkafka` directly for those who don't want to write the glue.
//!
//! Why the indirection: librdkafka (a C dependency) is awkward to require
//! everywhere this crate is referenced. The trait pattern keeps the engine
//! contracts testable end-to-end on any host while still letting
//! production deployments use rdkafka.
//!
//! # What's here
//!
//! - [`KafkaOffset`] — `(topic, partition, offset)` cursor implementing
//!   [`tflo_core::adapter::Cursor`].
//! - [`KafkaConsumer`] / [`KafkaProducer`] — minimal async traits a client
//!   library must satisfy.
//! - [`KafkaShardRouter`] — [`ShardRouter`] impl driven by rebalance
//!   callbacks; required `AsyncStateStore` constructor parameter so users
//!   can't forget durable state for sharded execution.
//! - [`InMemoryCursorStore`] — back-compat sync `CursorStore` impl, plus
//!   an `AsyncCursorStore` impl behind the `async` feature.
//! - Module `rdkafka_backend` (feature `rdkafka-backend`) wires the
//!   above to a real `rdkafka::StreamConsumer`.

#![warn(missing_docs)]
#![warn(missing_debug_implementations)]
#![deny(unsafe_code)]

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tflo_core::adapter::{Cursor, CursorStore};

#[cfg(feature = "rdkafka-backend")]
pub mod rdkafka_backend;

// ── KafkaOffset (Cursor impl) ─────────────────────────────────────────

/// Kafka partition offset — implements [`Cursor`] for use with the
/// checkpoint protocol.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct KafkaOffset {
    /// Topic name.
    pub topic: String,
    /// Partition number.
    pub partition: i32,
    /// Offset (next-to-read position).
    pub offset: i64,
}

impl Cursor for KafkaOffset {
    fn to_bytes(&self) -> Vec<u8> {
        serde_json::to_vec(self).unwrap_or_default()
    }
    fn from_bytes(data: &[u8]) -> Result<Self, String> {
        serde_json::from_slice(data).map_err(|e| format!("Failed to deserialize KafkaOffset: {e}"))
    }
    fn display(&self) -> String {
        format!("{}:{}:{}", self.topic, self.partition, self.offset)
    }
}

impl serde::Serialize for KafkaOffset {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let mut st = s.serialize_struct("KafkaOffset", 3)?;
        st.serialize_field("topic", &self.topic)?;
        st.serialize_field("partition", &self.partition)?;
        st.serialize_field("offset", &self.offset)?;
        st.end()
    }
}

impl<'de> serde::Deserialize<'de> for KafkaOffset {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(serde::Deserialize)]
        struct Helper {
            topic: String,
            partition: i32,
            offset: i64,
        }
        let h = Helper::deserialize(d)?;
        Ok(Self {
            topic: h.topic,
            partition: h.partition,
            offset: h.offset,
        })
    }
}

// ── In-memory cursor store (back-compat + async) ──────────────────────

/// In-memory cursor store. Implements the sync `CursorStore` trait; when
/// the `async` feature is on it also implements
/// [`tflo_core::state::AsyncCursorStore`] over the same backing store so
/// the same instance can serve both APIs in tests / single-process apps.
#[derive(Debug, Clone, Default)]
pub struct InMemoryCursorStore {
    cursors: Arc<Mutex<HashMap<Vec<u8>, KafkaOffset>>>,
}

impl InMemoryCursorStore {
    /// Construct.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

impl CursorStore for InMemoryCursorStore {
    type Cursor = KafkaOffset;

    fn save_cursor(&self, key: &[u8], cursor: &Self::Cursor) -> Result<(), String> {
        let mut guard = self
            .cursors
            .lock()
            .map_err(|_| "cursor store mutex poisoned".to_string())?;
        guard.insert(key.to_vec(), cursor.clone());
        Ok(())
    }

    fn load_cursor(&self, key: &[u8]) -> Result<Option<Self::Cursor>, String> {
        let guard = self
            .cursors
            .lock()
            .map_err(|_| "cursor store mutex poisoned".to_string())?;
        Ok(guard.get(key).cloned())
    }

    fn list_cursor_keys(&self) -> Result<Vec<Vec<u8>>, String> {
        let guard = self
            .cursors
            .lock()
            .map_err(|_| "cursor store mutex poisoned".to_string())?;
        Ok(guard.keys().cloned().collect())
    }
}

#[cfg(feature = "async")]
#[async_trait::async_trait]
impl tflo_core::state::AsyncCursorStore<KafkaOffset> for InMemoryCursorStore {
    async fn save_cursor(&self, key: &[u8], cursor: &KafkaOffset) -> Result<(), String> {
        <Self as CursorStore>::save_cursor(self, key, cursor)
    }
    async fn load_cursor(&self, key: &[u8]) -> Result<Option<KafkaOffset>, String> {
        <Self as CursorStore>::load_cursor(self, key)
    }
}

// ── KafkaConsumer / KafkaProducer trait surface ────────────────────────

/// A consumed Kafka record, as surfaced by [`KafkaConsumer::poll`].
#[derive(Debug, Clone)]
pub struct KafkaMessage {
    /// Topic this message came from.
    pub topic: String,
    /// Partition this message came from.
    pub partition: i32,
    /// Offset of this message (the offset to commit is `offset + 1`).
    pub offset: i64,
    /// Optional key bytes.
    pub key: Option<Vec<u8>>,
    /// Payload bytes.
    pub value: Vec<u8>,
    /// Producer timestamp in milliseconds since epoch, if present.
    pub timestamp_ms: Option<i64>,
}

/// A rebalance event surfaced to the consumer.
#[derive(Debug, Clone)]
pub enum RebalanceEvent {
    /// Partitions newly assigned to this consumer.
    Assigned(Vec<TopicPartition>),
    /// Partitions revoked from this consumer.
    Revoked(Vec<TopicPartition>),
}

/// Topic + partition identifier used in rebalance callbacks.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TopicPartition {
    /// Topic name.
    pub topic: String,
    /// Partition number.
    pub partition: i32,
}

/// Minimal async Kafka consumer trait. A concrete `rdkafka` impl lives
/// under [`rdkafka_backend`] (feature `rdkafka-backend`); test code
/// uses [`MockKafkaConsumer`].
#[cfg(feature = "async")]
#[async_trait::async_trait]
pub trait KafkaConsumer: Send + Sync {
    /// Subscribe to topics. Idempotent.
    ///
    /// # Errors
    ///
    /// Returns an error string when the underlying client cannot subscribe.
    async fn subscribe(&self, topics: &[String]) -> Result<(), String>;

    /// Poll for the next message; `None` indicates the consumer has been
    /// shut down or no more messages will arrive.
    ///
    /// # Errors
    ///
    /// Returns an error string on consumer or network failure.
    async fn poll(&self) -> Result<Option<KafkaMessage>, String>;

    /// Poll for the next rebalance event; `None` indicates none pending.
    /// Concrete impls may surface this through a separate channel rather
    /// than poll — that is a backend-specific detail.
    ///
    /// # Errors
    ///
    /// Returns an error string on consumer failure.
    async fn poll_rebalance(&self) -> Result<Option<RebalanceEvent>, String>;

    /// Commit a single `(topic, partition, offset)` as the next-to-read
    /// position. Typically called via the [`tflo_core::state::Checkpointer`]
    /// cursor write.
    ///
    /// # Errors
    ///
    /// Returns an error string when the broker rejects the commit.
    async fn commit_offset(&self, offset: &KafkaOffset) -> Result<(), String>;
}

/// Minimal async Kafka producer trait.
#[cfg(feature = "async")]
#[async_trait::async_trait]
pub trait KafkaProducer: Send + Sync {
    /// Send a message; resolves once the broker acks.
    ///
    /// # Errors
    ///
    /// Returns an error string on producer or broker failure.
    async fn send(
        &self,
        topic: &str,
        key: Option<&[u8]>,
        value: &[u8],
        timestamp_ms: Option<i64>,
    ) -> Result<(), String>;
}

// ── KafkaShardRouter (the Phase 1 ShardRouter impl) ───────────────────

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
            .map_err(|_| "ownership mutex poisoned".to_string())?
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
            .map_err(|_| "ownership mutex poisoned".to_string())?;
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
        self.owned
            .lock()
            .map(|g| g.contains(key))
            .unwrap_or(false)
    }
    fn assignment_epoch(&self) -> u64 {
        self.epoch.get()
    }
}

// ── Tests ──────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kafka_offset_round_trip() {
        let off = KafkaOffset {
            topic: "t".into(),
            partition: 3,
            offset: 99,
        };
        let bytes = off.to_bytes();
        let back = KafkaOffset::from_bytes(&bytes).expect("ok");
        assert_eq!(off, back);
    }

    #[test]
    fn cursor_store_round_trip() {
        let s = InMemoryCursorStore::new();
        let off = KafkaOffset {
            topic: "t".into(),
            partition: 0,
            offset: 12,
        };
        s.save_cursor(b"k", &off).expect("ok");
        assert_eq!(s.load_cursor(b"k").expect("ok"), Some(off));
        assert_eq!(s.list_cursor_keys().expect("ok"), vec![b"k".to_vec()]);
    }

    #[cfg(feature = "async")]
    #[tokio::test]
    async fn async_cursor_store_round_trip() {
        use tflo_core::state::AsyncCursorStore;
        let s = InMemoryCursorStore::new();
        let off = KafkaOffset {
            topic: "t".into(),
            partition: 0,
            offset: 1,
        };
        AsyncCursorStore::save_cursor(&s, b"k", &off).await.expect("ok");
        assert_eq!(
            AsyncCursorStore::load_cursor(&s, b"k").await.expect("ok"),
            Some(off)
        );
    }

    // ── KafkaShardRouter behavior ──
    #[cfg(feature = "async")]
    mod router {
        use super::*;
        use std::sync::Arc;
        use tflo_core::keyed::StateSnapshot;
        use tflo_core::shard::ShardRouter;

        // Trivial AsyncStateStore for tests.
        #[derive(Default)]
        struct NoopStore;

        #[async_trait::async_trait]
        impl tflo_core::state::AsyncStateStore for NoopStore {
            async fn save(&self, _k: &[u8], _s: &StateSnapshot) -> Result<(), String> {
                Ok(())
            }
            async fn load(&self, _k: &[u8]) -> Result<Option<StateSnapshot>, String> {
                Ok(None)
            }
            async fn list_keys(&self) -> Result<Vec<Vec<u8>>, String> {
                Ok(Vec::new())
            }
        }

        #[tokio::test]
        async fn owns_only_assigned_partitions() {
            let r = KafkaShardRouter::new(Arc::new(NoopStore));
            let p0 = TopicPartition {
                topic: "t".into(),
                partition: 0,
            };
            let p1 = TopicPartition {
                topic: "t".into(),
                partition: 1,
            };
            assert!(!r.owns(&p0));
            r.apply_rebalance(&RebalanceEvent::Assigned(vec![p0.clone()]))
                .expect("ok");
            assert!(r.owns(&p0));
            assert!(!r.owns(&p1));
        }

        #[tokio::test]
        async fn epoch_bumps_on_rebalance() {
            let r = KafkaShardRouter::new(Arc::new(NoopStore));
            let p0 = TopicPartition {
                topic: "t".into(),
                partition: 0,
            };
            let e0 = r.assignment_epoch();
            r.apply_rebalance(&RebalanceEvent::Assigned(vec![p0.clone()]))
                .expect("ok");
            assert_eq!(r.assignment_epoch(), e0 + 1);
            r.apply_rebalance(&RebalanceEvent::Revoked(vec![p0]))
                .expect("ok");
            assert_eq!(r.assignment_epoch(), e0 + 2);
        }

        #[tokio::test]
        async fn revoke_removes_partition() {
            let r = KafkaShardRouter::new(Arc::new(NoopStore));
            let p0 = TopicPartition {
                topic: "t".into(),
                partition: 0,
            };
            r.apply_rebalance(&RebalanceEvent::Assigned(vec![p0.clone()]))
                .expect("ok");
            r.apply_rebalance(&RebalanceEvent::Revoked(vec![p0.clone()]))
                .expect("ok");
            assert!(!r.owns(&p0));
        }
    }
}
