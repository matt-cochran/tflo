#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing, clippy::arithmetic_side_effects))]
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

use tflo_core::adapter::Cursor;
#[cfg(feature = "async")]
pub use crate::consumer::KafkaConsumer;
#[cfg(feature = "async")]
pub use crate::shard::KafkaShardRouter;

#[cfg(feature = "rdkafka-backend")]
pub mod rdkafka_backend;
pub mod cursor_store;
pub mod shard;
pub mod consumer;

pub use crate::cursor_store::InMemoryCursorStore;

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

impl KafkaOffset {
    /// Build a [`KafkaOffset`] from a [`CommitableOffset`] — the type-safe
    /// way to construct cursor entries. Using this constructor (vs. the
    /// raw struct literal with `offset: msg.offset`) makes the off-by-one
    /// correction explicit at the call site: the [`CommitableOffset`] you
    /// pass in is guaranteed to already be `record-offset + 1`.
    #[must_use]
    pub const fn from_committable(topic: String, partition: i32, offset: CommitableOffset) -> Self {
        Self {
            topic,
            partition,
            offset: offset.as_i64(),
        }
    }
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

// ── KafkaConsumer / KafkaProducer trait surface ────────────────────────

/// A Kafka offset already incremented by 1 — the value that should be passed
/// to `commit`. The newtype prevents the off-by-one bug documented historically:
/// [`KafkaMessage::offset`] is the offset of the current record; the commit
/// value is `offset + 1`. Use [`KafkaMessage::commit_offset`] to obtain a
/// `CommitableOffset`; do not construct directly.
///
/// Backed by `i64` to match Kafka's wire/protocol offset type (also what
/// `rdkafka` exposes). Saturating arithmetic is used at construction to
/// avoid a wraparound at `i64::MAX`.
#[derive(Debug, Copy, Clone, Eq, PartialEq, Ord, PartialOrd)]
pub struct CommitableOffset(i64);

impl CommitableOffset {
    /// Underlying offset value (already `+1`). Use this when persisting or
    /// sending to Kafka.
    #[must_use]
    pub const fn into_inner(self) -> i64 {
        self.0
    }

    /// As `i64` — convenient for `rdkafka` APIs that take signed offsets.
    /// Equivalent to [`Self::into_inner`]; provided for symmetry with the
    /// other type-conversion helpers and to read clearly at call sites.
    #[must_use]
    pub const fn as_i64(self) -> i64 {
        self.0
    }
}

impl From<CommitableOffset> for i64 {
    fn from(o: CommitableOffset) -> Self {
        o.0
    }
}

/// A consumed Kafka record, as surfaced by [`KafkaConsumer::poll`].
#[derive(Debug, Clone)]
pub struct KafkaMessage {
    /// Topic this message came from.
    pub topic: String,
    /// Partition this message came from.
    pub partition: i32,
    /// Offset of this message. **Do not commit this value directly** — use
    /// [`Self::commit_offset`] to obtain the type-safe `+1`-adjusted
    /// [`CommitableOffset`].
    pub offset: i64,
    /// Optional key bytes.
    pub key: Option<Vec<u8>>,
    /// Payload bytes. `None` distinguishes a *tombstone* (compaction
    /// delete marker) from an *empty payload*; the historical
    /// `Vec<u8>::new()` representation conflated the two.
    pub payload: Option<Vec<u8>>,
    /// Producer timestamp in milliseconds since epoch, if present.
    pub timestamp_ms: Option<i64>,
}

impl KafkaMessage {
    /// The offset to commit for this message (always `self.offset + 1`).
    /// Saturating to handle the rare overflow at `i64::MAX`.
    #[must_use]
    pub const fn commit_offset(&self) -> CommitableOffset {
        CommitableOffset(self.offset.saturating_add(1))
    }
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

// ── Tests ──────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use tflo_core::adapter::CursorStore;

    fn sample_msg(offset: i64, payload: Option<Vec<u8>>) -> KafkaMessage {
        KafkaMessage {
            topic: "t".into(),
            partition: 0,
            offset,
            key: None,
            payload,
            timestamp_ms: None,
        }
    }

    #[test]
    fn commit_offset_returns_offset_plus_one() {
        let m = sample_msg(42, Some(b"v".to_vec()));
        assert_eq!(m.commit_offset().into_inner(), 43);
        assert_eq!(m.commit_offset().as_i64(), 43);
    }

    #[test]
    fn commit_offset_does_not_panic_at_max() {
        let m = sample_msg(i64::MAX, None);
        // Saturating add: stays at i64::MAX, no panic, no wraparound.
        assert_eq!(m.commit_offset().into_inner(), i64::MAX);
    }

    #[test]
    fn commitable_offset_into_i64_via_from() {
        let m = sample_msg(7, None);
        let c = m.commit_offset();
        let raw: i64 = c.into();
        assert_eq!(raw, 8);
    }

    #[test]
    fn kafka_offset_from_committable_preserves_value() {
        let m = sample_msg(99, None);
        let c = m.commit_offset();
        let off = KafkaOffset::from_committable("topic-x".into(), 5, c);
        assert_eq!(off.topic, "topic-x");
        assert_eq!(off.partition, 5);
        assert_eq!(off.offset, 100);
    }

    #[test]
    fn kafka_message_payload_is_some_when_present() {
        let m = sample_msg(0, Some(b"v".to_vec()));
        assert_eq!(m.payload.as_deref(), Some(b"v".as_slice()));
    }

    #[test]
    fn kafka_message_payload_is_none_for_tombstone() {
        let m = sample_msg(0, None);
        assert_eq!(m.payload, None);
        // Crucially, a tombstone is *not* the same as an empty payload.
        let empty = sample_msg(0, Some(Vec::new()));
        assert_ne!(m.payload, empty.payload);
        assert_ne!(None::<Vec<u8>>, Some(Vec::<u8>::new()));
    }

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
