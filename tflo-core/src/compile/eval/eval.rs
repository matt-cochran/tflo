//! Node evaluation dispatch.
//!
//! [`eval_node`] is the top-level match that delegates each [`NodeOp`] variant
//! to a dedicated per-variant helper.
//!
//! Every `f64`-producing node threads a [`Computed`] — a finite value or a
//! typed [`Absent`] reason. Total math `.map`s the reason through; partial math
//! (`sqrt`/`ln`/`div`) turns a bad argument into a typed `Err`; binary ops
//! propagate the first absent input; stateful nodes skip their state update on
//! an absent input rather than advancing with a substitute value.

use std::any::Any;
use std::sync::Arc;

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
    /// Dispatches to a per-variant helper based on the node's operation.
    pub(super) fn eval_node(
        node: &mut CompiledNode<R>,
        record: &R,
        ts: i64,
        store: &ValueStore,
    ) -> NodeOutput {
        // Split the borrow: arms that don't touch `node.state` keep `&node.op`
        // and call stateless helpers; stateful arms clone the cheap `Arc`s
        // out of `op` first, then pass `&mut node.state` to their helper.
        match &node.op {
            NodeOp::Prop(f) => NodeOutput::from(f(record)),
            NodeOp::Const(v) => NodeOutput::from(*v),
            NodeOp::MapF64(input, f) => Self::eval_map(store, input, f),
            NodeOp::Map2F64(a, b, f) => Self::eval_map2(store, a, b, f),
            NodeOp::FilterF64(input, f) => Self::eval_filter(store, input, f),
            NodeOp::FilterMapF64(input, f) => Self::eval_filter_map(store, input, f),
            NodeOp::ScanF64(input, state_factory, step) => {
                let input = *input;
                let factory = Arc::clone(state_factory);
                let step = Arc::clone(step);
                Self::eval_scan(&mut node.state, store, &input, &factory, &step)
            }
            NodeOp::Scan2F64(a, b, state_factory, step) => {
                let a = *a;
                let b = *b;
                let factory = Arc::clone(state_factory);
                let step = Arc::clone(step);
                Self::eval_scan2(&mut node.state, store, &a, &b, &factory, &step)
            }
            NodeOp::Plugin { inputs } => {
                let inputs = inputs.clone();
                Self::eval_plugin(&mut node.state, store, &inputs, ts)
            }
        }
    }

    fn eval_map(
        store: &ValueStore,
        input: &NodeId,
        f: &Arc<dyn Fn(f64) -> f64 + Send + Sync>,
    ) -> NodeOutput {
        NodeOutput::from(Self::get_computed(store, input).map(|x| f(x)))
    }

    fn eval_map2(
        store: &ValueStore,
        a: &NodeId,
        b: &NodeId,
        f: &Arc<dyn Fn(f64, f64) -> f64 + Send + Sync>,
    ) -> NodeOutput {
        let va = Self::get_computed(store, a);
        let vb = Self::get_computed(store, b);
        NodeOutput::from(match (va, vb) {
            (Err(e), _) | (Ok(_), Err(e)) => Err(e),
            (Ok(x), Ok(y)) => Ok(f(x, y)),
        })
    }

    fn eval_filter(
        store: &ValueStore,
        input: &NodeId,
        f: &Arc<dyn Fn(f64) -> bool + Send + Sync>,
    ) -> NodeOutput {
        NodeOutput::from(Self::get_computed(store, input).and_then(|x| {
            if f(x) {
                Ok(x)
            } else {
                Err(Absent::FilteredOut)
            }
        }))
    }

    fn eval_filter_map(
        store: &ValueStore,
        input: &NodeId,
        f: &Arc<dyn Fn(f64) -> Option<f64> + Send + Sync>,
    ) -> NodeOutput {
        NodeOutput::from(
            Self::get_computed(store, input).and_then(|x| f(x).ok_or(Absent::FilteredOut)),
        )
    }

    fn eval_scan(
        state_slot: &mut NodeState,
        store: &ValueStore,
        input: &NodeId,
        state_factory: &Arc<dyn Fn() -> Box<dyn Any + Send + Sync> + Send + Sync>,
        step: &Arc<dyn Fn(&mut Box<dyn Any + Send + Sync>, f64) -> Computed + Send + Sync>,
    ) -> NodeOutput {
        // A scan does not advance its state on an absent input — it
        // propagates the reason and leaves the accumulator untouched.
        NodeOutput::from(match Self::get_computed(store, input) {
            Err(e) => Err(e),
            Ok(v) => match state_slot {
                NodeState::ScanState(state) => step(state, v),
                _ => {
                    let mut new_state = state_factory();
                    let result = step(&mut new_state, v);
                    *state_slot = NodeState::ScanState(new_state);
                    result
                }
            },
        })
    }

    fn eval_scan2(
        state_slot: &mut NodeState,
        store: &ValueStore,
        a: &NodeId,
        b: &NodeId,
        state_factory: &Arc<dyn Fn() -> Box<dyn Any + Send + Sync> + Send + Sync>,
        step: &Arc<dyn Fn(&mut Box<dyn Any + Send + Sync>, f64, f64) -> Computed + Send + Sync>,
    ) -> NodeOutput {
        NodeOutput::from(
            match (Self::get_computed(store, a), Self::get_computed(store, b)) {
                (Err(e), _) | (Ok(_), Err(e)) => Err(e),
                (Ok(va), Ok(vb)) => match state_slot {
                    NodeState::Scan2State(state) => step(state, va, vb),
                    _ => {
                        let mut new_state = state_factory();
                        let result = step(&mut new_state, va, vb);
                        *state_slot = NodeState::Scan2State(new_state);
                        result
                    }
                },
            },
        )
    }

    fn eval_plugin(
        state_slot: &mut NodeState,
        store: &ValueStore,
        inputs: &[NodeId],
        ts: i64,
    ) -> NodeOutput {
        let values: Vec<Computed> = inputs
            .iter()
            .map(|id| Self::get_computed(store, id))
            .collect();
        match state_slot {
            NodeState::Plugin(op) => op.eval(&values, ts),
            _ => NodeOutput::computed(Err(Absent::WarmingUp)),
        }
    }
}
