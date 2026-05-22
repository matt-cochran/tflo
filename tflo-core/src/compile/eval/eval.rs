//! Node evaluation dispatch.
//!
//! [`eval_node`] is the top-level match that delegates each [`NodeOp`] variant
//! to the appropriate computation.
//!
//! Every `f64`-producing node threads a [`Computed`] — a finite value or a
//! typed [`Absent`] reason. Total math `.map`s the reason through; partial math
//! (`sqrt`/`ln`/`div`) turns a bad argument into a typed `Err`; binary ops
//! propagate the first absent input; stateful nodes skip their state update on
//! an absent input rather than advancing with a substitute value.

use crate::comp::NodeId;
use crate::compile::{
    Absent, CompiledGraph, CompiledNode, Computed, NodeOp, NodeOutput, NodeState, ValueStore,
};
use crate::pipeline::PipelineContext;

impl<R, O, C: PipelineContext> CompiledGraph<R, O, C> {
    /// Read a node's [`Computed`] output from the store.
    ///
    /// A node with no stored value (an input not evaluated this step) is
    /// treated as still warming up.
    #[inline]
    pub(super) fn get_computed(store: &ValueStore, id: &NodeId) -> Computed {
        store.get_computed(id).unwrap_or(Err(Absent::WarmingUp))
    }

    /// Evaluate a single compiled node against the current record.
    ///
    /// Dispatches to the appropriate stateless computation or stateful helper
    /// based on the node's operation variant.
    pub(super) fn eval_node(
        node: &mut CompiledNode<R>,
        record: &R,
        ts: i64,
        store: &ValueStore,
    ) -> NodeOutput {
        match &node.op {
            // ---- Sources ----
            NodeOp::Prop(f) => NodeOutput::from(f(record)),
            NodeOp::Const(v) => NodeOutput::from(*v),

            // ---- Custom functional operators ----
            NodeOp::MapF64(input, f) => {
                NodeOutput::from(Self::get_computed(store, input).map(|x| f(x)))
            }
            NodeOp::Map2F64(a, b, f) => {
                let va = Self::get_computed(store, a);
                let vb = Self::get_computed(store, b);
                NodeOutput::from(match (va, vb) {
                    (Err(e), _) | (Ok(_), Err(e)) => Err(e),
                    (Ok(x), Ok(y)) => Ok(f(x, y)),
                })
            }
            NodeOp::FilterF64(input, f) => {
                NodeOutput::from(Self::get_computed(store, input).and_then(|x| {
                    if f(x) {
                        Ok(x)
                    } else {
                        Err(Absent::FilteredOut)
                    }
                }))
            }
            NodeOp::FilterMapF64(input, f) => NodeOutput::from(
                Self::get_computed(store, input).and_then(|x| f(x).ok_or(Absent::FilteredOut)),
            ),
            NodeOp::ScanF64(input, state_factory, step) => {
                // A scan does not advance its state on an absent input — it
                // propagates the reason and leaves the accumulator untouched.
                NodeOutput::from(match Self::get_computed(store, input) {
                    Err(e) => Err(e),
                    Ok(v) => match &mut node.state {
                        NodeState::ScanState(state) => step(state, v),
                        _ => {
                            let mut new_state = state_factory();
                            let result = step(&mut new_state, v);
                            node.state = NodeState::ScanState(new_state);
                            result
                        }
                    },
                })
            }
            NodeOp::Scan2F64(a, b, state_factory, step) => NodeOutput::from(
                match (Self::get_computed(store, a), Self::get_computed(store, b)) {
                    (Err(e), _) | (Ok(_), Err(e)) => Err(e),
                    (Ok(va), Ok(vb)) => match &mut node.state {
                        NodeState::Scan2State(state) => step(state, va, vb),
                        _ => {
                            let mut new_state = state_factory();
                            let result = step(&mut new_state, va, vb);
                            node.state = NodeState::Scan2State(new_state);
                            result
                        }
                    },
                },
            ),

            // ---- Plugin nodes ----
            NodeOp::Plugin { inputs } => {
                let values: Vec<Computed> = inputs
                    .iter()
                    .map(|id| Self::get_computed(store, id))
                    .collect();
                match &mut node.state {
                    NodeState::Plugin(op) => op.eval(&values, ts),
                    _ => NodeOutput::computed(Err(Absent::WarmingUp)),
                }
            }
        }
    }
}
