//! Custom graph nodes contributed by external crates.
//!
//! [`CustomNode`] is the extension point for runtime computation nodes that
//! `tflo-core` does not provide built in. An external crate implements this
//! trait and attaches instances to a computation graph with
//! [`Comp::custom_node`](crate::comp::Comp::custom_node) or
//! [`Comp::custom_node1`](crate::comp::Comp::custom_node1).
//!
//! This is the mechanism the `tflo-fintech` crate uses to provide indicators
//! such as ADX, ATR, and KAMA without any finance-specific code living in
//! `tflo-core`.

use std::sync::Arc;

/// A user-defined stateful computation node.
///
/// A `CustomNode` receives its resolved input values — one `f64` per declared
/// input, in the order the inputs were wired — on every record, and produces
/// one `f64` output. Returning [`f64::NAN`] signals "warming up / no value
/// yet", matching the convention of every built-in node.
///
/// # Example
///
/// ```
/// use tflo_core::custom_node::CustomNode;
///
/// /// A node that emits the running sum of its single input.
/// #[derive(Default)]
/// struct RunningSum {
///     total: f64,
/// }
///
/// impl CustomNode for RunningSum {
///     fn eval(&mut self, inputs: &[f64]) -> f64 {
///         self.total += inputs.first().copied().unwrap_or(0.0);
///         self.total
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
    /// `inputs` holds one value per wired input, in declaration order. A
    /// defensive implementation should read inputs with
    /// [`slice::get`] rather than indexing. Return [`f64::NAN`] while the
    /// node is still warming up.
    fn eval(&mut self, inputs: &[f64]) -> f64;

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
}

/// A boxed [`CustomNode`] — the live, stateful instance held by a compiled graph.
pub type BoxedCustomNode = Box<dyn CustomNode>;

/// A factory that produces fresh [`CustomNode`] instances.
///
/// The graph description stores a factory rather than a node instance so that
/// the description stays cheaply cloneable and every compiled graph (including
/// each per-key graph in keyed execution) receives its own independent state.
pub type CustomNodeFactory = Arc<dyn Fn() -> BoxedCustomNode + Send + Sync>;
