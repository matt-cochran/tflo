//! Operator plugin trait for the tflo engine.
//!
//! [`Operator`] is the unified extension point for runtime computation nodes
//! contributed by external crates. All node kinds other than built-in sources
//! and closure transforms reach the engine as an `Operator`.
//!
//! Attach an operator to a computation graph with
//! [`Comp::custom_node`](crate::comp::Comp::custom_node) or
//! [`Comp::custom_node1`](crate::comp::Comp::custom_node1).
//!
//! This is the mechanism the `tflo-fintech` crate uses to provide indicators
//! such as ADX, ATR, and KAMA without any finance-specific code living in
//! `tflo-core`.

use crate::compile::{Absent, Computed, NodeOutput};

/// Read input `idx` from an [`Operator::eval`] input slice.
///
/// An out-of-range index is reported as `Err(`[`Absent::WarmingUp`]`)`.
pub fn require(inputs: &[Computed], idx: usize) -> Computed {
    inputs.get(idx).copied().unwrap_or(Err(Absent::WarmingUp))
}

/// A node kind contributed to the engine — the single plugin mechanism.
///
/// `tflo-core` defines only sources and closure transforms natively; every
/// other node kind (the `tflo-ops` catalog, `tflo-fintech` indicators, user
/// plugins) reaches the engine as an `Operator`.
///
/// `Operator` receives the record timestamp (`ts`) on every call and returns
/// a [`NodeOutput`] that can carry any `'static` typed value, not just a `f64`.
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

/// Common interface over the windowing primitives (`TimeWindow`, `CountWindow`, …).
///
/// Lets the generic `Windowed` operator shape (in `tflo-ops`) treat
/// time-based and count-based windows uniformly.
pub trait WindowPrimitive {
    /// Admit a value; `ts` is ignored by count-based windows.
    fn push(&mut self, ts: i64, value: f64);
    /// Number of observations the window currently holds toward its
    /// reduction. For change-based windows (e.g. RSI) this counts changes —
    /// one fewer than the values pushed — so `is_empty` stays `true` until
    /// the window can actually produce a result.
    fn len(&self) -> usize;
    /// True when the window holds no observations yet.
    fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// Common interface over the two-input windowing primitives (correlation
/// windows). Lets the generic `BivariateWindowed` operator shape treat
/// time- and count-based correlation windows uniformly.
pub trait BivariateWindow {
    /// Admit a value pair; `ts` is ignored by count-based windows.
    fn push(&mut self, ts: i64, a: f64, b: f64);
    /// Number of value pairs currently retained.
    fn len(&self) -> usize;
    /// True when the window holds no pairs yet.
    fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

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
