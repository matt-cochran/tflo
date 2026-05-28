//! `KafkaConsumer` async trait. Extracted from `lib.rs` via structureos `move`.

#[cfg(feature = "async")]
use crate::{KafkaMessage, KafkaOffset, RebalanceEvent};

/// Minimal async Kafka consumer trait. A concrete `rdkafka` impl lives
/// under `crate::rdkafka_backend` (feature `rdkafka-backend`); test code
/// uses a hand-rolled in-memory mock (see the integration tests).
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
