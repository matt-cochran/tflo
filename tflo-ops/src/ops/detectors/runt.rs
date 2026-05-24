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
        let value = require(inputs, 0).unwrap_or(f64::NAN);
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
