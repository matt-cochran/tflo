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
    /// Fire every event-time timer in `timer_service` whose `fire_ts` is at
    /// or below `watermark`, in `(fire_ts, registration_seq)` order. Each
    /// fire calls the registering node's `on_timer`, writes the resulting
    /// output to the value store at that node's id, and emits a tuple
    /// `(fire_ts, value)` for the caller to wrap into a
    /// [`PipelineItem`](crate::pipeline::PipelineItem).
    ///
    /// This is called by the keyed executor before the record step (to
    /// flush timers due strictly *before* the new record), and on idle
    /// watermark advances ([`KeyedGraphState::advance_event_time_watermark`](crate::keyed::KeyedGraphState::advance_event_time_watermark)).
    ///
    /// Only plugin nodes (those with a `NodeState::Plugin`) can have
    /// registered timers; non-plugin nodes silently no-op if a registration
    /// somehow targets them.
    pub(crate) fn fire_due_timers(
        &mut self,
        timer_service: &mut crate::timer::TimerService,
        watermark: i64,
    ) -> Vec<(i64, O)>
    where
        O: ExtractOutput,
    {
        let mut emitted: Vec<(i64, O)> = Vec::new();
        // Drain due timers first to avoid holding a `&mut timer_service`
        // across node-state borrows.
        let mut due: Vec<crate::timer::TimerEntry> = Vec::new();
        while let Some(entry) = timer_service.pop_due(watermark) {
            due.push(entry);
        }
        for entry in due {
            let Some(node_idx) = self.nodes.iter().position(|n| n.id == entry.node_id) else {
                continue;
            };
            // Split-borrow: nodes vs store. Both come from self.
            let (nodes, store) = (&mut self.nodes, &mut self.store);
            let Some(node) = nodes.get_mut(node_idx) else {
                continue;
            };
            let output_opt = match &mut node.state {
                crate::compile::NodeState::Plugin(op) => {
                    let mut ctx = crate::timer::TimerCtx {
                        service: timer_service,
                        current_node_id: entry.node_id,
                        current_ts: entry.fire_ts,
                    };
                    Some(op.on_timer(entry.fire_ts, &mut ctx))
                }
                crate::compile::NodeState::Stateless
                | crate::compile::NodeState::ScanState(_)
                | crate::compile::NodeState::Scan2State(_) => None,
            };
            if let Some(output) = output_opt {
                store.store_value(node.id, output);
                if let Some(value) = O::extract(store, &self.output_ids) {
                    emitted.push((entry.fire_ts, value));
                }
            }
        }
        emitted
    }

    /// Execute one step against `record` with the per-key timer service
    /// available to operators via [`TimerCtx`](crate::timer::TimerCtx).
    ///
    /// Same semantics as [`step_with_context`](Self::step_with_context),
    /// but plugin nodes are evaluated with their `eval_with_ctx` method so
    /// they can register/delete event-time timers. Non-keyed graphs that
    /// have no use for timers can keep calling `step_with_context`.
    pub(crate) fn step_with_context_and_timers(
        &mut self,
        record: &R,
        ts: i64,
        ctx: C,
        timer_service: &mut crate::timer::TimerService,
    ) -> StepResult<C, O>
    where
        O: ExtractOutput,
    {
        // Saturating: see step_with_context for rationale.
        self.records_seen = self.records_seen.saturating_add(1);
        if self.records_seen < self.min_warmup {
            return StepResult::WarmingUp {
                remaining: self.min_warmup.saturating_sub(self.records_seen),
                reason: crate::compile::Absent::WarmingUp,
            };
        }
        self.store.clear();

        for node in &mut self.nodes {
            let value = match &mut node.state {
                crate::compile::NodeState::Plugin(op) => {
                    // Build inputs list, then dispatch with TimerCtx.
                    let inputs_ids = match &node.op {
                        crate::compile::NodeOp::Plugin { inputs } => inputs.clone(),
                        // Non-`Plugin` ops paired with a `Plugin` state are
                        // structurally impossible (the builder pairs them),
                        // but we don't `unreachable!()` because the lint
                        // family forbids it. An empty input list collapses
                        // gracefully into the operator's WarmingUp path.
                        crate::compile::NodeOp::Prop(_)
                        | crate::compile::NodeOp::Const(_)
                        | crate::compile::NodeOp::MapF64(..)
                        | crate::compile::NodeOp::Map2F64(..)
                        | crate::compile::NodeOp::FilterF64(..)
                        | crate::compile::NodeOp::FilterMapF64(..)
                        | crate::compile::NodeOp::ScanF64(..)
                        | crate::compile::NodeOp::Scan2F64(..) => Vec::new(),
                    };
                    let values: Vec<crate::compile::Computed> = inputs_ids
                        .iter()
                        .map(|id| Self::get_computed(&self.store, id))
                        .collect();
                    let mut ctx_inner = crate::timer::TimerCtx {
                        service: timer_service,
                        current_node_id: node.id,
                        current_ts: ts,
                    };
                    op.eval_with_ctx(&values, ts, &mut ctx_inner)
                }
                crate::compile::NodeState::Stateless
                | crate::compile::NodeState::ScanState(_)
                | crate::compile::NodeState::Scan2State(_) => {
                    Self::eval_node(node, record, ts, &self.store)
                }
            };
            self.store.store_value(node.id, value);
        }

        for entry in &self.composition_nodes {
            let value = match &entry.kind {
                CompositionNodeKind::Map { mapper } => mapper(&self.store),
                CompositionNodeKind::Fold { state, folder } => folder(&self.store, state),
            };
            if let Some(v) = value {
                self.store.store_value(entry.id, v);
            }
        }

        match O::extract(&self.store, &self.output_ids) {
            Some(value) => StepResult::Ready(PipelineItem { ctx, value }),
            None => StepResult::WarmingUp {
                remaining: self.min_warmup.saturating_sub(self.records_seen),
                reason: crate::compile::Absent::WarmingUp,
            },
        }
    }

    /// Execute one step with an explicitly-supplied context value. Used
    /// by keyed execution (where the context carries the key in addition
    /// to the timestamp) and by callers that need to inject a custom
    /// context into the pipeline.
    ///
    /// This entry point does **not** route through the per-key event-time
    /// timer service; the keyed-execution path uses an internal variant
    /// (with a [`TimerCtx`](crate::timer::TimerCtx)) for graphs whose
    /// operators register timers.
    pub fn step_with_context(&mut self, record: &R, ts: i64, ctx: C) -> StepResult<C, O>
    where
        O: ExtractOutput,
    {
        // Saturating: a stream long enough to push `records_seen` past
        // `usize::MAX` would have run for geological time on any 64-bit
        // host; pinning at the ceiling keeps `is_warmed_up()` correct
        // (still warm) without panicking.
        self.records_seen = self.records_seen.saturating_add(1);

        // Check warmup status
        if self.records_seen < self.min_warmup {
            return StepResult::WarmingUp {
                // Saturating: `records_seen < min_warmup` is checked
                // above, so the subtraction is always nonneg in
                // practice — keep `saturating_sub` for belt-and-braces
                // (and to satisfy the lint without an `#[allow]`).
                remaining: self.min_warmup.saturating_sub(self.records_seen),
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
        // Saturating — see `step_with_context` for rationale.
        self.records_seen = self.records_seen.saturating_add(1);

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
