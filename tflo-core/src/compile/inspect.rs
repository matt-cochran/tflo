use crate::comp::NodeId;
use crate::compile::{
    CompiledGraph, CompiledNode, CompositionNodeEntry, CompositionNodeKind, Computed, NodeOutput,
    NodeState, PipelinedGraph, ValueStore,
};
use crate::pipeline::PipelineContext;

/// Graph plan for introspection and debugging.
#[derive(Debug, Clone)]
pub struct GraphPlan {
    /// Total number of nodes (base + composition).
    pub node_count: usize,
    /// Number of base computation nodes.
    pub base_node_count: usize,
    /// Number of composition nodes (map, fold, etc.).
    pub composition_node_count: usize,
    /// Number of output nodes.
    pub output_count: usize,
    /// Number of records processed so far.
    pub records_seen: usize,
    /// Minimum warmup requirement.
    pub min_warmup: usize,
    /// Remaining records needed for warmup.
    pub warmup_remaining: usize,
    /// Type name of the context.
    pub context_type: String,
}

/// Runtime state summary for observability.
#[derive(Debug, Clone)]
pub struct GraphStateSummary {
    /// Number of records processed.
    pub records_seen: usize,
    /// Minimum warmup requirement.
    pub min_warmup: usize,
    /// Remaining records needed for warmup.
    pub warmup_remaining: usize,
    /// Whether the graph is fully warmed up.
    pub is_warmed_up: bool,
    /// Total number of nodes.
    pub node_count: usize,
    /// Number of outputs.
    pub output_count: usize,
}

impl std::fmt::Debug for ValueStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ValueStore")
            .field("value_count", &self.values.len())
            .finish()
    }
}

impl ValueStore {
    /// Create a new empty value store.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Store a computed [`NodeOutput`] for a node.
    pub(crate) fn store_value(&mut self, id: NodeId, value: NodeOutput) {
        let _ = self.values.insert(id, value);
    }

    /// Get a reference to a stored value, downcast to `T`.
    #[must_use]
    pub fn get<T: 'static>(&self, id: &NodeId) -> Option<&T> {
        self.values.get(id)?.as_any().downcast_ref()
    }

    /// Get a cloned copy of a stored value, downcast to `T`.
    #[must_use]
    pub fn get_cloned<T: Clone + 'static>(&self, id: &NodeId) -> Option<T> {
        self.get::<T>(id).cloned()
    }

    /// Get the typed [`Computed`] a node produced, if it has been evaluated.
    ///
    /// This is the absent-aware accessor: `Ok` for a present value, `Err` for
    /// a typed [`Absent`](super::Absent) reason, `None` if the node has not
    /// been evaluated this step.
    #[must_use]
    pub fn get_computed(&self, id: &NodeId) -> Option<Computed> {
        match self.values.get(id)? {
            NodeOutput::Computed(c) => Some(*c),
            NodeOutput::Other(b) => b.downcast_ref::<f64>().copied().map(Ok),
        }
    }

    /// Get a stored `f64` value via the fast path.
    ///
    /// Returns the value only when the node produced a present `Ok`; an absent
    /// node yields `None`. For absent-aware access use [`get_computed`](Self::get_computed).
    #[must_use]
    pub fn get_f64(&self, id: &NodeId) -> Option<f64> {
        match self.values.get(id)? {
            NodeOutput::Computed(c) => c.ok(),
            NodeOutput::Other(b) => b.downcast_ref::<f64>().copied(),
        }
    }

    /// Clear all stored values.
    pub fn clear(&mut self) {
        self.values.clear();
    }
}

impl std::fmt::Debug for CompositionNodeEntry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CompositionNodeEntry")
            .field("id", &self.id)
            .finish()
    }
}

impl std::fmt::Debug for CompositionNodeKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Map { .. } => write!(f, "Map"),
            Self::Fold { .. } => write!(f, "Fold"),
        }
    }
}

impl<R, O, C: PipelineContext> std::fmt::Debug for CompiledGraph<R, O, C> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CompiledGraph")
            .field("node_count", &self.nodes.len())
            .field("composition_count", &self.composition_nodes.len())
            .field("output_ids", &self.output_ids)
            .field("records_seen", &self.records_seen)
            .field("context_type", &std::any::type_name::<C>())
            .finish()
    }
}

impl std::fmt::Debug for NodeState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Stateless => write!(f, "Stateless"),
            Self::ScanState(_) => write!(f, "ScanState"),
            Self::Scan2State(_) => write!(f, "Scan2State"),
            Self::Plugin(op) => write!(f, "Plugin({})", op.name()),
        }
    }
}

impl<R> std::fmt::Debug for CompiledNode<R> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CompiledNode")
            .field("id", &self.id)
            .field("state", &self.state)
            .finish()
    }
}

impl<R, O1, O2, C> std::fmt::Debug for PipelinedGraph<R, O1, O2, C>
where
    C: PipelineContext,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PipelinedGraph")
            .field("first_nodes", &self.first.nodes.len())
            .field("second_nodes", &self.second.nodes.len())
            .finish()
    }
}
