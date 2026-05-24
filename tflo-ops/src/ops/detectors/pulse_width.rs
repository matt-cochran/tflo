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
        let value = require(inputs, 0).unwrap_or(f64::NAN);
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
