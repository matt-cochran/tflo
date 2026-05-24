//! Glitch-filter detector operator. Moved out of the parent module by
//! `StructureOS` `move` action; see `mod.rs` for the public extension
//! traits ([`DetectorOps::glitch_filter`](super::DetectorOps::glitch_filter)).

use crate::checkpoint;
use crate::events::GlitchResult;
use crate::primitives::GlitchFilter;
use serde::{Deserialize, Serialize};
use tflo_core::compile::{Computed, NodeOutput};
use tflo_core::operator::{Operator, OperatorLoadError, require};

/// Glitch filter — `glitch_filter`.
///
/// Wraps [`GlitchFilter`]; an absent input is substituted with `f64::NAN`,
/// matching the legacy `eval_glitch`. The `Option<bool>` the primitive returns
/// is mapped to a [`GlitchResult`]: `Some(true)` → `ValidPulse`,
/// `Some(false)` → `Rejected`, `None` → `NoTransition`.
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
        let value = require(inputs, 0).unwrap_or(f64::NAN);
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
