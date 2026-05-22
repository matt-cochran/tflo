mod ctx;
mod eval;
mod helpers;

use self::ctx::CompilationCtx;
use crate::comp::Node;
use crate::comp::NodeId;
use crate::compile::{
    CompiledGraph, CompiledNode, CompositionNodeKind, ExtractOutput, GraphPlan, GraphStateSummary,
    StepResult, ValueStore,
};
use crate::pipeline::{PipelineContext, PipelineItem};
use std::collections::HashSet;
use std::marker::PhantomData;
use std::sync::Arc;

impl<R, O, C: PipelineContext> CompiledGraph<R, O, C> {
    /// Create a compiled graph from a builder.
    ///
    /// Debug assertions check that every output ID references a real node and
    /// that node IDs are unique.
    pub fn compile(
        timestamp_fn: Arc<dyn Fn(&R) -> i64 + Send + Sync>,
        nodes: Vec<(NodeId, Node<R>)>,
        output_ids: Vec<NodeId>,
    ) -> Self {
        // Verify every output ID references a real node.
        let node_ids: HashSet<NodeId> = nodes.iter().map(|(id, _)| *id).collect();
        for output_id in &output_ids {
            debug_assert!(
                node_ids.contains(output_id),
                "Output node ID {} not found in graph nodes",
                output_id.0
            );
        }

        let ctx = CompilationCtx::<R>::new();
        let compiled_nodes: Vec<CompiledNode<R>> = nodes
            .into_iter()
            .map(|(id, node)| ctx.compile_node(id, node))
            .collect();

        // Verify there are no duplicate node IDs.
        let mut seen_ids = HashSet::new();
        for node in &compiled_nodes {
            debug_assert!(
                seen_ids.insert(node.id),
                "Duplicate node ID {} detected during compilation",
                node.id.0
            );
        }

        Self {
            timestamp_fn,
            nodes: compiled_nodes,
            composition_nodes: Vec::new(),
            output_ids,
            store: ValueStore::new(),
            records_seen: 0,
            min_warmup: 1,
            _phantom: PhantomData,
        }
    }

    /// Create a compiled graph with a specific context type.
    ///
    /// This is useful when you want to explicitly specify the context type,
    /// e.g., for sequence-based pipelines.
    pub fn compile_with_context(
        timestamp_fn: Arc<dyn Fn(&R) -> i64 + Send + Sync>,
        nodes: Vec<(NodeId, Node<R>)>,
        output_ids: Vec<NodeId>,
    ) -> Self {
        Self::compile(timestamp_fn, nodes, output_ids)
    }

    /// Get the total number of nodes in the graph.
    #[must_use]
    pub fn node_count(&self) -> usize {
        self.nodes.len() + self.composition_nodes.len()
    }

    /// Get a debug representation of the graph structure.
    ///
    /// Returns information about the graph's nodes, dependencies, and configuration
    /// without exposing internal state. Useful for debugging and observability.
    #[must_use]
    pub fn graph_plan(&self) -> GraphPlan {
        GraphPlan {
            node_count: self.node_count(),
            base_node_count: self.nodes.len(),
            composition_node_count: self.composition_nodes.len(),
            output_count: self.output_ids.len(),
            records_seen: self.records_seen,
            min_warmup: self.min_warmup,
            warmup_remaining: self.min_warmup.saturating_sub(self.records_seen),
            context_type: std::any::type_name::<C>().to_string(),
        }
    }

    /// Get runtime state summary for observability.
    ///
    /// Returns a summary of the graph's runtime state including warmup status,
    /// node counts, and other metrics useful for monitoring and debugging.
    #[must_use]
    pub fn state_summary(&self) -> GraphStateSummary {
        GraphStateSummary {
            records_seen: self.records_seen,
            min_warmup: self.min_warmup,
            warmup_remaining: self.min_warmup.saturating_sub(self.records_seen),
            is_warmed_up: self.records_seen >= self.min_warmup,
            node_count: self.node_count(),
            output_count: self.output_ids.len(),
        }
    }

    /// Get the maximum node ID in the graph (for offset calculations).
    pub(crate) fn max_node_id(&self) -> usize {
        let base_max = self.nodes.iter().map(|n| n.id.0).max().unwrap_or(0);
        let comp_max = self
            .composition_nodes
            .iter()
            .map(|n| n.id.0)
            .max()
            .unwrap_or(0);
        base_max.max(comp_max)
    }

    /// Execute one step of the computation with typed output and status.
    ///
    /// Returns `StepResult` which explicitly indicates whether the computation
    /// is ready, still warming up, or encountered an error.
    ///
    /// # Example
    ///
    /// ```ignore
    /// for tick in ticks {
    ///     match graph.step_with_status(&tick) {
    ///         StepResult::Ready(item) => println!("At {}: value={}", item.ctx, item.value),
    ///         StepResult::WarmingUp { remaining } => println!("Warming up, {} more needed", remaining),
    ///         StepResult::Error(e) => eprintln!("Error: {}", e),
    ///     }
    /// }
    /// ```
    pub fn step_with_status(&mut self, record: &R) -> StepResult<C, O>
    where
        O: ExtractOutput,
    {
        let ts = (self.timestamp_fn)(record);
        let ctx = C::from_ordering_key(ts);
        self.step_with_context(record, ts, ctx)
    }

    /// Execute one step with a pre-created context.
    ///
    /// This is useful for keyed execution where the context needs to carry
    /// additional information (like the key) beyond just the timestamp.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let ctx = KeyedTimestamped::new(ts, key);
    /// match graph.step_with_context(&record, ts, ctx) {
    ///     StepResult::Ready(item) => { /* ... */ }
    ///     _ => { /* ... */ }
    /// }
    /// ```
    pub fn step_with_context(&mut self, record: &R, ts: i64, ctx: C) -> StepResult<C, O>
    where
        O: ExtractOutput,
    {
        self.records_seen += 1;

        // Check warmup status
        if self.records_seen < self.min_warmup {
            return StepResult::WarmingUp {
                remaining: self.min_warmup - self.records_seen,
            };
        }

        // Clear previous values
        self.store.clear();

        // Evaluate all base nodes in order
        for node in &mut self.nodes {
            let value = Self::eval_node(node, record, ts, &self.store);
            self.store.store_value(node.id, value);
        }

        // Evaluate composition nodes
        for entry in &self.composition_nodes {
            let value = match &entry.kind {
                CompositionNodeKind::Map { mapper } => mapper(&self.store),
                CompositionNodeKind::Fold { state, folder } => folder(&self.store, state),
            };
            if let Some(v) = value {
                self.store.store_value(entry.id, v);
            }
        }

        // Extract typed output and wrap in PipelineItem
        match O::extract(&self.store, &self.output_ids) {
            Some(value) => StepResult::Ready(PipelineItem { ctx, value }),
            None => StepResult::WarmingUp {
                remaining: self.min_warmup.saturating_sub(self.records_seen),
            },
        }
    }

    /// Execute one step of the computation with typed output.
    ///
    /// Returns `Some(PipelineItem<C, O>)` if the computation succeeded,
    /// `None` if extraction failed (e.g., missing values during warmup).
    ///
    /// For explicit warmup/error handling, use [`step_with_status`](Self::step_with_status).
    ///
    /// # Example
    ///
    /// ```ignore
    /// for tick in ticks {
    ///     if let Some(item) = graph.step(&tick) {
    ///         println!("At {}: value={}", item.ctx, item.value);
    ///     }
    /// }
    /// ```
    pub fn step(&mut self, record: &R) -> Option<PipelineItem<C, O>>
    where
        O: ExtractOutput,
    {
        match self.step_with_status(record) {
            StepResult::Ready(item) => Some(item),
            StepResult::WarmingUp { .. } | StepResult::Error(_) => None,
        }
    }

    /// Execute one step and return just the value (convenience method).
    ///
    /// Use this when you don't need the pipeline context.
    pub fn step_value(&mut self, record: &R) -> Option<O>
    where
        O: ExtractOutput,
    {
        self.step(record).map(|item| item.value)
    }

    /// Execute one step and return raw boxed values (for composition).
    ///
    /// This is used internally for graph composition operations.
    #[doc(hidden)]
    pub fn step_raw(&mut self, record: &R) -> i64 {
        let ts = (self.timestamp_fn)(record);
        self.records_seen += 1;

        // Clear previous values
        self.store.clear();

        // Evaluate all base nodes in order
        for node in &mut self.nodes {
            let value = Self::eval_node(node, record, ts, &self.store);
            self.store.store_value(node.id, value);
        }

        // Evaluate composition nodes
        for entry in &self.composition_nodes {
            let value = match &entry.kind {
                CompositionNodeKind::Map { mapper } => mapper(&self.store),
                CompositionNodeKind::Fold { state, folder } => folder(&self.store, state),
            };
            if let Some(v) = value {
                self.store.store_value(entry.id, v);
            }
        }

        ts
    }

    /// Get the number of records processed.
    #[must_use]
    pub fn records_seen(&self) -> usize {
        self.records_seen
    }

    /// Check if the graph is warmed up (has seen minimum required records).
    #[must_use]
    pub fn is_warmed_up(&self) -> bool {
        self.records_seen >= self.min_warmup
    }

    /// Set the minimum warmup period.
    pub fn set_min_warmup(&mut self, min: usize) {
        self.min_warmup = min;
    }

    /// Take a snapshot of the current computation state for checkpointing.
    ///
    /// Returns an opaque `StateSnapshot` that can be persisted and later
    /// passed to [`restore()`](Self::restore).
    ///
    /// The snapshot captures the graph topology (node structure, output IDs)
    /// and current runtime state (records seen, warmup status). Full node
    /// state serialization (window buffers, accumulators, etc.) is a work in
    /// progress — see [GAPS.md](../../GAPS.md#24).
    ///
    /// # Usage
    ///
    /// ```ignore
    /// let snapshot = graph.snapshot();
    /// // persist `snapshot` somewhere
    /// // ... later ...
    /// let mut restored = CompiledGraph::compile(ts_fn, nodes, output_ids);
    /// restored.restore(&snapshot).unwrap();
    /// ```
    #[must_use]
    pub fn snapshot(&self) -> crate::keyed::StateSnapshot {
        use crate::keyed::{SnapshotMetadata, StateSnapshot};

        let snapshot_data = crate::keyed::SnapshotData {
            records_seen: self.records_seen,
            min_warmup: self.min_warmup,
            // Node state serialization is not yet implemented for all 45+ variants.
            // See GAPS.md item #24 for the full implementation plan.
            node_count: self.nodes.len(),
            output_count: self.output_ids.len(),
        };

        let data = serde_json::to_vec(&snapshot_data).unwrap_or_default();

        let timestamp_ms: i64 = {
            #[cfg(not(target_arch = "wasm32"))]
            {
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_millis() as i64
            }
            #[cfg(target_arch = "wasm32")]
            {
                // SystemTime::now() is unreliable on wasm; use 0 as placeholder
                0
            }
        };

        StateSnapshot {
            data,
            metadata: SnapshotMetadata {
                key: None,
                timestamp_ms,
                version: 1,
            },
        }
    }

    /// Restore computation state from a previously taken snapshot.
    ///
    /// The snapshot must have been taken from an identically structured
    /// compiled graph (same node topology, same output IDs). Returns an
    /// error if the snapshot data is invalid or the graph structure
    /// doesn't match.
    ///
    /// Currently restores top-level metadata (records_seen, min_warmup).
    /// Full per-node state restoration is a work in progress.
    pub fn restore(
        &mut self,
        snapshot: &crate::keyed::StateSnapshot,
    ) -> Result<(), crate::error::ComputeError> {
        let snapshot_data: crate::keyed::SnapshotData = serde_json::from_slice(&snapshot.data)
            .map_err(|_| crate::error::ComputeError::InvalidInput {
                reason: "snapshot data invalid: expected SnapshotData format",
            })?;

        // Verify graph structure compatibility
        if snapshot_data.node_count != self.nodes.len() {
            return Err(crate::error::ComputeError::InvalidInput {
                reason: "snapshot node count mismatch: graph topology has changed",
            });
        }

        if snapshot_data.output_count != self.output_ids.len() {
            return Err(crate::error::ComputeError::InvalidInput {
                reason: "snapshot output count mismatch: graph topology has changed",
            });
        }

        self.records_seen = snapshot_data.records_seen;
        self.min_warmup = snapshot_data.min_warmup;

        // Clear the value store to reset cached evaluations
        self.store.clear();

        Ok(())
    }
}
