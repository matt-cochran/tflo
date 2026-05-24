//! RSI with Wilder's smoothing. Moved out of `windows/mod.rs` by
//! `StructureOS` `move`; the `WindowOps::rsi_wilder` builder is in `mod.rs`.

use crate::checkpoint;
use serde::{Deserialize, Serialize};
use tflo_core::compile::{Computed, NodeOutput, finite_or_warming};
use tflo_core::operator::{Operator, OperatorLoadError, require};

/// RSI using Wilder's smoothing (count-based only).
///
/// Uses an EMA with `alpha = 1/period` for gain/loss averaging, matching
/// `TradingView`'s RSI.
#[derive(Serialize, Deserialize)]
pub(crate) struct RsiWilder {
    period: usize,
    prev: Option<f64>,
    count: usize,
    sum_gain: f64,
    sum_loss: f64,
    avg_gain: f64,
    avg_loss: f64,
    initialized: bool,
}

impl RsiWilder {
    pub(crate) const fn new(period: usize) -> Self {
        Self {
            period,
            prev: None,
            count: 0,
            sum_gain: 0.0,
            sum_loss: 0.0,
            avg_gain: 0.0,
            avg_loss: 0.0,
            initialized: false,
        }
    }

    /// Fold one value into the Wilder-smoothed RSI state.
    #[allow(clippy::cast_precision_loss)]
    fn update(&mut self, value: f64) -> f64 {
        if self.period == 0 {
            return f64::NAN;
        }

        let Some(prev) = self.prev else {
            self.prev = Some(value);
            return f64::NAN;
        };

        let change = value - prev;
        let gain = if change > 0.0 { change } else { 0.0 };
        let loss = if change < 0.0 { -change } else { 0.0 };
        self.prev = Some(value);

        if !self.initialized {
            self.count += 1;
            self.sum_gain += gain;
            self.sum_loss += loss;
            if self.count < self.period {
                return f64::NAN;
            }
            self.avg_gain = self.sum_gain / self.period as f64;
            self.avg_loss = self.sum_loss / self.period as f64;
            self.initialized = true;
        } else {
            self.avg_gain =
                (self.avg_gain * (self.period - 1) as f64 + gain) / self.period as f64;
            self.avg_loss =
                (self.avg_loss * (self.period - 1) as f64 + loss) / self.period as f64;
        }

        if self.avg_loss == 0.0 {
            if self.avg_gain == 0.0 { 50.0 } else { 100.0 }
        } else {
            100.0 - 100.0 / (1.0 + self.avg_gain / self.avg_loss)
        }
    }
}

impl Operator for RsiWilder {
    fn eval(&mut self, inputs: &[Computed], _ts: i64) -> NodeOutput {
        let v = match require(inputs, 0) {
            Ok(v) => v,
            Err(e) => return NodeOutput::computed(Err(e)),
        };
        NodeOutput::computed(finite_or_warming(self.update(v)))
    }

    fn name(&self) -> &str {
        "rsi_wilder"
    }

    fn save(&self) -> Option<Vec<u8>> {
        checkpoint::save(self)
    }

    fn load(&mut self, bytes: &[u8]) -> Result<(), OperatorLoadError> {
        checkpoint::load(self, bytes)
    }
}
