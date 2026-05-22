//! Operator and custom-node plugin traits for the tflo engine.
//!
//! [`Operator`] is the unified extension point for runtime computation nodes
//! contributed by external crates. [`CustomNode`] is the legacy extension
//! point kept for backward compatibility; new code should implement
//! [`Operator`] instead.
//!
//! Attach a custom node to a computation graph with
//! [`Comp::custom_node`](crate::comp::Comp::custom_node) or
//! [`Comp::custom_node1`](crate::comp::Comp::custom_node1); those builder
//! entry points are switched over to accept [`Operator`] in a later step.
//!
//! This is the mechanism the `tflo-fintech` crate uses to provide indicators
//! such as ADX, ATR, and KAMA without any finance-specific code living in
//! `tflo-core`.

use crate::compile::{Absent, Computed, NodeOutput};
use std::sync::Arc;

/// A user-defined stateful computation node.
///
/// A `CustomNode` receives its resolved input values — one [`Computed`] per
/// declared input, in the order the inputs were wired — on every record, and
/// produces one [`Computed`] output. Each input is either a finite `f64` or a
/// typed [`Absent`] reason; the output likewise carries a value or a reason.
///
/// # Example
///
/// ```
/// use tflo_core::compile::Computed;
/// use tflo_core::custom_node::{CustomNode, require};
///
/// /// A node that emits the running sum of its single input.
/// #[derive(Default)]
/// struct RunningSum {
///     total: f64,
/// }
///
/// impl CustomNode for RunningSum {
///     fn eval(&mut self, inputs: &[Computed]) -> Computed {
///         // Skip records where the input is absent — `?` propagates the
///         // reason and leaves `total` untouched.
///         let x = require(inputs, 0)?;
///         self.total += x;
///         Ok(self.total)
///     }
///
///     fn reset(&mut self) {
///         self.total = 0.0;
///     }
///
///     fn name(&self) -> &str {
///         "running_sum"
///     }
/// }
/// ```
pub trait CustomNode: Send + Sync + 'static {
    /// Evaluate the node against the current record's resolved inputs.
    ///
    /// `inputs` holds one [`Computed`] per wired input, in declaration order.
    /// Read inputs with [`require`] so a missing or absent input
    /// `?`-propagates as a typed reason. Return `Err(`[`Absent::WarmingUp`]`)`
    /// while the node is still warming up.
    fn eval(&mut self, inputs: &[Computed]) -> Computed;

    /// Reset internal state to the freshly-constructed condition.
    ///
    /// The default implementation does nothing.
    fn reset(&mut self) {}

    /// Human-readable name, used in graph-plan and debug output.
    ///
    /// The default is `"custom"`.
    fn name(&self) -> &str {
        "custom"
    }

    /// Serialize this node's state for checkpointing.
    ///
    /// The default returns `None` — "this node is not checkpointable" — which
    /// makes [`CompiledGraph::snapshot`](crate::compile::CompiledGraph::snapshot)
    /// reject any graph containing the node. Override this together with
    /// [`load`](CustomNode::load) to make the node checkpointable. The bytes
    /// are opaque to `tflo` and are round-tripped to [`load`](CustomNode::load)
    /// verbatim.
    fn save(&self) -> Option<Vec<u8>> {
        None
    }

    /// Restore this node's state from bytes produced by [`save`](CustomNode::save).
    ///
    /// # Errors
    ///
    /// Returns [`CustomNodeLoadError`] if the bytes cannot be decoded. The
    /// default implementation always errors, matching the non-checkpointable
    /// default [`save`](CustomNode::save).
    fn load(&mut self, _bytes: &[u8]) -> Result<(), CustomNodeLoadError> {
        Err(CustomNodeLoadError::new(
            "this custom node does not support checkpoint restore",
        ))
    }
}

/// Error returned by [`CustomNode::load`] when checkpoint bytes cannot be
/// applied to a node.
#[derive(Debug, Clone)]
pub struct CustomNodeLoadError {
    /// Human-readable reason the load failed.
    pub reason: String,
}

impl CustomNodeLoadError {
    /// Construct a load error with the given reason.
    #[must_use]
    pub fn new(reason: impl Into<String>) -> Self {
        Self {
            reason: reason.into(),
        }
    }
}

impl std::fmt::Display for CustomNodeLoadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "custom node load failed: {}", self.reason)
    }
}

impl std::error::Error for CustomNodeLoadError {}

/// Read input `idx` from a [`CustomNode::eval`] input slice.
///
/// An out-of-range index is reported as `Err(`[`Absent::WarmingUp`]`)`.
/// Combine with `?` inside `eval` to short-circuit on any absent input before
/// touching node state.
pub fn require(inputs: &[Computed], idx: usize) -> Computed {
    inputs.get(idx).copied().unwrap_or(Err(Absent::WarmingUp))
}

/// A boxed [`CustomNode`] — the live, stateful instance held by a compiled graph.
pub type BoxedCustomNode = Box<dyn CustomNode>;

/// A factory that produces fresh [`CustomNode`] instances.
///
/// The graph description stores a factory rather than a node instance so that
/// the description stays cheaply cloneable and every compiled graph (including
/// each per-key graph in keyed execution) receives its own independent state.
pub type CustomNodeFactory = Arc<dyn Fn() -> BoxedCustomNode + Send + Sync>;

// ---------------------------------------------------------------------------
// Operator — unified plugin trait
// ---------------------------------------------------------------------------

/// A node kind contributed to the engine — the single plugin mechanism.
///
/// `tflo-core` defines only sources and closure transforms natively; every
/// other node kind (the `tflo-ops` catalog, `tflo-fintech` indicators, user
/// plugins) reaches the engine as an `Operator`.
///
/// Unlike the legacy [`CustomNode`] trait, `Operator` receives the record
/// timestamp (`ts`) on every call and returns a [`NodeOutput`] that can carry
/// any `'static` typed value, not just a `f64`.
pub trait Operator: Send + Sync + 'static {
    /// Evaluate against this record's resolved inputs and timestamp.
    ///
    /// `inputs` holds one [`Computed`] per wired input, in declaration order.
    /// `ts` is the record timestamp (needed by time-windowed operators).
    fn eval(&mut self, inputs: &[Computed], ts: i64) -> NodeOutput;

    /// Reset to the freshly-constructed state. Default: no-op.
    fn reset(&mut self) {}

    /// Human-readable name for graph-plan/debug output. Default: `"operator"`.
    fn name(&self) -> &str {
        "operator"
    }

    /// Serialize state for checkpointing. Default `None` = not checkpointable.
    fn save(&self) -> Option<Vec<u8>> {
        None
    }

    /// Restore state from `save()` bytes. Default errors.
    fn load(&mut self, _bytes: &[u8]) -> Result<(), OperatorLoadError> {
        Err(OperatorLoadError::new(
            "operator does not support checkpoint restore",
        ))
    }
}

/// Error returned by [`Operator::load`] when checkpoint bytes cannot be
/// applied to an operator.
#[derive(Debug, Clone)]
pub struct OperatorLoadError {
    /// Human-readable reason the load failed.
    pub reason: String,
}

impl OperatorLoadError {
    /// Construct a load error with the given reason.
    #[must_use]
    pub fn new(reason: impl Into<String>) -> Self {
        Self {
            reason: reason.into(),
        }
    }
}

impl std::fmt::Display for OperatorLoadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "operator load failed: {}", self.reason)
    }
}

impl std::error::Error for OperatorLoadError {}

/// Boxed live operator instance held by a compiled graph.
pub type BoxedOperator = Box<dyn Operator>;

/// Factory producing fresh [`Operator`] instances (one per compiled graph).
pub type OperatorFactory = std::sync::Arc<dyn Fn() -> BoxedOperator + Send + Sync>;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compile::NodeOutput;

    #[test]
    fn operator_emits_typed_output() {
        struct Tagger;
        impl Operator for Tagger {
            fn eval(&mut self, inputs: &[Computed], _ts: i64) -> NodeOutput {
                NodeOutput::other(require(inputs, 0).is_ok())
            }
        }
        let mut op = Tagger;
        let out = op.eval(&[Ok(1.0)], 0);
        assert_eq!(out.as_any().downcast_ref::<bool>(), Some(&true));
    }
}
