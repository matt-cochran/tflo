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
use std::collections::{HashMap, VecDeque};
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
    /// Timestamp of the most recently *released* (graph-processed) record.
    pub(crate) last_ts: Option<i64>,
    pub(crate) policy: OutOfOrderPolicy,
    /// Records buffered by [`OutOfOrderPolicy::Buffer`], kept sorted ascending
    /// by timestamp (stable — ties preserve arrival order). Empty for the
    /// `Error` and `Drop` policies.
    pub(crate) pending: Vec<(i64, R)>,
    /// Highest timestamp seen so far — the basis for the release watermark.
    pub(crate) max_ts_seen: Option<i64>,
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
            pending: Vec::new(),
            max_ts_seen: None,
        }
    }

    /// Run one record through the graph, producing at most one output.
    fn run_one(
        &mut self,
        record: &R,
        ts: i64,
        key: K,
    ) -> Result<Option<PipelineItem<KeyedTimestamped<K>, O>>, ComputeError> {
        let ctx = KeyedTimestamped::new(ts, key);
        match self.graph.step_with_context(record, ts, ctx) {
            StepResult::Ready(item) => Ok(Some(item)),
            StepResult::WarmingUp { .. } => Ok(None),
            StepResult::Error(e) => Err(e),
        }
    }

    /// Release every buffered record with timestamp `<= watermark`, in order.
    fn drain_until(
        &mut self,
        watermark: i64,
        key: &K,
    ) -> Result<Vec<PipelineItem<KeyedTimestamped<K>, O>>, ComputeError> {
        let mut released = Vec::new();
        while self
            .pending
            .first()
            .is_some_and(|(bts, _)| *bts <= watermark)
        {
            let (bts, record) = self.pending.remove(0);
            self.last_ts = Some(bts);
            if let Some(item) = self.run_one(&record, bts, key.clone())? {
                released.push(item);
            }
        }
        Ok(released)
    }

    /// Step the graph with one record, applying the out-of-order policy.
    ///
    /// Returns every record this step *releases* — usually zero or one, but a
    /// [`Buffer`](OutOfOrderPolicy::Buffer) step can release several at once
    /// when an advancing watermark unblocks earlier buffered records.
    pub fn step(
        &mut self,
        record: R,
        ts: i64,
        key: K,
    ) -> Result<Vec<PipelineItem<KeyedTimestamped<K>, O>>, ComputeError> {
        match self.policy {
            OutOfOrderPolicy::Error => {
                if self.last_ts.is_some_and(|last| ts < last) {
                    return Err(ComputeError::InvalidInput {
                        reason: "out-of-order timestamp",
                    });
                }
                self.last_ts = Some(ts);
                Ok(self.run_one(&record, ts, key)?.into_iter().collect())
            }
            OutOfOrderPolicy::Drop => {
                if self.last_ts.is_some_and(|last| ts < last) {
                    return Ok(Vec::new());
                }
                self.last_ts = Some(ts);
                Ok(self.run_one(&record, ts, key)?.into_iter().collect())
            }
            OutOfOrderPolicy::Buffer { max_lateness_ms } => {
                // A record already behind the released frontier can never be
                // emitted in order — drop it.
                if self.last_ts.is_some_and(|last| ts < last) {
                    return Ok(Vec::new());
                }
                // Insert sorted; `partition_point` places the record after any
                // equal timestamps, preserving arrival order on ties.
                let pos = self.pending.partition_point(|(bts, _)| *bts <= ts);
                self.pending.insert(pos, (ts, record));
                self.max_ts_seen = Some(self.max_ts_seen.map_or(ts, |m| m.max(ts)));
                let watermark = self.max_ts_seen.unwrap_or(ts) - max_lateness_ms;
                self.drain_until(watermark, &key)
            }
        }
    }

    /// Release every remaining buffered record, in timestamp order.
    ///
    /// Call this once at end-of-stream so [`Buffer`](OutOfOrderPolicy::Buffer)
    /// records still inside the lateness window are not silently lost. It is a
    /// no-op for the `Error` and `Drop` policies (their buffers stay empty).
    pub fn flush(
        &mut self,
        key: K,
    ) -> Result<Vec<PipelineItem<KeyedTimestamped<K>, O>>, ComputeError> {
        let mut released = Vec::new();
        for (bts, record) in std::mem::take(&mut self.pending) {
            self.last_ts = Some(bts);
            if let Some(item) = self.run_one(&record, bts, key.clone())? {
                released.push(item);
            }
        }
        Ok(released)
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
    /// Records released by the most recent step(s) but not yet yielded — a
    /// single `Buffer` step can release several records at once.
    pub(crate) ready_queue: VecDeque<TFloResult<PipelineItem<KeyedTimestamped<K>, O>>>,
    /// Set once the input is exhausted and every key has been flushed.
    pub(crate) flushed: bool,
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
            // Serve anything released by an earlier step or by flushing.
            if let Some(result) = self.ready_queue.pop_front() {
                return Some(result);
            }

            let Some(record) = self.iter.next() else {
                // Input exhausted — flush every key's buffered records once so
                // `Buffer`-policy records inside the lateness window survive.
                if self.flushed {
                    return None;
                }
                self.flushed = true;
                for (key, graph_state) in self.graphs.iter_mut() {
                    match graph_state.flush(key.clone()) {
                        Ok(items) => self.ready_queue.extend(items.into_iter().map(Ok)),
                        Err(e) => {
                            self.ready_queue.push_back(Err(TFloError::Compute(e)));
                        }
                    }
                }
                continue;
            };

            let ts = (self.timestamp_fn)(&record);
            let key = (self.key_fn)(&record);

            // Get or create the graph for this key.
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

            match graph_state.step(record, ts, key) {
                Ok(items) => self.ready_queue.extend(items.into_iter().map(Ok)),
                Err(e) => self.ready_queue.push_back(Err(TFloError::Compute(e))),
            }
        }
    }
}
