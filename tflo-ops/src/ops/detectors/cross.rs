//! Cross-detection operators (`cross`, `cross_above`, `cross_under`,
//! `cross_hysteresis`). Moved out of the parent module
//! `move` action; see `mod.rs` for the public extension traits.

use crate::checkpoint;
use crate::events::ThresholdCrossEventMode;
use crate::primitives::{CrossDetector, HysteresisCrossDetector};
use serde::{Deserialize, Serialize};
use tflo_core::compile::{Computed, NodeOutput};
use tflo_core::operator::{Operator, OperatorLoadError, require};

/// Which `CrossDetector` update method a [`CrossOp`] runs.
#[derive(Serialize, Deserialize, Clone, Copy)]
pub(crate) enum CrossMode {
    /// Report a cross in either direction.
    Both,
    /// Report only crosses above the threshold.
    Above,
    /// Report only crosses below the threshold.
    Under,
}

/// Threshold-crossing detector — `cross` / `cross_above` / `cross_under`.
#[derive(Serialize, Deserialize)]
pub(crate) struct CrossOp {
    mode: CrossMode,
    detector: CrossDetector,
}

impl CrossOp {
    pub(crate) const fn new(mode: CrossMode) -> Self {
        Self {
            mode,
            detector: CrossDetector::new(),
        }
    }
}

impl Operator for CrossOp {
    fn eval(&mut self, inputs: &[Computed], _ts: i64) -> NodeOutput {
        // Absent input → no event this tick. Skip the detector update entirely
        // so the next present record sees the prior value, not a NaN-polluted
        // state.
        let (Ok(value), Ok(threshold)) = (require(inputs, 0), require(inputs, 1)) else {
            return NodeOutput::other(ThresholdCrossEventMode::None);
        };
        let edge = match self.mode {
            CrossMode::Both => self.detector.update(value, threshold),
            CrossMode::Above => self.detector.update_above(value, threshold),
            CrossMode::Under => self.detector.update_below(value, threshold),
        };
        NodeOutput::other(to_event(edge))
    }

    fn name(&self) -> &str {
        match self.mode {
            CrossMode::Both => "cross",
            CrossMode::Above => "cross_above",
            CrossMode::Under => "cross_under",
        }
    }

    fn save(&self) -> Option<Vec<u8>> {
        checkpoint::save(self)
    }

    fn load(&mut self, bytes: &[u8]) -> Result<(), OperatorLoadError> {
        checkpoint::load(self, bytes)
    }
}

/// Hysteresis threshold-crossing detector — `cross_hysteresis`.
#[derive(Serialize, Deserialize)]
pub(crate) struct CrossHysteresisOp {
    detector: HysteresisCrossDetector,
}

impl CrossHysteresisOp {
    pub(crate) const fn new(detector: HysteresisCrossDetector) -> Self {
        Self { detector }
    }
}

impl Operator for CrossHysteresisOp {
    fn eval(&mut self, inputs: &[Computed], _ts: i64) -> NodeOutput {
        // Absent input → no event this tick. Skip the detector update entirely
        // so the next present record sees the prior value, not a NaN-polluted
        // state.
        let (Ok(value), Ok(threshold)) = (require(inputs, 0), require(inputs, 1)) else {
            return NodeOutput::other(ThresholdCrossEventMode::None);
        };
        let edge = self.detector.update(value, threshold);
        NodeOutput::other(to_event(edge))
    }

    fn name(&self) -> &str {
        "cross_hysteresis"
    }

    fn save(&self) -> Option<Vec<u8>> {
        checkpoint::save(self)
    }

    fn load(&mut self, bytes: &[u8]) -> Result<(), OperatorLoadError> {
        checkpoint::load(self, bytes)
    }
}

/// Map the `tflo-core` primitive's `ThresholdCrossEventMode` to the `tflo-ops`
/// copy of that enum (see [`crate::events`]).
pub(crate) const fn to_event(
    mode: crate::primitives::ThresholdCrossEventMode,
) -> ThresholdCrossEventMode {
    use crate::primitives::ThresholdCrossEventMode as Core;
    match mode {
        Core::Rising => ThresholdCrossEventMode::Rising,
        Core::Falling => ThresholdCrossEventMode::Falling,
        Core::None => ThresholdCrossEventMode::None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tflo_core::compile::Absent;

    /// Confirms the OPS-001 fix: an absent input emits
    /// `ThresholdCrossEventMode::None` (no event this tick) without advancing
    /// the detector with a `NaN`-poisoned sample.
    #[test]
    fn cross_skips_on_absent_input() {
        let mut op = CrossOp::new(CrossMode::Both);

        // First record absent → no event, detector state untouched.
        let out0 = op.eval(&[Err(Absent::WarmingUp), Ok(100.0)], 1);
        assert_eq!(
            out0.as_any().downcast_ref::<ThresholdCrossEventMode>(),
            Some(&ThresholdCrossEventMode::None),
        );

        // Threshold absent → no event.
        let out1 = op.eval(&[Ok(90.0), Err(Absent::WarmingUp)], 2);
        assert_eq!(
            out1.as_any().downcast_ref::<ThresholdCrossEventMode>(),
            Some(&ThresholdCrossEventMode::None),
        );

        // First *present* record initializes the detector at 90.0 (below 100).
        let out2 = op.eval(&[Ok(90.0), Ok(100.0)], 3);
        assert_eq!(
            out2.as_any().downcast_ref::<ThresholdCrossEventMode>(),
            Some(&ThresholdCrossEventMode::None),
        );

        // Next present record at 110.0 crosses above 100. If the absent ticks
        // above had polluted detector state with NaN, this rising edge would
        // be lost.
        let out3 = op.eval(&[Ok(110.0), Ok(100.0)], 4);
        assert_eq!(
            out3.as_any().downcast_ref::<ThresholdCrossEventMode>(),
            Some(&ThresholdCrossEventMode::Rising),
        );
    }
}
