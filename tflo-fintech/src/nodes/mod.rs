//! Runtime [`CustomNode`] implementations for OHLC-bound indicators.
//!
//! ADX, ATR, +DI, -DI, and KAMA need TA-Lib-exact lookback and Wilder seeding,
//! so they are real stateful nodes rather than composites. Each plugs into a
//! `tflo` graph through [`Comp::custom_node`](tflo_core::comp::Comp::custom_node).
//!
//! Input order for the three-input nodes (ADX/ATR/+DI/-DI) is
//! `[close, high, low]`.

mod talib_math;

use talib_math::{kama_last, ta_adx_last, ta_minus_di_last, ta_plus_di_last, true_range};
use tflo_core::custom_node::CustomNode;

#[inline]
fn input_at(inputs: &[f64], idx: usize) -> f64 {
    inputs.get(idx).copied().unwrap_or(f64::NAN)
}

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
    pub fn new(period: usize) -> Self {
        Self {
            high: Vec::new(),
            low: Vec::new(),
            close: Vec::new(),
            period,
        }
    }
}

impl CustomNode for AdxNode {
    fn eval(&mut self, inputs: &[f64]) -> f64 {
        self.close.push(input_at(inputs, 0));
        self.high.push(input_at(inputs, 1));
        self.low.push(input_at(inputs, 2));
        ta_adx_last(&self.high, &self.low, &self.close, self.period)
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
    pub fn new(period: usize) -> Self {
        Self {
            high: Vec::new(),
            low: Vec::new(),
            close: Vec::new(),
            period,
        }
    }
}

impl CustomNode for PlusDiNode {
    fn eval(&mut self, inputs: &[f64]) -> f64 {
        self.close.push(input_at(inputs, 0));
        self.high.push(input_at(inputs, 1));
        self.low.push(input_at(inputs, 2));
        ta_plus_di_last(&self.high, &self.low, &self.close, self.period)
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
    pub fn new(period: usize) -> Self {
        Self {
            high: Vec::new(),
            low: Vec::new(),
            close: Vec::new(),
            period,
        }
    }
}

impl CustomNode for MinusDiNode {
    fn eval(&mut self, inputs: &[f64]) -> f64 {
        self.close.push(input_at(inputs, 0));
        self.high.push(input_at(inputs, 1));
        self.low.push(input_at(inputs, 2));
        ta_minus_di_last(&self.high, &self.low, &self.close, self.period)
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
    pub fn new(period: usize) -> Self {
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

impl CustomNode for AtrNode {
    fn eval(&mut self, inputs: &[f64]) -> f64 {
        let close = input_at(inputs, 0);
        let high = input_at(inputs, 1);
        let low = input_at(inputs, 2);

        self.close.push(close);
        self.high.push(high);
        self.low.push(low);
        let n = self.close.len();
        let period = self.period;

        if n < period + 1 {
            return f64::NAN;
        }

        // On the exact trigger index: seed with SMA of first `period` TR values.
        if !self.seeded {
            let mut sum_tr = 0.0;
            for i in 0..period {
                sum_tr += true_range(self.high[i + 1], self.low[i + 1], self.close[i]);
            }
            self.prev_atr = sum_tr / period as f64;
            self.seeded = true;
            return self.prev_atr;
        }

        // Ongoing: Wilder smoothing — ATR = (prev_ATR * (period - 1) + TR) / period.
        let prev_tr = true_range(high, low, self.close[n - 2]);
        self.prev_atr = (self.prev_atr * (period as f64 - 1.0) + prev_tr) / period as f64;
        self.prev_atr
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
    pub fn new(period: usize) -> Self {
        Self {
            values: Vec::new(),
            period,
        }
    }
}

impl CustomNode for KamaNode {
    fn eval(&mut self, inputs: &[f64]) -> f64 {
        self.values.push(input_at(inputs, 0));
        kama_last(&self.values, self.period)
    }

    fn reset(&mut self) {
        self.values.clear();
    }

    fn name(&self) -> &str {
        "kama"
    }
}
