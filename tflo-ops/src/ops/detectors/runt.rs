//! Runt-pulse detector operator. Moved out of the parent module by
//! `StructureOS` `move` action; see `mod.rs` for the public extension trait
//! method ([`DetectorOps::runt_detect`](super::DetectorOps::runt_detect)).

use crate::checkpoint;
use crate::events::RuntResult;
use crate::primitives::RuntDetector;
use serde::{Deserialize, Serialize};
use tflo_core::compile::{Computed, NodeOutput};
use tflo_core::operator::{Operator, OperatorLoadError, require};

/// Runt detector — `runt_detect`.
#[derive(Serialize, Deserialize)]
pub(crate) struct RuntOp {
    detector: RuntDetector,
}

impl RuntOp {
    pub(crate) const fn new(detector: RuntDetector) -> Self {
        Self { detector }
    }
}

impl Operator for RuntOp {
    fn eval(&mut self, inputs: &[Computed], _ts: i64) -> NodeOutput {
        // Absent input → no event this tick. Skip the detector update entirely
        // so the next present record sees the prior level, not a NaN-polluted
        // state.
        let Ok(value) = require(inputs, 0) else {
            return NodeOutput::other::<Option<RuntResult>>(None);
        };
        let result: Option<RuntResult> = self.detector.update(value).map(to_runt);
        NodeOutput::other(result)
    }

    fn name(&self) -> &str {
        "runt_detect"
    }

    fn save(&self) -> Option<Vec<u8>> {
        checkpoint::save(self)
    }

    fn load(&mut self, bytes: &[u8]) -> Result<(), OperatorLoadError> {
        checkpoint::load(self, bytes)
    }
}

/// Map the `tflo-core` primitive's `RuntResult` to the `tflo-ops` copy.
pub(crate) const fn to_runt(result: crate::primitives::RuntResult) -> RuntResult {
    use crate::primitives::RuntResult as Core;
    match result {
        Core::Runt { peak } => RuntResult::Runt { peak },
        Core::ValidPulse { peak } => RuntResult::ValidPulse { peak },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tflo_core::compile::Absent;

    /// Confirms the OPS-001 fix: an absent input emits `None` without feeding
    /// `NaN` to the runt detector and corrupting its in-pulse peak tracking.
    #[test]
    fn runt_skips_on_absent_input() {
        // Low 30, high 70.
        let mut op = RuntOp::new(RuntDetector::new(30.0, 70.0));

        // Below low → None.
        let out0 = op.eval(&[Ok(20.0)], 1);
        assert_eq!(
            out0.as_any().downcast_ref::<Option<RuntResult>>(),
            Some(&None),
        );

        // Cross into the band — peak 50 (below high).
        let out1 = op.eval(&[Ok(50.0)], 2);
        assert_eq!(
            out1.as_any().downcast_ref::<Option<RuntResult>>(),
            Some(&None),
        );

        // Absent mid-pulse → no event, peak state untouched.
        let out2 = op.eval(&[Err(Absent::WarmingUp)], 3);
        assert_eq!(
            out2.as_any().downcast_ref::<Option<RuntResult>>(),
            Some(&None),
        );

        // Back below low → Runt {peak: 50}. Legacy NaN-substitution would
        // have either dropped the pulse or corrupted the peak value.
        let out3 = op.eval(&[Ok(25.0)], 4);
        assert_eq!(
            out3.as_any()
                .downcast_ref::<Option<RuntResult>>()
                .copied()
                .flatten(),
            Some(RuntResult::Runt { peak: 50.0 }),
        );
    }
}
