use crate::comp::NodeId;
use crate::compile::CompositionNodeKind;
use crate::compile::NodeOp;
use crate::compile::NodeState;

/// Entry for composition nodes with their output ID.
pub struct CompositionNodeEntry {
    pub(crate) id: NodeId,
    pub(crate) kind: CompositionNodeKind,
}

/// A compiled node with its operation and state.
pub struct CompiledNode<R> {
    pub(crate) id: NodeId,
    pub(crate) op: NodeOp<R>,
    pub(crate) state: NodeState,
}

impl<R> CompiledNode<R> {
    /// Offset all input IDs by a given amount.
    //
    // SAFETY (for every `id.0 + offset` site below): `offset` is
    // `self.max_node_id() + 1` in the only call site (`zip`), and both
    // operands are `usize` indices into a graph whose node count is
    // bounded by available memory. A sum of two graph-bounded indices
    // cannot overflow `usize` on any realizable target.
    #[allow(clippy::arithmetic_side_effects)]
    pub(crate) fn offset_input_ids(&mut self, offset: usize) {
        match &mut self.op {
            NodeOp::Prop(_) | NodeOp::Const(_) => {}
            NodeOp::MapF64(id, _)
            | NodeOp::FilterF64(id, _)
            | NodeOp::FilterMapF64(id, _)
            | NodeOp::ScanF64(id, _, _, _) => {
                *id = NodeId(id.0 + offset);
            }
            NodeOp::Map2F64(a, b, _) => {
                *a = NodeId(a.0 + offset);
                *b = NodeId(b.0 + offset);
            }
            NodeOp::Scan2F64(a, b, _, _, _) => {
                *a = NodeId(a.0 + offset);
                *b = NodeId(b.0 + offset);
            }
            NodeOp::Plugin { inputs } => {
                for id in inputs.iter_mut() {
                    *id = NodeId(id.0 + offset);
                }
            }
        }
    }
}

/// Helper function to offset node IDs.
// SAFETY: same as `offset_input_ids` — `offset` and `id.0` are both
// graph-bounded `usize` indices that cannot overflow when summed.
#[allow(clippy::arithmetic_side_effects)]
pub fn offset_node_ids(ids: &[NodeId], offset: usize) -> Vec<NodeId> {
    ids.iter().map(|id| NodeId(id.0 + offset)).collect()
}
