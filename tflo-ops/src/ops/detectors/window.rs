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
        let value = require(inputs, 0).unwrap_or(f64::NAN);
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
