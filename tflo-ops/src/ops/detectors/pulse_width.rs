//! Pulse-width detector operator. Moved out of the parent module by
//! `StructureOS` `move` action; see `mod.rs` for the public extension
//! trait method ([`DetectorOps::pulse_width`](super::DetectorOps::pulse_width)).

use crate::checkpoint;
use crate::events::PulseWidthResult;
use crate::primitives::PulseWidthDetector;
use serde::{Deserialize, Serialize};
use tflo_core::compile::{Computed, NodeOutput};
use tflo_core::operator::{Operator, OperatorLoadError, require};

/// Pulse-width detector — `pulse_width`.
#[derive(Serialize, Deserialize)]
pub(crate) struct PulseWidthOp {
    detector: PulseWidthDetector,
}

impl PulseWidthOp {
    pub(crate) const fn new(detector: PulseWidthDetector) -> Self {
        Self { detector }
    }
}

impl Operator for PulseWidthOp {
    fn eval(&mut self, inputs: &[Computed], ts: i64) -> NodeOutput {
        // Absent input → no event this tick. Skip the detector update entirely
        // so the next present record sees the prior level, not a NaN-polluted
        // state.
        let Ok(value) = require(inputs, 0) else {
            return NodeOutput::other::<Option<PulseWidthResult>>(None);
        };
        let result: Option<PulseWidthResult> = self.detector.update(value, ts).map(to_pulse_width);
        NodeOutput::other(result)
    }

    fn name(&self) -> &str {
        "pulse_width"
    }

    fn save(&self) -> Option<Vec<u8>> {
        checkpoint::save(self)
    }

    fn load(&mut self, bytes: &[u8]) -> Result<(), OperatorLoadError> {
        checkpoint::load(self, bytes)
    }
}

/// Map the `tflo-core` primitive's `PulseWidthResult` to the `tflo-ops` copy.
pub(crate) const fn to_pulse_width(
    result: crate::primitives::PulseWidthResult,
) -> PulseWidthResult {
    use crate::primitives::PulseWidthResult as Core;
    match result {
        Core::TooShort { width_ms } => PulseWidthResult::TooShort { width_ms },
        Core::Valid { width_ms } => PulseWidthResult::Valid { width_ms },
        Core::TooLong { width_ms } => PulseWidthResult::TooLong { width_ms },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tflo_core::compile::Absent;

    /// Confirms the OPS-001 fix: an absent input emits `None` without feeding
    /// `NaN` to the pulse-width detector and corrupting its in-pulse state.
    #[test]
    fn pulse_width_skips_on_absent_input() {
        // Threshold 100, valid range 5..=15ms.
        let mut op = PulseWidthOp::new(PulseWidthDetector::new(100.0, 5, 15));

        // Pulse starts.
        let out0 = op.eval(&[Ok(110.0)], 0);
        assert_eq!(
            out0.as_any().downcast_ref::<Option<PulseWidthResult>>(),
            Some(&None),
        );

        // Absent mid-pulse → no event, detector state untouched.
        let out1 = op.eval(&[Err(Absent::WarmingUp)], 5);
        assert_eq!(
            out1.as_any().downcast_ref::<Option<PulseWidthResult>>(),
            Some(&None),
        );

        // Pulse ends at 10ms (Valid range 5..=15). Legacy NaN-substitution
        // would have killed the pulse on the absent tick.
        let out2 = op.eval(&[Ok(90.0)], 10);
        assert_eq!(
            out2.as_any()
                .downcast_ref::<Option<PulseWidthResult>>()
                .copied()
                .flatten(),
            Some(PulseWidthResult::Valid { width_ms: 10 }),
        );
    }
}
