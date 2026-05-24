//! EMA operator. Moved out of `windows/mod.rs` by `StructureOS` `move`.

use crate::checkpoint;
use crate::primitives::{CountEma, TimeEma};
use serde::{Deserialize, Serialize};
use tflo_core::compile::{Computed, NodeOutput, finite_or_warming};
use tflo_core::operator::{Operator, OperatorLoadError, require};

/// Exponential moving average over a time- or count-based window.
///
/// EMA keeps a single recursively smoothed value rather than a sliding buffer,
/// so it is not a [`Windowed`](crate::shapes::Windowed) reduction. Wraps the
/// [`TimeEma`] / [`CountEma`] primitives — same primitives the legacy
/// `tflo-core` catalog used, so results are bit-identical.
#[derive(Serialize, Deserialize)]
pub(crate) enum Ema {
    /// Time-decayed EMA (halflife-based).
    Time(TimeEma),
    /// Count-based EMA (period-based smoothing factor).
    Count(CountEma),
}

impl Operator for Ema {
    fn eval(&mut self, inputs: &[Computed], ts: i64) -> NodeOutput {
        let v = match require(inputs, 0) {
            Ok(v) => v,
            Err(e) => return NodeOutput::computed(Err(e)),
        };
        let out = match self {
            Self::Time(e) => e.push(ts, v),
            Self::Count(e) => e.push(v),
        };
        NodeOutput::computed(finite_or_warming(out))
    }

    fn name(&self) -> &str {
        "ema"
    }

    fn save(&self) -> Option<Vec<u8>> {
        checkpoint::save(self)
    }

    fn load(&mut self, bytes: &[u8]) -> Result<(), OperatorLoadError> {
        checkpoint::load(self, bytes)
    }
}
