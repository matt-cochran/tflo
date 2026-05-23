#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]
//! Kafka consumer adapter for tflo keyed execution.
//!
//! This crate provides a reference implementation of how to integrate `tflo`
//! with Kafka consumers, demonstrating:
//!
//! - Partition-based keyed execution
//! - Checkpoint coordination (state snapshots + offset commits)
//! - Per-partition state isolation
//!
//! # Example
//!
//! ```rust,no_run
//! use tflo_connect_kafka::KafkaTfloAdapter;
//! use tflo_core::prelude::*;
//!
//! // This is a conceptual example - actual Kafka integration would use
//! // rdkafka or another Kafka client library
//! # /*
//! let adapter = KafkaTfloAdapter::new(
//!     kafka_consumer,
//!     |record| record.key.clone(), // Extract key from Kafka record
//!     |record| record.timestamp,   // Extract timestamp
//!     |t| {
//!         t.timestamp(|r| r.timestamp);
//!         let price = t.prop(|r| r.price);
//!         price.sma(5.secs())
//!     },
//!     checkpoint_policy,
//!     state_store,
//!     cursor_store,
//! );
//!
//! for item in adapter {
//!     // Process results with key attribution
//!     println!("Key: {:?}, Value: {}", item.ctx.key(), item.value);
//! }
//! # */
//! ```
//!
//! # Architecture
//!
//! The adapter:
//! 1. Consumes records from Kafka partitions
//! 2. Extracts key and timestamp from each record
//! 3. Routes to per-key `tflo_keyed` execution
//! 4. On checkpoint boundaries:
//!    - Takes state snapshot
//!    - Persists to state store
//!    - Commits Kafka offset (cursor)
//! 5. Emits results with `KeyedTimestamped<K>` context

#![warn(missing_docs)]
#![warn(missing_debug_implementations)]
#![deny(unsafe_code)]

use std::collections::HashMap;
use std::hash::Hash;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tflo_core::adapter::{Checkpoint, CheckpointPolicy, Cursor, CursorStore, KeyedMetrics};
use tflo_core::builder::Compile;
use tflo_core::keyed::{OutOfOrderPolicy, StateSnapshot, StateStore};
use tflo_core::prelude::*;

/// Kafka offset cursor.
///
/// Represents a Kafka partition offset for checkpoint coordination.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct KafkaOffset {
    /// Topic name
    pub topic: String,
    /// Partition number
    pub partition: i32,
    /// Offset value
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
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut state = serializer.serialize_struct("KafkaOffset", 3)?;
        state.serialize_field("topic", &self.topic)?;
        state.serialize_field("partition", &self.partition)?;
        state.serialize_field("offset", &self.offset)?;
        state.end()
    }
}

impl<'de> serde::Deserialize<'de> for KafkaOffset {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        use serde::de::{self, Visitor};
        use std::fmt;

        struct KafkaOffsetVisitor;

        impl<'de> Visitor<'de> for KafkaOffsetVisitor {
            type Value = KafkaOffset;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("struct KafkaOffset")
            }

            fn visit_map<V>(self, mut map: V) -> Result<KafkaOffset, V::Error>
            where
                V: de::MapAccess<'de>,
            {
                let mut topic = None;
                let mut partition = None;
                let mut offset = None;
                while let Some(key) = map.next_key()? {
                    match key {
                        "topic" => {
                            if topic.is_some() {
                                return Err(de::Error::duplicate_field("topic"));
                            }
                            topic = Some(map.next_value()?);
                        }
                        "partition" => {
                            if partition.is_some() {
                                return Err(de::Error::duplicate_field("partition"));
                            }
                            partition = Some(map.next_value()?);
                        }
                        "offset" => {
                            if offset.is_some() {
                                return Err(de::Error::duplicate_field("offset"));
                            }
                            offset = Some(map.next_value()?);
                        }
                        _ => {
                            let _ = map.next_value::<de::IgnoredAny>()?;
                        }
                    }
                }
                let topic = topic.ok_or_else(|| de::Error::missing_field("topic"))?;
                let partition = partition.ok_or_else(|| de::Error::missing_field("partition"))?;
                let offset = offset.ok_or_else(|| de::Error::missing_field("offset"))?;
                Ok(KafkaOffset {
                    topic,
                    partition,
                    offset,
                })
            }
        }

        deserializer.deserialize_map(KafkaOffsetVisitor)
    }
}

/// In-memory cursor store for Kafka offsets.
///
/// In production, this would delegate to Kafka's consumer group commit mechanism.
/// This is a reference implementation showing the contract.
#[derive(Debug, Clone, Default)]
pub struct InMemoryCursorStore {
    cursors: std::sync::Arc<std::sync::Mutex<HashMap<Vec<u8>, KafkaOffset>>>,
}

impl InMemoryCursorStore {
    /// Create a new in-memory cursor store.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

impl CursorStore for InMemoryCursorStore {
    type Cursor = KafkaOffset;

    fn save_cursor(&self, key: &[u8], cursor: &Self::Cursor) -> Result<(), String> {
        // In a real implementation, this would commit to Kafka consumer group
        // For now, just store in memory
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

impl<R, K, KF, TF, FF, C, O> std::fmt::Debug for KafkaTfloAdapter<R, K, KF, TF, FF, C, O>
where
    K: Clone + Hash + Eq + Send + Sync + Default + 'static,
    O: ExtractOutput,
    KF: Fn(&R) -> K + Send + Sync + 'static,
    TF: Fn(&R) -> i64 + Send + Sync + 'static,
    FF: Fn(&mut TFlowBuilder<R>) -> C + Send + Sync + 'static,
    C: Compile<R>,
    C::Output: ExtractOutput,
    R: 'static,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("KafkaTfloAdapter").finish()
    }
}

/// Kafka adapter for tflo keyed execution.
///
/// This is a conceptual reference implementation showing how to integrate
/// `tflo` with Kafka. In practice, you would use an actual Kafka client
/// library (rdkafka, kafka-rust, etc.) and implement the adapter pattern
/// shown here.
///
/// # Architecture Notes
///
/// - **Partitioning**: Kafka partitions naturally map to `tflo_keyed` keys
/// - **Checkpointing**: State snapshots + offset commits are coordinated via `Checkpoint`
/// - **State isolation**: Each partition gets its own `CompiledGraph` instance
/// - **Observability**: Metrics hooks allow emitting per-partition stats
pub struct KafkaTfloAdapter<R, K, KF, TF, FF, C, O>
where
    K: Clone + Hash + Eq + Send + Sync + Default + 'static,
    O: ExtractOutput,
    KF: Fn(&R) -> K + Send + Sync + 'static,
    TF: Fn(&R) -> i64 + Send + Sync + 'static,
    FF: Fn(&mut TFlowBuilder<R>) -> C + Send + Sync + 'static,
    C: Compile<R>,
    C::Output: ExtractOutput,
    R: 'static,
{
    // In a real implementation, this would hold a Kafka consumer
    // For now, this is a placeholder showing the structure
    _phantom: std::marker::PhantomData<(R, K, KF, TF, FF, C, O)>,
}

impl<R, K, KF, TF, FF, C, O> KafkaTfloAdapter<R, K, KF, TF, FF, C, O>
where
    K: Clone + Hash + Eq + Send + Sync + Default + 'static,
    O: ExtractOutput,
    KF: Fn(&R) -> K + Send + Sync + 'static,
    TF: Fn(&R) -> i64 + Send + Sync + 'static,
    FF: Fn(&mut TFlowBuilder<R>) -> C + Send + Sync + 'static,
    C: Compile<R>,
    C::Output: ExtractOutput,
    R: 'static,
{
    /// Create a new Kafka adapter.
    ///
    /// # Arguments
    ///
    /// * `key_fn`: Extract key from Kafka record
    /// * `timestamp_fn`: Extract timestamp from Kafka record
    /// * `builder_fn`: Build tflo computation graph
    /// * `policy`: Out-of-order handling policy
    /// * `checkpoint_policy`: When to take checkpoints
    /// * `state_store`: Where to persist state snapshots
    /// * `cursor_store`: Where to persist Kafka offsets
    /// * `metrics`: Optional metrics emitter
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        _key_fn: KF,
        _timestamp_fn: TF,
        _builder_fn: FF,
        _policy: OutOfOrderPolicy,
        _checkpoint_policy: CheckpointPolicy,
        _state_store: Arc<dyn StateStore>,
        _cursor_store: Arc<dyn CursorStore<Cursor = KafkaOffset>>,
        _metrics: Arc<dyn KeyedMetrics>,
    ) -> Self {
        // In a real implementation, this would:
        // 1. Set up Kafka consumer
        // 2. Initialize per-partition state
        // 3. Set up checkpoint coordination
        Self {
            _phantom: std::marker::PhantomData,
        }
    }
}

/// Helper function to create a checkpoint from state and cursor.
pub fn create_checkpoint<C: Cursor>(state: StateSnapshot, cursor: &C) -> Checkpoint {
    // A clock before the Unix epoch is not representable; fall back to 0.
    let timestamp_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64;
    let checkpoint_id = format!("checkpoint_{timestamp_ms}");
    Checkpoint::new(checkpoint_id, state, cursor, timestamp_ms)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_kafka_offset_serialization() {
        let offset = KafkaOffset {
            topic: "test-topic".to_string(),
            partition: 0,
            offset: 12345,
        };

        let bytes = offset.to_bytes();
        let restored = KafkaOffset::from_bytes(&bytes).unwrap();
        assert_eq!(offset, restored);
    }

    #[test]
    fn test_cursor_store() {
        let store = InMemoryCursorStore::new();
        let offset = KafkaOffset {
            topic: "test".to_string(),
            partition: 0,
            offset: 100,
        };

        store.save_cursor(b"key1", &offset).unwrap();
        let loaded = store.load_cursor(b"key1").unwrap();
        assert_eq!(loaded, Some(offset));
    }
}
