//! Keyed execution support for tflo-core.
//!
//! This module provides APIs for partitioning records by key and running
//! separate computation graphs per key, ensuring state isolation while
//! preserving key attribution in the pipeline context.

use crate::builder::{Compile, TFlowBuilder};
use crate::compile::{CompiledGraph, ExtractOutput, StepResult};
use crate::error::{ComputeError, TFloError, TFloResult};
use crate::pipeline::{KeyedTimestamped, PipelineItem};
use crate::timer::TimerService;
use serde::{Deserialize, Serialize};
use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap, VecDeque};
use std::hash::Hash;
use std::sync::Arc;

/// Maximum allowed `max_lateness_ms` for [`OutOfOrderPolicy::Buffer`].
/// 24 hours. Looser semantics should use [`OutOfOrderPolicy::Drop`].
///
/// Why a ceiling: the `pending` buffer is O(log n) per op
/// (`BinaryHeap`-backed in Phase 3a), but unbounded `max_lateness_ms`
/// lets adversarial input grow the per-key heap without bound and
/// arbitrarily delay timer fires that depend on watermark advance.
/// 24h is a deliberate ceiling that comfortably covers daily-batch
/// upstream catchups; multi-day-late records belong in a separate
/// reprocessing pipeline, not the live stream.
pub const MAX_LATENESS_MS: i64 = 24 * 60 * 60 * 1000;

/// Heap entry for the per-key out-of-order buffer. Ordered by
/// `(ts, seq)` so [`BinaryHeap::pop`] (with `Reverse` wrapping) returns
/// the smallest `ts`, with ties broken by arrival order — deterministic
/// across runs and snapshot/restore boundaries.
pub(crate) struct PendingEntry<R> {
    pub(crate) ts: i64,
    pub(crate) seq: u64,
    pub(crate) record: R,
}

impl<R> PartialEq for PendingEntry<R> {
    fn eq(&self, other: &Self) -> bool {
        self.ts == other.ts && self.seq == other.seq
    }
}
impl<R> Eq for PendingEntry<R> {}
impl<R> PartialOrd for PendingEntry<R> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
impl<R> Ord for PendingEntry<R> {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.ts.cmp(&other.ts).then(self.seq.cmp(&other.seq))
    }
}

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
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SnapshotMetadata {
    /// The key this snapshot belongs to (if keyed execution).
    pub key: Option<Vec<u8>>,
    /// Timestamp when snapshot was taken (milliseconds since epoch).
    pub timestamp_ms: i64,
    /// Version identifier for snapshot format compatibility.
    pub version: u32,
    /// Topology fingerprint of the builder that produced this snapshot.
    ///
    /// `None` for snapshots produced by callers that didn't supply one
    /// (back-compat). `Some(_)` for new snapshots produced via the
    /// [`Checkpointer`](crate::state::Checkpointer) Phase 1 path. On
    /// restore, mismatched fingerprints **must** be rejected — see
    /// [`TFlowBuilder::fingerprint`](crate::builder::TFlowBuilder::fingerprint).
    #[serde(default)]
    pub topology_fingerprint: Option<[u8; 32]>,
}

/// Trait for encoding/decoding state snapshots.
///
/// Implementations can use serde, bincode, postcard, or any other
/// serialization format. This trait allows users to choose their preferred
/// codec without tflo-core depending on specific serialization libraries.
pub trait SnapshotCodec: Send + Sync {
    /// Encode a state snapshot to bytes.
    ///
    /// # Errors
    ///
    /// Returns an error string when the codec cannot serialize the snapshot
    /// (unsupported value, allocation failure, schema mismatch, etc.).
    fn encode(&self, snapshot: &StateSnapshot) -> Result<Vec<u8>, String>;

    /// Decode bytes into a state snapshot.
    ///
    /// # Errors
    ///
    /// Returns an error string when `data` is not a valid encoding produced
    /// by this codec (truncated bytes, version mismatch, malformed payload).
    fn decode(&self, data: &[u8]) -> Result<StateSnapshot, String>;
}

/// Trait for persisting and retrieving state snapshots.
///
/// Users provide their own implementation (S3, Redis, local filesystem, etc.)
/// to integrate with their infrastructure.
pub trait StateStore: Send + Sync {
    /// Save a snapshot for a given key.
    ///
    /// # Errors
    ///
    /// Returns an error string when the underlying backend cannot persist
    /// the snapshot (I/O failure, network timeout, permission denied, etc.).
    fn save(&self, key: &[u8], snapshot: &StateSnapshot) -> Result<(), String>;

    /// Load the most recent snapshot for a given key.
    ///
    /// # Errors
    ///
    /// Returns an error string when the underlying backend cannot be queried.
    /// A missing snapshot is reported as `Ok(None)`, not an error.
    fn load(&self, key: &[u8]) -> Result<Option<StateSnapshot>, String>;

    /// List all keys that have snapshots.
    ///
    /// # Errors
    ///
    /// Returns an error string when the underlying backend cannot be
    /// enumerated.
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
    ///
    /// # Performance and bounds (current implementation)
    ///
    /// The `pending` buffer is backed by a `Vec<(i64, R)>` kept
    /// sorted by timestamp. Insertion and front-drain are O(n) in
    /// the buffer length: a sustained burst of late-arriving records
    /// to a single key degrades quadratically. This is acceptable
    /// for typical sensor / detection workloads (a few records of
    /// reorder) but is a known limit at high lateness or large
    /// bursts. The closure plan replaces the `Vec` with a
    /// `BinaryHeap` in Phase 3a, which lifts the bound to
    /// O(log n) per op.
    ///
    /// `max_lateness_ms` is currently unbounded (`i64`). A future
    /// release adds a 24-hour ceiling and refuses construction
    /// above it; users wanting looser semantics should use
    /// [`OutOfOrderPolicy::Drop`] instead.
    Buffer {
        /// Maximum lateness in milliseconds. See variant docs for
        /// current bounds and performance caveats.
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
    /// Records buffered by [`OutOfOrderPolicy::Buffer`], kept in a
    /// min-heap by `(ts, seq)`. `pop` returns the smallest `ts`, with
    /// arrival-order tie-breaking. Empty for `Error` and `Drop`
    /// policies. O(log n) insert + drain (vs. the Phase 2 `Vec`'s O(n)).
    pub(crate) pending: BinaryHeap<Reverse<PendingEntry<R>>>,
    /// Monotonic counter for [`PendingEntry::seq`] — the heap's
    /// arrival-order tie-breaker.
    pub(crate) pending_seq: u64,
    /// Highest timestamp seen so far — the basis for the release watermark.
    pub(crate) max_ts_seen: Option<i64>,
    /// Per-key event-time timer service. Owns the heap of pending timers
    /// and is passed by `&mut` into `CompiledGraph` evaluation paths so
    /// operators can register and delete timers via [`TimerCtx`](crate::timer::TimerCtx).
    pub(crate) timer_service: TimerService,
    /// Records that were dropped because they arrived too late — either
    /// past the release frontier under [`OutOfOrderPolicy::Buffer`], or
    /// strictly less than `last_ts` under [`OutOfOrderPolicy::Drop`].
    /// Captured opt-in: callers must call [`take_late_records`](Self::take_late_records)
    /// to drain. Empty for [`OutOfOrderPolicy::Error`] (errors are
    /// surfaced inline as `Err`).
    pub(crate) late_records: Vec<(i64, R)>,
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
            pending: BinaryHeap::new(),
            pending_seq: 0,
            max_ts_seen: None,
            timer_service: TimerService::default(),
            late_records: Vec::new(),
        }
    }

    /// Drain every late-arriving record this state has dropped since the
    /// last drain. A record is considered *late* when it arrives with a
    /// timestamp strictly less than the most recent released frontier
    /// (under [`OutOfOrderPolicy::Drop`]) or behind the buffer's release
    /// watermark (under [`OutOfOrderPolicy::Buffer`]).
    ///
    /// Returned tuples are `(record_ts, record)`. Order is arrival order
    /// (not necessarily timestamp order). Empty for
    /// [`OutOfOrderPolicy::Error`] streams, which surface late records
    /// inline as `Err(ComputeError::InvalidInput { ... })` instead.
    pub fn take_late_records(&mut self) -> Vec<(i64, R)> {
        std::mem::take(&mut self.late_records)
    }

    /// Run one record through the graph, producing zero or more outputs.
    ///
    /// Returns:
    /// - Items emitted by event-time timers that became due *strictly
    ///   before* this record (their `fire_ts` is in `(last_released_ts, ts]`),
    ///   in `(fire_ts, registration_seq)` order, followed by
    /// - At most one item from the record's own step.
    ///
    /// Timer-fired items carry the timer's `fire_ts` as their context
    /// timestamp, not the record's `ts`, so downstream consumers see the
    /// canonical event-time order.
    fn run_one(
        &mut self,
        record: &R,
        ts: i64,
        key: K,
    ) -> Result<Vec<PipelineItem<KeyedTimestamped<K>, O>>, ComputeError> {
        let mut out: Vec<PipelineItem<KeyedTimestamped<K>, O>> = Vec::new();

        // 1. Fire any timers due strictly before this record's ts. (Timers
        //    at fire_ts == ts go in the same logical instant as the record;
        //    we fire them before the record so the record's eval sees the
        //    post-fire store state.)
        let due_items = self.graph.fire_due_timers(&mut self.timer_service, ts);
        for (fire_ts, value) in due_items {
            out.push(PipelineItem {
                ctx: KeyedTimestamped::new(fire_ts, key.clone()),
                value,
            });
        }

        // 2. Run the record's eval (plugin nodes get the TimerCtx so they
        //    can register/delete timers for future fire_ts).
        let ctx = KeyedTimestamped::new(ts, key);
        match self
            .graph
            .step_with_context_and_timers(record, ts, ctx, &mut self.timer_service)
        {
            StepResult::Ready(item) => {
                out.push(item);
                Ok(out)
            }
            StepResult::WarmingUp { .. } => Ok(out),
            StepResult::Error(e) => Err(e),
        }
    }

    /// Release every buffered record with timestamp `<= watermark`, in
    /// `(ts, seq)` order. O(k log n) for `k` released entries.
    fn drain_until(
        &mut self,
        watermark: i64,
        key: &K,
    ) -> Result<Vec<PipelineItem<KeyedTimestamped<K>, O>>, ComputeError> {
        let mut released = Vec::new();
        while self
            .pending
            .peek()
            .is_some_and(|Reverse(e)| e.ts <= watermark)
        {
            let entry = self
                .pending
                .pop()
                .expect("peek-then-pop is total over BinaryHeap")
                .0;
            self.last_ts = Some(entry.ts);
            released.extend(self.run_one(&entry.record, entry.ts, key.clone())?);
        }
        Ok(released)
    }

    /// Step the graph with one record, applying the out-of-order policy.
    ///
    /// Returns every record this step *releases* — usually zero or one, but a
    /// [`Buffer`](OutOfOrderPolicy::Buffer) step can release several at once
    /// when an advancing watermark unblocks earlier buffered records.
    ///
    /// # Errors
    ///
    /// Returns [`ComputeError::InvalidInput`] when the policy is
    /// [`OutOfOrderPolicy::Error`] and `ts` precedes the most recently
    /// released timestamp. Propagates any [`ComputeError`] raised by the
    /// underlying graph step.
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
                self.run_one(&record, ts, key)
            }
            OutOfOrderPolicy::Drop => {
                if self.last_ts.is_some_and(|last| ts < last) {
                    self.late_records.push((ts, record));
                    return Ok(Vec::new());
                }
                self.last_ts = Some(ts);
                self.run_one(&record, ts, key)
            }
            OutOfOrderPolicy::Buffer { max_lateness_ms } => {
                // Bound `max_lateness_ms` at construction-of-use time. A
                // value above `MAX_LATENESS_MS` would let an adversary grow
                // the per-key heap and arbitrarily delay timer fires.
                if max_lateness_ms < 0 || max_lateness_ms > MAX_LATENESS_MS {
                    return Err(ComputeError::InvalidInput {
                        reason: "max_lateness_ms must be in [0, 24h]; \
                                 use OutOfOrderPolicy::Drop for looser semantics",
                    });
                }
                // A record already behind the released frontier can never be
                // emitted in order — drop it (captured for opt-in side output).
                if self.last_ts.is_some_and(|last| ts < last) {
                    self.late_records.push((ts, record));
                    return Ok(Vec::new());
                }
                // O(log n) heap insert. `seq` preserves arrival order
                // across equal `ts`.
                let seq = self.pending_seq;
                self.pending_seq = self.pending_seq.saturating_add(1);
                self.pending.push(Reverse(PendingEntry { ts, seq, record }));
                self.max_ts_seen = Some(self.max_ts_seen.map_or(ts, |m| m.max(ts)));
                // Saturating: if `max_ts_seen < max_lateness_ms` (early
                // in the stream, or a small timestamp epoch), the
                // watermark clamps at `i64::MIN` — semantically "no
                // records are late yet", which is exactly the right
                // behavior for `drain_until` (nothing flushes).
                let watermark = self
                    .max_ts_seen
                    .unwrap_or(ts)
                    .saturating_sub(max_lateness_ms);
                self.drain_until(watermark, &key)
            }
        }
    }

    /// Release every remaining buffered record, in timestamp order.
    ///
    /// Call this once at end-of-stream so [`Buffer`](OutOfOrderPolicy::Buffer)
    /// records still inside the lateness window are not silently lost. It is a
    /// no-op for the `Error` and `Drop` policies (their buffers stay empty).
    ///
    /// # Errors
    ///
    /// Propagates any [`ComputeError`] raised while draining buffered
    /// records through the underlying graph step.
    pub fn flush(
        &mut self,
        key: K,
    ) -> Result<Vec<PipelineItem<KeyedTimestamped<K>, O>>, ComputeError> {
        let mut released = Vec::new();
        // Drain pending buffered records first, in `(ts, seq)` order.
        let heap = std::mem::take(&mut self.pending);
        let mut entries: Vec<PendingEntry<R>> = heap.into_iter().map(|r| r.0).collect();
        entries.sort();
        for entry in entries {
            self.last_ts = Some(entry.ts);
            released.extend(self.run_one(&entry.record, entry.ts, key.clone())?);
        }
        // Then fire any still-registered timers in `(fire_ts, seq)` order.
        // `drain_all` returns them sorted; we re-register each and let
        // `fire_due_timers` pop and dispatch through the graph.
        //
        // `last_ts` is *not* decreased by a timer fire whose `fire_ts` is
        // smaller than a previously-released record's `ts`. The watermark
        // contract is monotonically non-decreasing; out-of-event-time
        // timer fires (which only arise at end-of-stream when a registered
        // timer's fire_ts pre-dates a later record) keep the existing
        // last_ts.
        let remaining: Vec<crate::timer::TimerEntry> = self.timer_service.drain_all();
        for entry in remaining {
            self.timer_service.register(entry.fire_ts, entry.node_id);
            let fired = self.graph.fire_due_timers(&mut self.timer_service, entry.fire_ts);
            for (fire_ts, value) in fired {
                self.last_ts = Some(self.last_ts.map_or(fire_ts, |last| last.max(fire_ts)));
                released.push(PipelineItem {
                    ctx: KeyedTimestamped::new(fire_ts, key.clone()),
                    value,
                });
            }
        }
        Ok(released)
    }

    /// Advance the per-key event-time watermark without consuming a record.
    ///
    /// Fires every registered timer with `fire_ts <= ts` in
    /// `(fire_ts, registration_seq)` order. Useful when the source stream
    /// is sparse and the caller needs absence-of-event detection (e.g.,
    /// a pulse-width detector emitting `TooLong` when no closing record
    /// arrives within the configured bound).
    ///
    /// `ts` is wrapped in [`EventTimeMs`](crate::timer::EventTimeMs) so the
    /// caller cannot accidentally pass a processing-time value
    /// (`SystemTime::now`-style wall clock) on an event-time stream.
    ///
    /// # Errors
    ///
    /// Returns [`ComputeError::NonMonotonicWatermark`] if `ts` is strictly
    /// less than a previously-advanced watermark for this key — the call
    /// would attempt to move the watermark backward, which violates
    /// event-time ordering.
    pub fn advance_event_time_watermark(
        &mut self,
        ts: crate::timer::EventTimeMs,
        key: K,
    ) -> Result<Vec<PipelineItem<KeyedTimestamped<K>, O>>, ComputeError> {
        let ts_raw: i64 = ts.into();
        if let Some(last) = self.last_ts {
            if ts_raw < last {
                return Err(ComputeError::NonMonotonicWatermark {
                    last,
                    attempted: ts_raw,
                });
            }
        }
        self.last_ts = Some(ts_raw);
        self.max_ts_seen = Some(self.max_ts_seen.map_or(ts_raw, |m| m.max(ts_raw)));
        let due_items = self.graph.fire_due_timers(&mut self.timer_service, ts_raw);
        let mut out = Vec::with_capacity(due_items.len());
        for (fire_ts, value) in due_items {
            out.push(PipelineItem {
                ctx: KeyedTimestamped::new(fire_ts, key.clone()),
                value,
            });
        }
        Ok(out)
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

impl<I, R, O, K, C> TFloKeyedIter<I, R, O, K, C>
where
    K: Hash + Eq + Clone + Send + Sync + Default + 'static,
    O: ExtractOutput,
    R: 'static,
{
    /// Drain every late-arriving record dropped across every active key
    /// since the last drain. See
    /// [`KeyedGraphState::take_late_records`] for the per-key
    /// definition; this method aggregates across keys. Returned tuples
    /// are `(key, record_ts, record)`.
    ///
    /// Order is iteration-order across keys (implementation-defined,
    /// currently `HashMap`) followed by arrival order within each key.
    pub fn take_late_records(&mut self) -> Vec<(K, i64, R)> {
        let mut out = Vec::new();
        for (k, state) in &mut self.graphs {
            for (ts, r) in state.take_late_records() {
                out.push((k.clone(), ts, r));
            }
        }
        out
    }

    /// Advance the per-key event-time watermark on every active key
    /// without consuming a record from the source iterator.
    ///
    /// Fires every registered timer with `fire_ts <= ts` (event-time) on
    /// every key currently tracked. Useful when the source stream is
    /// sparse and the caller needs absence-of-event detection to fire
    /// promptly (a pulse-width detector emitting `TooLong` when no
    /// closing record arrived within the configured bound).
    ///
    /// Returned items are appended to the iterator's ready-queue and will
    /// be yielded on subsequent `next()` calls in `(key insertion order,
    /// then fire_ts, then registration_seq)`. The relative ordering across
    /// keys is implementation-defined (currently `HashMap` iteration order);
    /// callers needing a deterministic global order across keys must
    /// host tflo inside an engine that provides a global watermark — see
    /// `docs/non-goals.md`.
    ///
    /// # Errors
    ///
    /// Returns the first per-key
    /// [`ComputeError::NonMonotonicWatermark`]
    /// encountered (a watermark cannot move backward). Any items emitted
    /// before the error are kept in the ready queue and will still be
    /// yielded on subsequent `next()` calls.
    pub fn advance_event_time_watermark(
        &mut self,
        ts: crate::timer::EventTimeMs,
    ) -> Result<(), ComputeError> {
        // Collect keys first to avoid borrowing self.graphs mutably and
        // immutably at the same time.
        let keys: Vec<K> = self.graphs.keys().cloned().collect();
        for key in keys {
            // SAFETY-of-unwrap: the key came from `self.graphs.keys()`
            // immediately above, so it must still be present.
            let graph_state = self
                .graphs
                .get_mut(&key)
                .expect("key just obtained from self.graphs.keys()");
            match graph_state.advance_event_time_watermark(ts, key.clone()) {
                Ok(items) => self.ready_queue.extend(items.into_iter().map(Ok)),
                Err(e) => return Err(e),
            }
        }
        Ok(())
    }
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
                for (key, graph_state) in &mut self.graphs {
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
                let fingerprint = builder.fingerprint();
                let timestamp_fn = builder
                    .get_timestamp_fn()
                    .unwrap_or_else(|| self.timestamp_fn.clone());
                let nodes = builder.into_nodes();
                let graph: CompiledGraph<R, O, KeyedTimestamped<K>> =
                    CompiledGraph::compile(timestamp_fn, nodes, output_ids)
                        .with_topology_fingerprint(fingerprint);
                KeyedGraphState::new(graph, self.policy)
            });

            match graph_state.step(record, ts, key) {
                Ok(items) => self.ready_queue.extend(items.into_iter().map(Ok)),
                Err(e) => self.ready_queue.push_back(Err(TFloError::Compute(e))),
            }
        }
    }
}
