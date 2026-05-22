//! Keyed execution support for tflo-core.
//!
//! This module provides APIs for partitioning records by key and running
//! separate computation graphs per key, ensuring state isolation while
//! preserving key attribution in the pipeline context.

use crate::builder::{Compile, TFlowBuilder};
use crate::compile::{CompiledGraph, ExtractOutput, StepResult};
use crate::error::{ComputeError, TFloError, TFloResult};
use crate::pipeline::{KeyedTimestamped, PipelineItem};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::hash::Hash;
use std::sync::Arc;

/// Snapshot of computation graph state for checkpoint/restore.
///
/// This is an opaque byte representation of the graph's state that can be
/// serialized and persisted, then later restored to resume computation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateSnapshot {
    /// Opaque bytes representing the serialized state.
    pub data: Vec<u8>,
    /// Metadata about the snapshot (key, timestamp, version, etc.).
    pub metadata: SnapshotMetadata,
}

/// Metadata associated with a state snapshot.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotMetadata {
    /// The key this snapshot belongs to (if keyed execution).
    pub key: Option<Vec<u8>>,
    /// Timestamp when snapshot was taken (milliseconds since epoch).
    pub timestamp_ms: i64,
    /// Version identifier for snapshot format compatibility.
    pub version: u32,
}

/// Trait for encoding/decoding state snapshots.
///
/// Implementations can use serde, bincode, postcard, or any other
/// serialization format. This trait allows users to choose their preferred
/// codec without tflo-core depending on specific serialization libraries.
pub trait SnapshotCodec: Send + Sync {
    /// Encode a state snapshot to bytes.
    fn encode(&self, snapshot: &StateSnapshot) -> Result<Vec<u8>, String>;

    /// Decode bytes into a state snapshot.
    fn decode(&self, data: &[u8]) -> Result<StateSnapshot, String>;
}

/// Trait for persisting and retrieving state snapshots.
///
/// Users provide their own implementation (S3, Redis, local filesystem, etc.)
/// to integrate with their infrastructure.
pub trait StateStore: Send + Sync {
    /// Save a snapshot for a given key.
    fn save(&self, key: &[u8], snapshot: &StateSnapshot) -> Result<(), String>;

    /// Load the most recent snapshot for a given key.
    fn load(&self, key: &[u8]) -> Result<Option<StateSnapshot>, String>;

    /// List all keys that have snapshots.
    fn list_keys(&self) -> Result<Vec<Vec<u8>>, String>;
}

/// Structured data embedded in a `StateSnapshot`.
///
/// This is serialized/deserialized by [`CompiledGraph::snapshot()`] and
/// [`CompiledGraph::restore()`] to persist and restore computation state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotData {
    /// Number of records processed at snapshot time.
    pub records_seen: usize,
    /// Minimum warmup period.
    pub min_warmup: usize,
    /// Number of nodes in the graph (topology verification).
    pub node_count: usize,
    /// Number of output nodes (topology verification).
    pub output_count: usize,
}

/// Policy for handling out-of-order records within a keyed partition.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutOfOrderPolicy {
    /// Error immediately if a record arrives out of order.
    Error,
    /// Drop out-of-order records silently.
    Drop,
    /// Buffer out-of-order records up to a maximum lateness window.
    Buffer {
        /// Maximum lateness in milliseconds.
        max_lateness_ms: i64,
    },
}

/// Keyed execution state for a single key.
pub struct KeyedGraphState<R, O, K>
where
    K: Clone + Send + Sync + Default + std::hash::Hash + Eq + 'static,
    O: ExtractOutput,
{
    pub(crate) graph: CompiledGraph<R, O, KeyedTimestamped<K>>,
    pub(crate) last_ts: Option<i64>,
    pub(crate) policy: OutOfOrderPolicy,
}

impl<R, O, K> std::fmt::Debug for KeyedGraphState<R, O, K>
where
    K: Clone + Send + Sync + Default + std::hash::Hash + Eq + 'static,
    O: ExtractOutput,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("KeyedGraphState")
            .field("graph", &self.graph)
            .field("last_ts", &self.last_ts)
            .field("policy", &self.policy)
            .finish()
    }
}

impl<I, R, O, K, C> std::fmt::Debug for TFloKeyedIter<I, R, O, K, C>
where
    I: std::fmt::Debug,
    K: Clone + Send + Sync + Default + std::hash::Hash + Eq + 'static,
    O: ExtractOutput,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TFloKeyedIter")
            .field("iter", &self.iter)
            .field("policy", &self.policy)
            .finish()
    }
}

impl<R, O, K> KeyedGraphState<R, O, K>
where
    K: Clone + Send + Sync + Default + std::hash::Hash + Eq + 'static,
    O: ExtractOutput,
{
    /// Create a new keyed graph state.
    pub fn new(graph: CompiledGraph<R, O, KeyedTimestamped<K>>, policy: OutOfOrderPolicy) -> Self {
        Self {
            graph,
            last_ts: None,
            policy,
        }
    }

    /// Step the graph with one record, handling out-of-order policy.
    pub fn step(
        &mut self,
        record: &R,
        ts: i64,
        key: K,
    ) -> Result<Option<PipelineItem<KeyedTimestamped<K>, O>>, ComputeError> {
        // Check out-of-order policy
        if let Some(last) = self.last_ts {
            if ts < last {
                match self.policy {
                    OutOfOrderPolicy::Error => {
                        return Err(ComputeError::InvalidInput {
                            reason: "out-of-order timestamp",
                        });
                    }
                    OutOfOrderPolicy::Drop => return Ok(None),
                    OutOfOrderPolicy::Buffer { max_lateness_ms } => {
                        if ts < last - max_lateness_ms {
                            // Too late, drop it
                            return Ok(None);
                        }
                        // Within lateness window, buffer it (for now, just process it)
                        // TODO: Implement proper buffering
                    }
                }
            }
        }
        self.last_ts = Some(ts);

        // Create keyed context
        let ctx = KeyedTimestamped::new(ts, key);

        // Step the graph with the pre-created keyed context
        match self.graph.step_with_context(record, ts, ctx) {
            StepResult::Ready(item) => Ok(Some(item)),
            StepResult::WarmingUp { .. } => Ok(None),
            StepResult::Error(e) => Err(e),
        }
    }
}

/// Iterator adapter for keyed temporal computations.
///
/// Routes records to per-key computation graphs, ensuring state isolation
/// while preserving key attribution in outputs.
pub struct TFloKeyedIter<I, R, O, K, C>
where
    K: Clone + Send + Sync + Default + std::hash::Hash + Eq + 'static,
    O: ExtractOutput,
{
    pub(crate) iter: I,
    pub(crate) graphs: HashMap<K, KeyedGraphState<R, O, K>>,
    pub(crate) timestamp_fn: Arc<dyn Fn(&R) -> i64 + Send + Sync>,
    pub(crate) key_fn: Arc<dyn Fn(&R) -> K + Send + Sync>,
    pub(crate) builder_fn: Box<dyn Fn(&mut TFlowBuilder<R>) -> C + Send + Sync>,
    pub(crate) policy: OutOfOrderPolicy,
    pub(crate) _marker: std::marker::PhantomData<(R, O)>,
}

impl<I, R, O, K, C> Iterator for TFloKeyedIter<I, R, O, K, C>
where
    I: Iterator<Item = R>,
    K: Hash + Eq + Clone + Send + Sync + Default + 'static,
    O: ExtractOutput,
    C: Compile<R>,
    C::Output: ExtractOutput,
    R: 'static,
{
    type Item = TFloResult<PipelineItem<KeyedTimestamped<K>, O>>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let record = self.iter.next()?;
            let ts = (self.timestamp_fn)(&record);
            let key = (self.key_fn)(&record);

            // Get or create graph for this key
            let graph_state = self.graphs.entry(key.clone()).or_insert_with(|| {
                let mut builder = TFlowBuilder::new();
                let ts_fn = self.timestamp_fn.clone();
                builder.timestamp(move |r| ts_fn(r));
                let comps = (self.builder_fn)(&mut builder);
                let output_ids = comps.output_ids();
                let timestamp_fn = builder
                    .get_timestamp_fn()
                    .unwrap_or_else(|| self.timestamp_fn.clone());
                let nodes = builder.into_nodes();
                let graph: CompiledGraph<R, O, KeyedTimestamped<K>> =
                    CompiledGraph::compile(timestamp_fn, nodes, output_ids);
                KeyedGraphState::new(graph, self.policy)
            });

            match graph_state.step(&record, ts, key) {
                Ok(Some(item)) => return Some(Ok(item)),
                Ok(None) => continue, // Warmup or dropped
                Err(e) => return Some(Err(TFloError::Compute(e))),
            }
        }
    }
}
