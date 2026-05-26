mod ctx;
// The `eval` submodule shares its parent's name — a deliberate
// `eval/{ctx,eval}.rs` split of the evaluation code.
#[allow(clippy::module_inception)]
mod eval;
mod state;

use self::ctx::CompilationCtx;
use crate::comp::Node;
use crate::comp::NodeId;
use crate::compile::{
    CompiledGraph, CompiledNode, CompositionNodeKind, ExtractOutput, StepResult, ValueStore,
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
            topology_fingerprint: None,
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

    /// Stamp this compiled graph with a topology fingerprint.
    ///
    /// Once set, [`snapshot`](Self::snapshot) embeds the fingerprint in
    /// the snapshot metadata and [`restore`](Self::restore) refuses to
    /// load any snapshot whose fingerprint differs. The recommended
    /// source of the value is
    /// [`TFlowBuilder::fingerprint`](crate::builder::TFlowBuilder::fingerprint).
    #[must_use]
    pub const fn with_topology_fingerprint(mut self, fingerprint: [u8; 32]) -> Self {
        self.topology_fingerprint = Some(fingerprint);
        self
    }
}
