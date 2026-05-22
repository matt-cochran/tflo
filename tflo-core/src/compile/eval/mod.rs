mod ctx;
// The `eval` submodule shares its parent's name — a deliberate
// `eval/{ctx,eval}.rs` split of the evaluation code.
#[allow(clippy::module_inception)]
mod eval;

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
    ///         StepResult::WarmingUp { remaining, .. } => println!("Warming up, {} more needed", remaining),
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
                reason: crate::compile::Absent::WarmingUp,
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

        // Extract typed output and wrap in PipelineItem. Extraction only fails
        // when an output node has not been evaluated yet (a composition node
        // still warming up) — a base node always stores a `Computed`, and an
        // absent base node still extracts (as `NaN` for `O = f64`, or the
        // typed reason for `O = Computed`).
        match O::extract(&self.store, &self.output_ids) {
            Some(value) => StepResult::Ready(PipelineItem { ctx, value }),
            None => StepResult::WarmingUp {
                remaining: self.min_warmup.saturating_sub(self.records_seen),
                reason: crate::compile::Absent::WarmingUp,
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
    /// Returns a `StateSnapshot` — the full per-node state encoded with
    /// `postcard` — that can be persisted and later passed to
    /// [`restore()`](Self::restore).
    ///
    /// # Errors
    ///
    /// Returns [`ComputeError::InvalidInput`](crate::error::ComputeError::InvalidInput)
    /// if the graph contains state that cannot be captured: a `scan`/`scan2`
    /// node, a `fold` composition node, or an
    /// [`Operator`](crate::operator::Operator) plugin node that does not
    /// override [`save`](crate::operator::Operator::save). The snapshot is
    /// all-or-nothing — it never writes a partial checkpoint.
    ///
    /// # Usage
    ///
    /// ```ignore
    /// let snapshot = graph.snapshot()?;
    /// // persist `snapshot` somewhere
    /// // ... later ...
    /// let mut restored = CompiledGraph::compile(ts_fn, nodes, output_ids);
    /// restored.restore(&snapshot)?;
    /// ```
    pub fn snapshot(&self) -> Result<crate::keyed::StateSnapshot, crate::error::ComputeError> {
        use super::snapshot::GraphSnapshot;
        use crate::error::ComputeError;
        use crate::keyed::{SnapshotMetadata, StateSnapshot};

        // `fold` composition nodes hold opaque accumulator state; `map` nodes
        // are stateless and fine.
        if self
            .composition_nodes
            .iter()
            .any(|e| matches!(e.kind, super::CompositionNodeKind::Fold { .. }))
        {
            return Err(ComputeError::InvalidInput {
                reason: "graph has a fold composition node, which cannot be checkpointed",
            });
        }

        let mut node_states = Vec::with_capacity(self.nodes.len());
        for node in &self.nodes {
            match node.state.to_snapshot() {
                Some(s) => node_states.push(s),
                None => {
                    return Err(ComputeError::InvalidInput {
                        reason: "graph has a non-checkpointable node (scan/scan2, or a \
                                 custom node without save())",
                    });
                }
            }
        }

        let graph = GraphSnapshot {
            records_seen: self.records_seen,
            min_warmup: self.min_warmup,
            node_count: self.nodes.len(),
            output_count: self.output_ids.len(),
            node_states,
        };
        let data = postcard::to_stdvec(&graph).map_err(|_| ComputeError::InvalidInput {
            reason: "snapshot encoding failed",
        })?;

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

        Ok(StateSnapshot {
            data,
            metadata: SnapshotMetadata {
                key: None,
                timestamp_ms,
                version: 1,
            },
        })
    }

    /// Restore computation state from a previously taken snapshot.
    ///
    /// The snapshot must have been taken from an identically structured
    /// compiled graph (same node topology, same output IDs). Every node's
    /// state — window buffers, accumulators, detector state machines — is
    /// restored.
    ///
    /// # Errors
    ///
    /// Returns [`ComputeError::InvalidInput`](crate::error::ComputeError::InvalidInput)
    /// if the snapshot bytes are malformed, or if the graph structure does not
    /// match the snapshot (different node count, output count, or a node whose
    /// kind differs from the snapshot's).
    pub fn restore(
        &mut self,
        snapshot: &crate::keyed::StateSnapshot,
    ) -> Result<(), crate::error::ComputeError> {
        use super::snapshot::GraphSnapshot;
        use crate::error::ComputeError;

        let graph: GraphSnapshot =
            postcard::from_bytes(&snapshot.data).map_err(|_| ComputeError::InvalidInput {
                reason: "snapshot data invalid: expected GraphSnapshot format",
            })?;

        // Verify graph structure compatibility.
        if graph.node_count != self.nodes.len() || graph.node_states.len() != self.nodes.len() {
            return Err(ComputeError::InvalidInput {
                reason: "snapshot node count mismatch: graph topology has changed",
            });
        }
        if graph.output_count != self.output_ids.len() {
            return Err(ComputeError::InvalidInput {
                reason: "snapshot output count mismatch: graph topology has changed",
            });
        }

        for (index, (node, snap)) in self.nodes.iter_mut().zip(graph.node_states).enumerate() {
            snap.apply_to(&mut node.state, index)
                .map_err(|_| ComputeError::InvalidInput {
                    reason: "snapshot node-state mismatch: graph topology has changed",
                })?;
        }

        self.records_seen = graph.records_seen;
        self.min_warmup = graph.min_warmup;

        // Clear the value store to reset cached evaluations.
        self.store.clear();

        Ok(())
    }
}
