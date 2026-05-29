//! Adapter contracts for distributed execution.
//!
//! This module provides minimal, runtime-agnostic abstractions that allow
//! extension crates to integrate `tflo` with distributed data planes (Kafka,
//! Kinesis, NATS, SQL) and state planes (S3, Postgres, files) without
//! building a full distributed runtime.
//!
//! # Key Concepts
//!
//! - **Cursor**: Represents progress through a data stream (offset, sequence number, LSN, etc.)
//! - **`CursorStore`**: Persists and retrieves cursors for checkpoint coordination
//! - **`CheckpointPolicy`**: Determines when to take snapshots
//! - **`CheckpointId`**: Ties together state snapshot + cursor for atomic checkpointing

use crate::keyed::StateSnapshot;
use std::fmt::Debug;

/// A cursor representing progress through a data stream.
///
/// Different data planes use different cursor types:
/// - Kafka: `(topic, partition, offset)`
/// - Kinesis: `(stream, shard, sequence_number)`
/// - NATS `JetStream`: `(subject, consumer, sequence)`
/// - SQL CDC: `(table, lsn)` or `(table, timestamp)`
///
/// This trait allows adapters to abstract over these differences.
pub trait Cursor: Clone + Send + Sync + Debug + 'static {
    /// Serialize the cursor to bytes for storage.
    fn to_bytes(&self) -> Vec<u8>;

    /// Deserialize a cursor from bytes.
    ///
    /// # Errors
    ///
    /// Returns an error string when `data` is not a valid encoding of `Self`.
    fn from_bytes(data: &[u8]) -> Result<Self, String>;

    /// Get a human-readable representation for logging/debugging.
    fn display(&self) -> String;
}

/// Store for persisting and retrieving cursors.
///
/// Implementations can use the data plane's native commit mechanism
/// (e.g., Kafka consumer group commits) or a separate store.
pub trait CursorStore: Send + Sync {
    /// The cursor type this store manages.
    type Cursor: Cursor;

    /// Save a cursor for a given key.
    ///
    /// The key identifies which partition/shard/subject this cursor belongs to.
    ///
    /// # Errors
    ///
    /// Returns an error string when the underlying store cannot persist the
    /// cursor (I/O failure, network timeout, permission denied, etc.).
    fn save_cursor(&self, key: &[u8], cursor: &Self::Cursor) -> Result<(), String>;

    /// Load the most recent cursor for a given key.
    ///
    /// # Errors
    ///
    /// Returns an error string when the underlying store cannot be queried
    /// (I/O failure, network timeout, permission denied, etc.). A missing
    /// cursor is reported as `Ok(None)`, not an error.
    fn load_cursor(&self, key: &[u8]) -> Result<Option<Self::Cursor>, String>;

    /// List all keys that have cursors.
    ///
    /// # Errors
    ///
    /// Returns an error string when the underlying store cannot be enumerated.
    fn list_cursor_keys(&self) -> Result<Vec<Vec<u8>>, String>;
}

/// Policy for when to take checkpoints.
#[derive(Debug, Clone)]
pub enum CheckpointPolicy {
    /// Checkpoint every N records.
    EveryNRecords {
        /// Number of records between checkpoints.
        n: usize,
    },
    /// Checkpoint every N milliseconds.
    EveryNMs {
        /// Milliseconds between checkpoints.
        ms: i64,
    },
    /// Take a checkpoint manually (caller controls timing).
    Manual,
    /// Take a checkpoint on both time and record count boundaries.
    Both {
        /// Checkpoint every N records
        records: usize,
        /// Checkpoint every N milliseconds
        ms: i64,
    },
}

impl CheckpointPolicy {
    /// Check if a checkpoint should be taken based on this policy.
    ///
    /// Returns `true` if a checkpoint should be taken given:
    /// - `records_since_last_checkpoint`: Number of records processed since last checkpoint
    /// - `ms_since_last_checkpoint`: Milliseconds elapsed since last checkpoint
    #[must_use]
    pub const fn should_checkpoint(
        &self,
        records_since_last_checkpoint: usize,
        ms_since_last_checkpoint: i64,
    ) -> bool {
        match self {
            Self::EveryNRecords { n } => records_since_last_checkpoint >= *n,
            Self::EveryNMs { ms } => ms_since_last_checkpoint >= *ms,
            Self::Manual => false, // Manual checkpoints are triggered externally
            Self::Both { records, ms } => {
                records_since_last_checkpoint >= *records || ms_since_last_checkpoint >= *ms
            }
        }
    }
}

/// Identifier for a checkpoint, tying together state snapshot and cursor.
///
/// This allows atomic checkpointing: both state and cursor progress are
/// committed together, ensuring consistency.
#[derive(Debug, Clone)]
pub struct CheckpointId {
    /// Unique identifier for this checkpoint (e.g., timestamp, sequence number)
    pub id: String,
    /// The cursor position at the time of this checkpoint
    pub cursor_bytes: Vec<u8>,
    /// Timestamp when checkpoint was taken (milliseconds since epoch)
    pub timestamp_ms: i64,
    /// The key this checkpoint belongs to (if keyed execution)
    pub key: Option<Vec<u8>>,
}

impl CheckpointId {
    /// Create a new checkpoint ID.
    #[must_use]
    pub const fn new(
        id: String,
        cursor_bytes: Vec<u8>,
        timestamp_ms: i64,
        key: Option<Vec<u8>>,
    ) -> Self {
        Self {
            id,
            cursor_bytes,
            timestamp_ms,
            key,
        }
    }
}

/// Helper for coordinating checkpoints between state snapshots and cursors.
///
/// This struct ties together the `StateSnapshot` and `Cursor` concepts,
/// allowing adapters to atomically checkpoint both state and progress.
#[derive(Debug, Clone)]
pub struct Checkpoint {
    /// The checkpoint identifier
    pub id: CheckpointId,
    /// The state snapshot at this checkpoint
    pub state: StateSnapshot,
}

impl Checkpoint {
    /// Create a new checkpoint from a state snapshot and cursor.
    #[must_use]
    pub fn new<C: Cursor>(id: String, state: StateSnapshot, cursor: &C, timestamp_ms: i64) -> Self {
        let cursor_bytes = cursor.to_bytes();
        let key = state.metadata.key.clone();
        Self {
            id: CheckpointId::new(id, cursor_bytes, timestamp_ms, key),
            state,
        }
    }
}

/// Observability metrics for per-key execution.
///
/// Adapters can implement this trait to emit metrics about keyed execution
/// (graph count, warmup status, checkpoint latency, etc.) to their monitoring
/// system (Prometheus, `StatsD`, etc.).
///
/// # Status: defined, not yet wired
///
/// The trait's contract is stable, but the keyed runtime
/// ([`crate::keyed`]) and the [`crate::state::Checkpointer`] do not yet
/// call these methods at their lifecycle points. A `MetricsSink` carrier
/// plus the wire-up at graph-create, graph-remove, commit, and restore
/// is a follow-up. Implementors should expect no traffic until then.
pub trait KeyedMetrics: Send + Sync {
    /// Record that a new graph was created for a key.
    fn record_graph_created(&self, key: &[u8]);

    /// Record that a graph was removed (key expired or partition reassigned).
    fn record_graph_removed(&self, key: &[u8]);

    /// Record checkpoint duration.
    fn record_checkpoint_duration(&self, key: &[u8], duration_ms: i64);

    /// Record restore duration.
    fn record_restore_duration(&self, key: &[u8], duration_ms: i64);

    /// Record number of warmed-up graphs.
    fn record_warmed_graphs(&self, count: usize);

    /// Record number of graphs still warming up.
    fn record_warming_graphs(&self, count: usize);
}

/// No-op metrics implementation for when metrics are not needed.
///
/// # Status
///
/// The [`KeyedMetrics`] surface is defined but **not yet wired into the
/// keyed runtime** — `record_*` methods are documented integration
/// points, not active callbacks. A future Phase-1.5 task will thread
/// the trait through [`crate::keyed`] checkpoint and lifecycle hooks.
/// Until then, supplying a real implementation has no effect.
///
/// # Example
///
/// ```
/// use tflo_core::adapter::{KeyedMetrics, NoopMetrics};
///
/// let metrics = NoopMetrics;
/// metrics.record_graph_created(b"sensor-1");
/// metrics.record_checkpoint_duration(b"sensor-1", 42);
/// ```
#[derive(Debug, Clone, Copy, Default)]
pub struct NoopMetrics;

impl KeyedMetrics for NoopMetrics {
    fn record_graph_created(&self, _key: &[u8]) {}
    fn record_graph_removed(&self, _key: &[u8]) {}
    fn record_checkpoint_duration(&self, _key: &[u8], _duration_ms: i64) {}
    fn record_restore_duration(&self, _key: &[u8], _duration_ms: i64) {}
    fn record_warmed_graphs(&self, _count: usize) {}
    fn record_warming_graphs(&self, _count: usize) {}
}
