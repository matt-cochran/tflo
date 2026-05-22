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
    pub(crate) fn offset_input_ids(&mut self, offset: usize) {
        match &mut self.op {
            NodeOp::Prop(_) | NodeOp::Const(_) => {}
            NodeOp::MapF64(id, _)
            | NodeOp::FilterF64(id, _)
            | NodeOp::FilterMapF64(id, _)
            | NodeOp::ScanF64(id, _, _) => {
                *id = NodeId(id.0 + offset);
            }
            NodeOp::Map2F64(a, b, _) => {
                *a = NodeId(a.0 + offset);
                *b = NodeId(b.0 + offset);
            }
            NodeOp::Scan2F64(a, b, _, _) => {
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
pub fn offset_node_ids(ids: &[NodeId], offset: usize) -> Vec<NodeId> {
    ids.iter().map(|id| NodeId(id.0 + offset)).collect()
}
