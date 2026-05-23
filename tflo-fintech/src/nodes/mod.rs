//! Runtime [`Operator`] implementations for OHLC-bound indicators.
//!
//! ADX, ATR, +DI, -DI, and KAMA need TA-Lib-exact lookback and Wilder seeding,
//! so they are real stateful nodes rather than composites. Each plugs into a
//! `tflo` graph through [`Comp::custom_node`](tflo_core::comp::Comp::custom_node).
//!
//! Input order for the three-input nodes (ADX/ATR/+DI/-DI) is
//! `[close, high, low]`. Each `eval` reads and propagates every input
//! *before* touching its OHLC history, so an absent input never lands a
//! placeholder in the buffers.

mod talib_math;

use talib_math::{kama_last, ta_adx_last, ta_minus_di_last, ta_plus_di_last, true_range};
use tflo_core::compile::{Absent, Computed, NodeOutput, finite_or_warming};
use tflo_core::operator::{Operator, require};

/// Average Directional Index (ADX) — Wilder-smoothed trend strength.
///
/// Inputs: `[close, high, low]`.
#[derive(Debug, Default)]
pub struct AdxNode {
    high: Vec<f64>,
    low: Vec<f64>,
    close: Vec<f64>,
    period: usize,
}

impl AdxNode {
    /// Create an ADX node with the given lookback period.
    #[must_use]
    pub const fn new(period: usize) -> Self {
        Self {
            high: Vec::new(),
            low: Vec::new(),
            close: Vec::new(),
            period,
        }
    }
}

impl Operator for AdxNode {
    fn eval(&mut self, inputs: &[Computed], _ts: i64) -> NodeOutput {
        let close = match require(inputs, 0) {
            Ok(v) => v,
            Err(e) => return NodeOutput::computed(Err(e)),
        };
        let high = match require(inputs, 1) {
            Ok(v) => v,
            Err(e) => return NodeOutput::computed(Err(e)),
        };
        let low = match require(inputs, 2) {
            Ok(v) => v,
            Err(e) => return NodeOutput::computed(Err(e)),
        };
        self.close.push(close);
        self.high.push(high);
        self.low.push(low);
        NodeOutput::computed(finite_or_warming(ta_adx_last(
            &self.high,
            &self.low,
            &self.close,
            self.period,
        )))
    }

    fn reset(&mut self) {
        self.high.clear();
        self.low.clear();
        self.close.clear();
    }

    fn name(&self) -> &str {
        "adx"
    }
}

/// Plus Directional Indicator (+DI).
///
/// Inputs: `[close, high, low]`.
#[derive(Debug, Default)]
pub struct PlusDiNode {
    high: Vec<f64>,
    low: Vec<f64>,
    close: Vec<f64>,
    period: usize,
}

impl PlusDiNode {
    /// Create a +DI node with the given lookback period.
    #[must_use]
    pub const fn new(period: usize) -> Self {
        Self {
            high: Vec::new(),
            low: Vec::new(),
            close: Vec::new(),
            period,
        }
    }
}

impl Operator for PlusDiNode {
    fn eval(&mut self, inputs: &[Computed], _ts: i64) -> NodeOutput {
        let close = match require(inputs, 0) {
            Ok(v) => v,
            Err(e) => return NodeOutput::computed(Err(e)),
        };
        let high = match require(inputs, 1) {
            Ok(v) => v,
            Err(e) => return NodeOutput::computed(Err(e)),
        };
        let low = match require(inputs, 2) {
            Ok(v) => v,
            Err(e) => return NodeOutput::computed(Err(e)),
        };
        self.close.push(close);
        self.high.push(high);
        self.low.push(low);
        NodeOutput::computed(finite_or_warming(ta_plus_di_last(
            &self.high,
            &self.low,
            &self.close,
            self.period,
        )))
    }

    fn reset(&mut self) {
        self.high.clear();
        self.low.clear();
        self.close.clear();
    }

    fn name(&self) -> &str {
        "plus_di"
    }
}

/// Minus Directional Indicator (-DI).
///
/// Inputs: `[close, high, low]`.
#[derive(Debug, Default)]
pub struct MinusDiNode {
    high: Vec<f64>,
    low: Vec<f64>,
    close: Vec<f64>,
    period: usize,
}

impl MinusDiNode {
    /// Create a -DI node with the given lookback period.
    #[must_use]
    pub const fn new(period: usize) -> Self {
        Self {
            high: Vec::new(),
            low: Vec::new(),
            close: Vec::new(),
            period,
        }
    }
}

impl Operator for MinusDiNode {
    fn eval(&mut self, inputs: &[Computed], _ts: i64) -> NodeOutput {
        let close = match require(inputs, 0) {
            Ok(v) => v,
            Err(e) => return NodeOutput::computed(Err(e)),
        };
        let high = match require(inputs, 1) {
            Ok(v) => v,
            Err(e) => return NodeOutput::computed(Err(e)),
        };
        let low = match require(inputs, 2) {
            Ok(v) => v,
            Err(e) => return NodeOutput::computed(Err(e)),
        };
        self.close.push(close);
        self.high.push(high);
        self.low.push(low);
        NodeOutput::computed(finite_or_warming(ta_minus_di_last(
            &self.high,
            &self.low,
            &self.close,
            self.period,
        )))
    }

    fn reset(&mut self) {
        self.high.clear();
        self.low.clear();
        self.close.clear();
    }

    fn name(&self) -> &str {
        "minus_di"
    }
}

/// Average True Range (ATR) with Wilder's smoothing.
///
/// Inputs: `[close, high, low]`.
#[derive(Debug, Default)]
pub struct AtrNode {
    high: Vec<f64>,
    low: Vec<f64>,
    close: Vec<f64>,
    period: usize,
    prev_atr: f64,
    seeded: bool,
}

impl AtrNode {
    /// Create an ATR node with the given lookback period.
    #[must_use]
    pub const fn new(period: usize) -> Self {
        Self {
            high: Vec::new(),
            low: Vec::new(),
            close: Vec::new(),
            period,
            prev_atr: 0.0,
            seeded: false,
        }
    }
}

impl Operator for AtrNode {
    fn eval(&mut self, inputs: &[Computed], _ts: i64) -> NodeOutput {
        let close = match require(inputs, 0) {
            Ok(v) => v,
            Err(e) => return NodeOutput::computed(Err(e)),
        };
        let high = match require(inputs, 1) {
            Ok(v) => v,
            Err(e) => return NodeOutput::computed(Err(e)),
        };
        let low = match require(inputs, 2) {
            Ok(v) => v,
            Err(e) => return NodeOutput::computed(Err(e)),
        };

        self.close.push(close);
        self.high.push(high);
        self.low.push(low);
        let n = self.close.len();
        let period = self.period;

        if n < period + 1 {
            return NodeOutput::computed(Err(Absent::WarmingUp));
        }

        // On the exact trigger index: seed with SMA of first `period` TR values.
        if !self.seeded {
            let mut sum_tr = 0.0;
            for i in 0..period {
                sum_tr += true_range(self.high[i + 1], self.low[i + 1], self.close[i]);
            }
            self.prev_atr = sum_tr / period as f64;
            self.seeded = true;
            return NodeOutput::computed(Ok(self.prev_atr));
        }

        // Ongoing: Wilder smoothing — ATR = (prev_ATR * (period - 1) + TR) / period.
        let prev_tr = true_range(high, low, self.close[n - 2]);
        self.prev_atr = (self.prev_atr * (period as f64 - 1.0) + prev_tr) / period as f64;
        NodeOutput::computed(Ok(self.prev_atr))
    }

    fn reset(&mut self) {
        self.high.clear();
        self.low.clear();
        self.close.clear();
        self.prev_atr = 0.0;
        self.seeded = false;
    }

    fn name(&self) -> &str {
        "atr"
    }
}

/// Kaufman Adaptive Moving Average (KAMA).
///
/// Input: `[value]`.
#[derive(Debug, Default)]
pub struct KamaNode {
    values: Vec<f64>,
    period: usize,
}

impl KamaNode {
    /// Create a KAMA node with the given efficiency-ratio period.
    #[must_use]
    pub const fn new(period: usize) -> Self {
        Self {
            values: Vec::new(),
            period,
        }
    }
}

impl Operator for KamaNode {
    fn eval(&mut self, inputs: &[Computed], _ts: i64) -> NodeOutput {
        let value = match require(inputs, 0) {
            Ok(v) => v,
            Err(e) => return NodeOutput::computed(Err(e)),
        };
        self.values.push(value);
        NodeOutput::computed(finite_or_warming(kama_last(&self.values, self.period)))
    }

    fn reset(&mut self) {
        self.values.clear();
    }

    fn name(&self) -> &str {
        "kama"
    }
}
