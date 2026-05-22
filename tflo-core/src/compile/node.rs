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
            NodeOp::Sma(id)
            | NodeOp::Ema(id)
            | NodeOp::Std(id)
            | NodeOp::Variance(id)
            | NodeOp::Max(id)
            | NodeOp::Min(id)
            | NodeOp::Sum(id)
            | NodeOp::Count(id)
            | NodeOp::Prev(id)
            | NodeOp::Lag(id)
            | NodeOp::Delta(id)
            | NodeOp::Abs(id)
            | NodeOp::Sqrt(id)
            | NodeOp::Ln(id)
            | NodeOp::Neg(id)
            | NodeOp::Rate(id)
            | NodeOp::Velocity(id)
            | NodeOp::Acceleration(id)
            | NodeOp::Median(id)
            | NodeOp::Wma(id)
            | NodeOp::Rsi(id)
            | NodeOp::CumSum(id)
            | NodeOp::CumMax(id)
            | NodeOp::CumMin(id)
            | NodeOp::CumProd(id)
            | NodeOp::PctChange(id)
            | NodeOp::LogReturn(id)
            | NodeOp::Exp(id)
            | NodeOp::Log10(id)
            | NodeOp::Log2(id)
            | NodeOp::Floor(id)
            | NodeOp::Ceil(id)
            | NodeOp::Round(id)
            | NodeOp::Skewness(id)
            | NodeOp::Kurtosis(id)
            | NodeOp::Rank(id) => {
                *id = NodeId(id.0 + offset);
            }
            NodeOp::PrevBy(id, _) => {
                *id = NodeId(id.0 + offset);
            }
            NodeOp::Add(a, b)
            | NodeOp::Sub(a, b)
            | NodeOp::Mul(a, b)
            | NodeOp::Div(a, b)
            | NodeOp::Cross(a, b)
            | NodeOp::CrossAbove(a, b)
            | NodeOp::CrossUnder(a, b)
            | NodeOp::CrossHysteresis(a, b)
            | NodeOp::Gt(a, b)
            | NodeOp::Gte(a, b)
            | NodeOp::Lt(a, b)
            | NodeOp::Lte(a, b)
            | NodeOp::Eq(a, b)
            | NodeOp::Correlation(a, b)
            | NodeOp::Covariance(a, b) => {
                *a = NodeId(a.0 + offset);
                *b = NodeId(b.0 + offset);
            }
            NodeOp::MulConst(id, _)
            | NodeOp::AddConst(id, _)
            | NodeOp::DivConst(id, _)
            | NodeOp::Pow(id, _)
            | NodeOp::Clamp(id, _, _)
            | NodeOp::Quantile(id, _)
            | NodeOp::GlitchFilter(id)
            | NodeOp::RuntDetect(id)
            | NodeOp::PulseWidth(id)
            | NodeOp::WindowDetect(id)
            | NodeOp::MapF64(id, _)
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
            NodeOp::Custom { inputs } => {
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
