//! Compilation context for building [`CompiledNode`]s from [`Node`] definitions.
//!
//! [`CompilationCtx`] holds any dependencies needed during graph compilation.
//! Currently it is a marker, but future options (validation hooks, feature flags,
//! custom type resolvers, …) can be added here without changing the signature of
//! `compile_node`.

use crate::comp::Node;
use crate::comp::NodeId;
use crate::compile::{CompiledNode, NodeOp, NodeState};

/// Context object that carries dependencies for graph compilation.
///
/// Pass this as the first argument to [`compile_node`][Self::compile_node].
/// The struct is intentionally empty for now; new fields can be added here
/// without modifying the `compile_node` signature.
pub struct CompilationCtx<R> {
    _marker: std::marker::PhantomData<R>,
}

impl<R> CompilationCtx<R> {
    /// Create a new compilation context.
    pub fn new() -> Self {
        Self {
            _marker: std::marker::PhantomData,
        }
    }

    /// Compile a single [`Node`] into a [`CompiledNode`].
    ///
    /// This method maps each node variant to its corresponding operation
    /// and initialises the appropriate state tracker.
    pub fn compile_node(&self, id: NodeId, node: Node<R>) -> CompiledNode<R> {
        match node {
            Node::Prop(f) => CompiledNode {
                id,
                op: NodeOp::Prop(f),
                state: NodeState::Stateless,
            },
            Node::Const(v) => CompiledNode {
                id,
                op: NodeOp::Const(v),
                state: NodeState::Stateless,
            },
            // Custom functional operators
            Node::MapF64 { input, f, .. } => CompiledNode {
                id,
                op: NodeOp::MapF64(input, f),
                state: NodeState::Stateless,
            },
            Node::Map2F64 { a, b, f, .. } => CompiledNode {
                id,
                op: NodeOp::Map2F64(a, b, f),
                state: NodeState::Stateless,
            },
            Node::FilterF64 {
                input, predicate, ..
            } => CompiledNode {
                id,
                op: NodeOp::FilterF64(input, predicate),
                state: NodeState::Stateless,
            },
            Node::FilterMapF64 { input, f, .. } => CompiledNode {
                id,
                op: NodeOp::FilterMapF64(input, f),
                state: NodeState::Stateless,
            },
            Node::ScanF64 {
                input, ctor, step, ..
            } => {
                let initial_state = ctor();
                CompiledNode {
                    id,
                    op: NodeOp::ScanF64(input, ctor, step),
                    state: NodeState::ScanState(initial_state),
                }
            }
            Node::Scan2F64 {
                a, b, ctor, step, ..
            } => {
                let initial_state = ctor();
                CompiledNode {
                    id,
                    op: NodeOp::Scan2F64(a, b, ctor, step),
                    state: NodeState::Scan2State(initial_state),
                }
            }
            // Plugin node: `factory()` builds a fresh instance so each
            // compiled graph (including per-key graphs) gets independent state.
            Node::Plugin { inputs, factory } => CompiledNode {
                id,
                op: NodeOp::Plugin { inputs },
                state: NodeState::Plugin(factory()),
            },
        }
    }
}

impl<R> Default for CompilationCtx<R> {
    fn default() -> Self {
        Self::new()
    }
}
