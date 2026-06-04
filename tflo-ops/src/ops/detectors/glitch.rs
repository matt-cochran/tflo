//! Glitch-filter detector operator. Moved out of the parent module by
//! module extraction; see `mod.rs` for the public extension
//! traits ([`DetectorOps::glitch_filter`](super::DetectorOps::glitch_filter)).

use crate::checkpoint;
use crate::events::GlitchResult;
use crate::primitives::GlitchFilter;
use serde::{Deserialize, Serialize};
use tflo_core::compile::{Computed, NodeOutput};
use tflo_core::operator::{Operator, OperatorLoadError, require};

/// Glitch filter — `glitch_filter`.
///
/// Wraps [`GlitchFilter`]; an absent input emits
/// [`GlitchResult::NoTransition`] without advancing the detector state, so a
/// later present record sees the prior level rather than a `NaN`-polluted
/// internal state. The `Option<bool>` the primitive returns is mapped to a
/// [`GlitchResult`]: `Some(true)` → `ValidPulse`, `Some(false)` → `Rejected`,
/// `None` → `NoTransition`.
///
/// The [`DetectorOps::glitch_filter`](super::DetectorOps::glitch_filter)
/// builder method is the normal entry point.
#[derive(Serialize, Deserialize)]
pub(crate) struct GlitchOp {
    detector: GlitchFilter,
}

impl GlitchOp {
    pub(crate) const fn new(detector: GlitchFilter) -> Self {
        Self { detector }
    }
}

impl Operator for GlitchOp {
    fn eval(&mut self, inputs: &[Computed], ts: i64) -> NodeOutput {
        // Absent input → no event this tick. Skip the detector update entirely
        // so the next present record sees the prior level, not a NaN-polluted
        // state.
        let Ok(value) = require(inputs, 0) else {
            return NodeOutput::other(GlitchResult::NoTransition);
        };
        let result = match self.detector.update(value, ts) {
            Some(true) => GlitchResult::ValidPulse,
            Some(false) => GlitchResult::Rejected,
            None => GlitchResult::NoTransition,
        };
        NodeOutput::other(result)
    }

    fn name(&self) -> &str {
        "glitch_filter"
    }

    fn save(&self) -> Option<Vec<u8>> {
        checkpoint::save(self)
    }

    fn load(&mut self, bytes: &[u8]) -> Result<(), OperatorLoadError> {
        checkpoint::load(self, bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tflo_core::compile::Absent;

    /// Confirms the OPS-001 fix: an absent input emits
    /// `GlitchResult::NoTransition` without feeding `NaN` to the filter and
    /// poisoning its in-pulse state.
    #[test]
    fn glitch_skips_on_absent_input() {
        // Threshold 100, minimum pulse 5ms.
        let mut op = GlitchOp::new(GlitchFilter::new(100.0, 5));

        // Start the pulse (above threshold).
        let out0 = op.eval(&[Ok(110.0)], 0);
        assert_eq!(
            out0.as_any().downcast_ref::<GlitchResult>(),
            Some(&GlitchResult::NoTransition),
        );

        // Absent tick mid-pulse → no event, filter state untouched.
        let out1 = op.eval(&[Err(Absent::WarmingUp)], 3);
        assert_eq!(
            out1.as_any().downcast_ref::<GlitchResult>(),
            Some(&GlitchResult::NoTransition),
        );

        // Pulse ends 12ms after it started (≥ 5ms minimum) → ValidPulse.
        // If the absent tick had poisoned the filter (legacy NaN-substitution
        // would have terminated the pulse mid-flight), this edge would have
        // been swallowed.
        let out2 = op.eval(&[Ok(90.0)], 12);
        assert_eq!(
            out2.as_any().downcast_ref::<GlitchResult>(),
            Some(&GlitchResult::ValidPulse),
        );
    }
}
