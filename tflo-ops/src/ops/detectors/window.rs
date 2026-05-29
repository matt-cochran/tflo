//! Window-band detector operator. Moved out of the parent module by
//! `StructureOS` `move` action; see `mod.rs` for the public extension
//! trait method ([`DetectorOps::window_detect`](super::DetectorOps::window_detect)).

use crate::checkpoint;
use crate::events::WindowEvent;
use crate::primitives::WindowDetector;
use serde::{Deserialize, Serialize};
use tflo_core::compile::{Computed, NodeOutput};
use tflo_core::operator::{Operator, OperatorLoadError, require};

/// Window detector — `window_detect`.
#[derive(Serialize, Deserialize)]
pub(crate) struct WindowDetectOp {
    detector: WindowDetector,
}

impl WindowDetectOp {
    pub(crate) const fn new(detector: WindowDetector) -> Self {
        Self { detector }
    }
}

impl Operator for WindowDetectOp {
    fn eval(&mut self, inputs: &[Computed], _ts: i64) -> NodeOutput {
        // Absent input → no event this tick. Skip the detector update entirely
        // so the next present record sees the prior level, not a NaN-polluted
        // state.
        let Ok(value) = require(inputs, 0) else {
            return NodeOutput::other::<Option<WindowEvent>>(None);
        };
        let result: Option<WindowEvent> = self.detector.update(value).map(to_window_event);
        NodeOutput::other(result)
    }

    fn name(&self) -> &str {
        "window_detect"
    }

    fn save(&self) -> Option<Vec<u8>> {
        checkpoint::save(self)
    }

    fn load(&mut self, bytes: &[u8]) -> Result<(), OperatorLoadError> {
        checkpoint::load(self, bytes)
    }
}

/// Map the `tflo-core` primitive's `WindowEvent` to the `tflo-ops` copy.
pub(crate) const fn to_window_event(event: crate::primitives::WindowEvent) -> WindowEvent {
    use crate::primitives::WindowEvent as Core;
    match event {
        Core::EnteredWindow => WindowEvent::EnteredWindow,
        Core::ExitedLow => WindowEvent::ExitedLow,
        Core::ExitedHigh => WindowEvent::ExitedHigh,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tflo_core::compile::Absent;

    /// Confirms the OPS-001 fix: an absent input emits `None` without feeding
    /// `NaN` to the window detector and corrupting its inside-window state.
    #[test]
    fn window_detect_skips_on_absent_input() {
        // Window 4.5..=5.5.
        let mut op = WindowDetectOp::new(WindowDetector::new(4.5, 5.5));

        // Initialize below the band.
        let out0 = op.eval(&[Ok(4.0)], 1);
        assert_eq!(
            out0.as_any().downcast_ref::<Option<WindowEvent>>(),
            Some(&None),
        );

        // Absent → no event, detector state untouched.
        let out1 = op.eval(&[Err(Absent::WarmingUp)], 2);
        assert_eq!(
            out1.as_any().downcast_ref::<Option<WindowEvent>>(),
            Some(&None),
        );

        // Crossing into the band must still fire EnteredWindow. Legacy
        // NaN-substitution could have left the detector in an undefined
        // inside/outside state, swallowing this event.
        let out2 = op.eval(&[Ok(5.0)], 3);
        assert_eq!(
            out2.as_any()
                .downcast_ref::<Option<WindowEvent>>()
                .copied()
                .flatten(),
            Some(WindowEvent::EnteredWindow),
        );
    }
}
