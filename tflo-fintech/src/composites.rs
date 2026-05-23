//! Financial technical-analysis indicators as an extension trait on `Comp`.
//!
//! Every method here builds on `tflo-core`'s public `Comp` API — windowed
//! aggregations, closure scans, arithmetic — plus the OHLC-bound custom nodes
//! in [`crate::nodes`]. Bring [`FintechIndicators`] into scope to use them.

use crate::nodes::{AdxNode, AtrNode, KamaNode, MinusDiNode, PlusDiNode};
use std::collections::VecDeque;
use tflo_core::comp::Comp;
use tflo_core::window::Window;
use tflo_ops::prelude::*;

/// Financial technical-analysis indicators layered on the `tflo` graph.
///
/// Implemented for `Comp<R, f64>`. The receiver (`self`) is the primary input
/// — typically the close price.
pub trait FintechIndicators<R> {
    /// MACD: returns `(macd_line, signal_line, histogram)`.
    ///
    /// `macd_line` = fast EMA − slow EMA; `signal_line` = EMA of `macd_line`;
    /// `histogram` = `macd_line − signal_line`. Standard: fast 12, slow 26,
    /// signal 9.
    fn macd_n(&self, fast: usize, slow: usize, signal: usize) -> (Comp<R>, Comp<R>, Comp<R>);

    /// Stochastic Oscillator %K = `(close − lowest) / (highest − lowest) * 100`.
    fn stochastic_k(&self, window: impl Into<Window>) -> Comp<R>;

    /// Stochastic Oscillator: returns `(%K, %D)`, where %D is an SMA of %K.
    fn stochastic_n(&self, k_period: usize, d_period: usize) -> (Comp<R>, Comp<R>);

    /// Stochastic Oscillator using explicit `high`/`low` inputs.
    fn stochastic_ohlc_n(
        &self,
        high: &Comp<R>,
        low: &Comp<R>,
        k_period: usize,
        d_period: usize,
    ) -> (Comp<R>, Comp<R>);

    /// Williams %R: `(highest − close) / (highest − lowest) * -100`.
    fn williams_r_n(&self, n: usize) -> Comp<R>;

    /// Williams %R using explicit `high`/`low` inputs.
    fn williams_r_ohlc_n(&self, high: &Comp<R>, low: &Comp<R>, n: usize) -> Comp<R>;

    /// Commodity Channel Index. Assumes `self` is the typical price.
    fn cci_n(&self, n: usize) -> Comp<R>;

    /// Gap-aware true range from `self` (close), `high`, and `low`.
    fn true_range(&self, high: &Comp<R>, low: &Comp<R>) -> Comp<R>;

    /// Average True Range as an EMA of true range.
    fn atr_n(&self, high: &Comp<R>, low: &Comp<R>, n: usize) -> Comp<R>;

    /// Average True Range with TA-Lib-compatible Wilder smoothing.
    fn atr_wilder_n(&self, high: &Comp<R>, low: &Comp<R>, period: usize) -> Comp<R>;

    /// Triangular Moving Average — SMA of SMA.
    fn trima(&self, period: usize) -> Comp<R>;

    /// Double Exponential Moving Average: `2*EMA1 − EMA2`.
    fn dema_n(&self, period: usize) -> Comp<R>;

    /// Triple Exponential Moving Average: `3*EMA1 − 3*EMA2 + EMA3`.
    fn tema_n(&self, period: usize) -> Comp<R>;

    /// Typical price: `(high + low + close) / 3`. `self` is the close.
    fn typical_price(&self, high: &Comp<R>, low: &Comp<R>) -> Comp<R>;

    /// Volume Weighted Average Price: `cumsum(price*volume) / cumsum(volume)`.
    fn vwap(&self, volume: &Comp<R>) -> Comp<R>;

    /// On-Balance Volume. `self` is the close.
    fn obv(&self, volume: &Comp<R>) -> Comp<R>;

    /// Money Flow Index. `self` is the typical price.
    fn mfi_n(&self, volume: &Comp<R>, n: usize) -> Comp<R>;

    /// Average Directional Index (Wilder-smoothed trend strength).
    fn adx_n(&self, high: &Comp<R>, low: &Comp<R>, period: usize) -> Comp<R>;

    /// Plus Directional Indicator (+DI).
    fn plus_di_n(&self, high: &Comp<R>, low: &Comp<R>, period: usize) -> Comp<R>;

    /// Minus Directional Indicator (-DI).
    fn minus_di_n(&self, high: &Comp<R>, low: &Comp<R>, period: usize) -> Comp<R>;

    /// Kaufman Adaptive Moving Average.
    fn kama_n(&self, period: usize) -> Comp<R>;

    /// Chande Momentum Oscillator (RSI-like, range −100..+100).
    fn cmo_n(&self, period: usize) -> Comp<R>;

    /// Slope of the least-squares regression line over the last `period` values.
    fn linearreg_slope_n(&self, period: usize) -> Comp<R>;

    /// Percentage Price Oscillator: `((fast_ema − slow_ema) / slow_ema) * 100`.
    fn ppo_n(&self, fast: usize, slow: usize) -> Comp<R>;

    /// TRIX — 1-period change rate of a triple-smoothed EMA.
    fn trix_n(&self, period: usize) -> Comp<R>;

    /// Stochastic RSI: returns `(fast_k, fast_d)`.
    fn stochrsi_n(&self, rsi_period: usize, fastk: usize, fastd: usize) -> (Comp<R>, Comp<R>);
}

impl<R: 'static> FintechIndicators<R> for Comp<R, f64> {
    fn macd_n(&self, fast: usize, slow: usize, signal: usize) -> (Self, Self, Self) {
        let line = self.scan_f64(Vec::<f64>::new, move |data, value| {
            data.push(value);
            macd_last(data, fast, slow, signal, 0)
        });
        let sig = self.scan_f64(Vec::<f64>::new, move |data, value| {
            data.push(value);
            macd_last(data, fast, slow, signal, 1)
        });
        let hist = self.scan_f64(Vec::<f64>::new, move |data, value| {
            data.push(value);
            macd_last(data, fast, slow, signal, 2)
        });
        (line, sig, hist)
    }

    fn stochastic_k(&self, window: impl Into<Window>) -> Self {
        let w: Window = window.into();
        let highest = self.max(w);
        let lowest = self.min(w);
        let range = &highest - &lowest;
        ((self - &lowest) / &range) * 100.0
    }

    fn stochastic_n(&self, k_period: usize, d_period: usize) -> (Self, Self) {
        let k = self.stochastic_k(k_period);
        let d = k.sma(d_period);
        (k, d)
    }

    fn stochastic_ohlc_n(
        &self,
        high: &Self,
        low: &Self,
        k_period: usize,
        d_period: usize,
    ) -> (Self, Self) {
        let highest = high.max(k_period);
        let lowest = low.min(k_period);
        let range = &highest - &lowest;
        let fast_k = ((self - &lowest) / &range) * 100.0;
        let slow_k = fast_k.sma(d_period);
        let slow_d = slow_k.sma(d_period);
        (slow_k, slow_d)
    }

    fn williams_r_n(&self, n: usize) -> Self {
        let highest = self.max(n);
        let lowest = self.min(n);
        let range = &highest - &lowest;
        ((&highest - self) / &range) * -100.0
    }

    fn williams_r_ohlc_n(&self, high: &Self, low: &Self, n: usize) -> Self {
        let highest = high.max(n);
        let lowest = low.min(n);
        let range = &highest - &lowest;
        ((&highest - self) / &range) * -100.0
    }

    fn cci_n(&self, n: usize) -> Self {
        self.scan_f64(
            move || VecDeque::<f64>::with_capacity(n),
            move |window, value| {
                window.push_back(value);
                if window.len() > n {
                    window.pop_front();
                }
                if window.len() < n {
                    return f64::NAN;
                }
                let mean = window.iter().sum::<f64>() / n as f64;
                let mean_deviation =
                    window.iter().map(|x| (x - mean).abs()).sum::<f64>() / n as f64;
                if mean_deviation == 0.0 {
                    0.0
                } else {
                    (value - mean) / (0.015 * mean_deviation)
                }
            },
        )
    }

    fn true_range(&self, high: &Self, low: &Self) -> Self {
        let prev_close = self.prev();
        let hl = high - low;
        let hpc = (high - &prev_close).abs();
        let lpc = (low - &prev_close).abs();
        let max_gap = hpc.map2_f64(&lpc, f64::max);
        hl.map2_f64(&max_gap, f64::max)
    }

    fn atr_n(&self, high: &Self, low: &Self, n: usize) -> Self {
        self.true_range(high, low).ema(n)
    }

    fn atr_wilder_n(&self, high: &Self, low: &Self, period: usize) -> Self {
        Self::custom_node(self, &[high, low], move || AtrNode::new(period))
    }

    fn trima(&self, period: usize) -> Self {
        self.scan_f64(Vec::<f64>::new, move |data, value| {
            data.push(value);
            trima_last(data, period)
        })
    }

    fn dema_n(&self, period: usize) -> Self {
        self.scan_f64(Vec::<f64>::new, move |data, value| {
            data.push(value);
            dema_last(data, period)
        })
    }

    fn tema_n(&self, period: usize) -> Self {
        self.scan_f64(Vec::<f64>::new, move |data, value| {
            data.push(value);
            tema_last(data, period)
        })
    }

    fn typical_price(&self, high: &Self, low: &Self) -> Self {
        let sum = high + low;
        (&sum + self) / 3.0
    }

    fn vwap(&self, volume: &Self) -> Self {
        let pv = self * volume;
        let pv_sum = pv.cumsum();
        let vol_sum = volume.cumsum();
        &pv_sum / &vol_sum
    }

    fn obv(&self, volume: &Self) -> Self {
        self.scan2_f64(volume, ObvState::default, obv_step)
    }

    fn mfi_n(&self, volume: &Self, n: usize) -> Self {
        self.scan2_f64(volume, move || MfiState::new(n), mfi_step)
    }

    fn adx_n(&self, high: &Self, low: &Self, period: usize) -> Self {
        Self::custom_node(self, &[high, low], move || AdxNode::new(period))
    }

    fn plus_di_n(&self, high: &Self, low: &Self, period: usize) -> Self {
        Self::custom_node(self, &[high, low], move || PlusDiNode::new(period))
    }

    fn minus_di_n(&self, high: &Self, low: &Self, period: usize) -> Self {
        Self::custom_node(self, &[high, low], move || MinusDiNode::new(period))
    }

    fn kama_n(&self, period: usize) -> Self {
        self.custom_node1(move || KamaNode::new(period))
    }

    fn cmo_n(&self, period: usize) -> Self {
        #[derive(Default)]
        struct CmoState {
            prev: Option<f64>,
            count: usize,
            sum_gain: f64,
            sum_loss: f64,
            avg_gain: f64,
            avg_loss: f64,
        }

        self.scan_f64(CmoState::default, move |state, value| {
            if period == 0 {
                return f64::NAN;
            }

            let Some(prev) = state.prev else {
                state.prev = Some(value);
                return f64::NAN;
            };
            state.prev = Some(value);

            let diff = value - prev;
            let gain = if diff > 0.0 { diff } else { 0.0 };
            let loss = if diff < 0.0 { -diff } else { 0.0 };
            state.count += 1;

            if state.count <= period {
                state.sum_gain += gain;
                state.sum_loss += loss;
                if state.count < period {
                    return f64::NAN;
                }
                state.avg_gain = state.sum_gain / period as f64;
                state.avg_loss = state.sum_loss / period as f64;
            } else {
                state.avg_gain = (state.avg_gain * (period - 1) as f64 + gain) / period as f64;
                state.avg_loss = (state.avg_loss * (period - 1) as f64 + loss) / period as f64;
            }

            let denom = state.avg_gain + state.avg_loss;
            if denom.abs() < 1e-15 {
                0.0
            } else {
                100.0 * (state.avg_gain - state.avg_loss) / denom
            }
        })
    }

    fn linearreg_slope_n(&self, period: usize) -> Self {
        self.scan_f64(
            move || VecDeque::<f64>::with_capacity(period),
            move |buf, val| {
                if period == 0 {
                    return f64::NAN;
                }
                buf.push_back(val);
                if buf.len() > period {
                    buf.pop_front();
                }
                if buf.len() < period {
                    return f64::NAN;
                }

                let n = period as f64;
                let sum_x = n * (n - 1.0) / 2.0;
                let sum_x2 = n * (n - 1.0) * (2.0 * n - 1.0) / 6.0;

                let mut sum_y = 0.0;
                let mut sum_xy = 0.0;
                for (i, &v) in buf.iter().enumerate() {
                    sum_y += v;
                    sum_xy += i as f64 * v;
                }

                let denom = n * sum_x2 - sum_x * sum_x;
                if denom.abs() < 1e-15 {
                    return f64::NAN;
                }
                (n * sum_xy - sum_x * sum_y) / denom
            },
        )
    }

    fn ppo_n(&self, fast: usize, slow: usize) -> Self {
        self.scan_f64(Vec::<f64>::new, move |data, value| {
            data.push(value);
            ppo_last(data, fast, slow)
        })
    }

    fn trix_n(&self, period: usize) -> Self {
        self.scan_f64(Vec::<f64>::new, move |data, value| {
            data.push(value);
            trix_last(data, period)
        })
    }

    fn stochrsi_n(&self, rsi_period: usize, fastk: usize, fastd: usize) -> (Self, Self) {
        let k = self.scan_f64(Vec::<f64>::new, move |data, value| {
            data.push(value);
            stochrsi_last(data, rsi_period, fastk, fastd, 0)
        });
        let d = self.scan_f64(Vec::<f64>::new, move |data, value| {
            data.push(value);
            stochrsi_last(data, rsi_period, fastk, fastd, 1)
        });
        (k, d)
    }
}

// ── Free helper functions (moved verbatim from tflo-core) ──────────────────

#[derive(Default)]
struct ObvState {
    prev_close: Option<f64>,
    obv: f64,
}

fn obv_step(state: &mut ObvState, close: f64, volume: f64) -> f64 {
    match state.prev_close {
        None => {
            state.prev_close = Some(close);
            state.obv = volume;
        }
        Some(prev_close) => {
            if close > prev_close {
                state.obv += volume;
            } else if close < prev_close {
                state.obv -= volume;
            }
            state.prev_close = Some(close);
        }
    }
    state.obv
}

struct MfiState {
    period: usize,
    prev_typical_price: Option<f64>,
    positive_flows: VecDeque<f64>,
    negative_flows: VecDeque<f64>,
    positive_sum: f64,
    negative_sum: f64,
}

impl MfiState {
    fn new(period: usize) -> Self {
        Self {
            period,
            prev_typical_price: None,
            positive_flows: VecDeque::with_capacity(period),
            negative_flows: VecDeque::with_capacity(period),
            positive_sum: 0.0,
            negative_sum: 0.0,
        }
    }
}

fn mfi_step(state: &mut MfiState, typical_price: f64, volume: f64) -> f64 {
    if state.period == 0 {
        return f64::NAN;
    }

    let Some(prev_typical_price) = state.prev_typical_price else {
        state.prev_typical_price = Some(typical_price);
        return f64::NAN;
    };

    let raw_money_flow = typical_price * volume;
    let positive_flow = if typical_price > prev_typical_price {
        raw_money_flow
    } else {
        0.0
    };
    let negative_flow = if typical_price < prev_typical_price {
        raw_money_flow
    } else {
        0.0
    };

    state.prev_typical_price = Some(typical_price);
    state.positive_flows.push_back(positive_flow);
    state.negative_flows.push_back(negative_flow);
    state.positive_sum += positive_flow;
    state.negative_sum += negative_flow;

    if state.positive_flows.len() > state.period {
        if let Some(old) = state.positive_flows.pop_front() {
            state.positive_sum -= old;
        }
    }
    if state.negative_flows.len() > state.period {
        if let Some(old) = state.negative_flows.pop_front() {
            state.negative_sum -= old;
        }
    }

    if state.positive_flows.len() < state.period {
        return f64::NAN;
    }

    if state.negative_sum == 0.0 {
        if state.positive_sum == 0.0 {
            50.0
        } else {
            100.0
        }
    } else {
        let money_ratio = state.positive_sum / state.negative_sum;
        100.0 - 100.0 / (1.0 + money_ratio)
    }
}

fn ema_series(data: &[f64], period: usize) -> Vec<Option<f64>> {
    let n = data.len();
    let mut out = vec![None; n];
    if n < period || period == 0 {
        return out;
    }
    let alpha = 2.0 / (period as f64 + 1.0);
    let mut val = data[..period].iter().sum::<f64>() / period as f64;
    out[period - 1] = Some(val);
    for i in period..n {
        if data[i].is_nan() {
            continue;
        }
        val = alpha * data[i] + (1.0 - alpha) * val;
        out[i] = Some(val);
    }
    out
}

fn trima_last(data: &[f64], period: usize) -> f64 {
    let n = data.len();
    if period == 0 || n < period {
        return f64::NAN;
    }

    let window = &data[n - period..n];
    let half = period / 2;
    let mut weighted_sum = 0.0;
    let mut weight_sum = 0.0;

    for (i, value) in window.iter().enumerate() {
        let weight = if period % 2 == 0 {
            if i < half { i + 1 } else { period - i }
        } else if i <= half {
            i + 1
        } else {
            period - i
        } as f64;
        weighted_sum += value * weight;
        weight_sum += weight;
    }

    weighted_sum / weight_sum
}

fn dema_last(data: &[f64], period: usize) -> f64 {
    let n = data.len();
    if period == 0 || n < 2 * period - 1 {
        return f64::NAN;
    }

    let ema1 = ema_series(data, period);
    let ema1_last = match ema1[n - 1] {
        Some(v) => v,
        None => return f64::NAN,
    };

    let ema1_start = period - 1;
    let ema1_values: Vec<f64> = ema1[ema1_start..]
        .iter()
        .map(|v| v.unwrap_or(f64::NAN))
        .collect();
    let ema2 = ema_series(&ema1_values, period);
    let ema2_idx = n - period;
    let ema2_last = match ema2.get(ema2_idx).copied().flatten() {
        Some(v) => v,
        None => return f64::NAN,
    };

    2.0 * ema1_last - ema2_last
}

fn tema_last(data: &[f64], period: usize) -> f64 {
    let n = data.len();
    if period == 0 || n < 3 * period - 2 {
        return f64::NAN;
    }

    let ema1 = ema_series(data, period);
    let ema1_last = match ema1[n - 1] {
        Some(v) => v,
        None => return f64::NAN,
    };

    let ema1_start = period - 1;
    let ema1_values: Vec<f64> = ema1[ema1_start..]
        .iter()
        .map(|v| v.unwrap_or(f64::NAN))
        .collect();
    let ema2 = ema_series(&ema1_values, period);
    let ema2_idx = n - period;
    let ema2_last = match ema2.get(ema2_idx).copied().flatten() {
        Some(v) => v,
        None => return f64::NAN,
    };

    let ema2_start = period - 1;
    let ema2_values: Vec<f64> = ema2[ema2_start..]
        .iter()
        .map(|v| v.unwrap_or(f64::NAN))
        .collect();
    let ema3 = ema_series(&ema2_values, period);
    let ema3_idx = n - (2 * period - 1);
    let ema3_last = match ema3.get(ema3_idx).copied().flatten() {
        Some(v) => v,
        None => return f64::NAN,
    };

    3.0 * ema1_last - 3.0 * ema2_last + ema3_last
}

fn ppo_last(data: &[f64], fast: usize, slow: usize) -> f64 {
    if data.len() < fast.max(slow) || fast == 0 || slow == 0 {
        return f64::NAN;
    }
    let fast_ma = data[data.len() - fast..].iter().sum::<f64>() / fast as f64;
    let slow_ma = data[data.len() - slow..].iter().sum::<f64>() / slow as f64;
    if slow_ma == 0.0 {
        0.0
    } else {
        (fast_ma - slow_ma) / slow_ma * 100.0
    }
}

fn macd_last(data: &[f64], fast: usize, slow: usize, signal: usize, output: usize) -> f64 {
    let n = data.len();
    if n == 0 || slow == 0 || signal == 0 || n < slow + signal - 1 {
        return f64::NAN;
    }
    let fast_ema = ema_series(data, fast);
    let slow_ema = ema_series(data, slow);
    let macd_start = slow - 1;
    let mut line_raw = Vec::with_capacity(n.saturating_sub(macd_start));
    for i in macd_start..n {
        match (fast_ema[i], slow_ema[i]) {
            (Some(f), Some(s)) => line_raw.push(f - s),
            _ => return f64::NAN,
        }
    }
    if line_raw.len() < signal {
        return f64::NAN;
    }
    let alpha = 2.0 / (signal as f64 + 1.0);
    let mut sig = line_raw[..signal].iter().sum::<f64>() / signal as f64;
    for v in &line_raw[signal..] {
        sig = alpha * *v + (1.0 - alpha) * sig;
    }
    let Some(&line) = line_raw.last() else {
        return f64::NAN;
    };
    match output {
        0 => line,
        1 => sig,
        _ => line - sig,
    }
}

fn trix_last(data: &[f64], period: usize) -> f64 {
    let n = data.len();
    if n < period * 3 - 2 || period == 0 {
        return f64::NAN;
    }
    let ema1 = ema_series(data, period);
    let flat1: Vec<f64> = ema1.iter().map(|v| v.unwrap_or(f64::NAN)).collect();
    let ema2_start = period - 1;
    if flat1.len() <= ema2_start {
        return f64::NAN;
    }
    let ema2_vals = ema_series(&flat1[ema2_start..], period);
    let flat2: Vec<f64> = ema2_vals.iter().map(|v| v.unwrap_or(f64::NAN)).collect();
    let ema3_start = period - 1;
    if flat2.len() <= ema3_start {
        return f64::NAN;
    }
    let ema3_vals = ema_series(&flat2[ema3_start..], period);
    let idx = n - (period - 1) - (period - 1) - 1;
    if idx == 0 || idx >= ema3_vals.len() {
        return f64::NAN;
    }
    match (ema3_vals[idx - 1], ema3_vals[idx]) {
        (Some(prev), Some(cur)) if prev != 0.0 => (cur - prev) / prev * 100.0,
        _ => f64::NAN,
    }
}

fn rsi_series(data: &[f64], period: usize) -> Vec<Option<f64>> {
    let n = data.len();
    let mut out = vec![None; n];
    if n < period + 1 || period == 0 {
        return out;
    }
    let mut gains = Vec::with_capacity(n - 1);
    let mut losses = Vec::with_capacity(n - 1);
    for i in 1..n {
        let d = data[i] - data[i - 1];
        gains.push(if d > 0.0 { d } else { 0.0 });
        losses.push(if d < 0.0 { -d } else { 0.0 });
    }
    let mut avg_gain = gains[..period].iter().sum::<f64>() / period as f64;
    let mut avg_loss = losses[..period].iter().sum::<f64>() / period as f64;
    out[period] = Some(rsi_value(avg_gain, avg_loss));
    for i in (period + 1)..n {
        avg_gain = (avg_gain * (period - 1) as f64 + gains[i - 1]) / period as f64;
        avg_loss = (avg_loss * (period - 1) as f64 + losses[i - 1]) / period as f64;
        out[i] = Some(rsi_value(avg_gain, avg_loss));
    }
    out
}

fn rsi_value(avg_gain: f64, avg_loss: f64) -> f64 {
    if avg_loss == 0.0 {
        if avg_gain == 0.0 { 50.0 } else { 100.0 }
    } else {
        100.0 - 100.0 / (1.0 + avg_gain / avg_loss)
    }
}

fn stochrsi_last(data: &[f64], period: usize, fastk: usize, fastd: usize, output: usize) -> f64 {
    let rsi_vals = rsi_series(data, period);
    let defined: Vec<f64> = rsi_vals.iter().filter_map(|&v| v).collect();
    let n_def = defined.len();
    if n_def < fastk {
        return f64::NAN;
    }
    let mut fast_k_raw = vec![f64::NAN; n_def];
    for i in (fastk - 1)..n_def {
        let slice = &defined[i + 1 - fastk..=i];
        let mn = slice.iter().copied().fold(f64::INFINITY, f64::min);
        let mx = slice.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        let rng = mx - mn;
        fast_k_raw[i] = if rng == 0.0 {
            0.0
        } else {
            (defined[i] - mn) / rng * 100.0
        };
    }
    if output == 0 {
        return *fast_k_raw.last().unwrap_or(&f64::NAN);
    }
    if n_def < fastk + fastd - 1 {
        return f64::NAN;
    }
    let i = n_def - 1;
    fast_k_raw[i + 1 - fastd..=i].iter().sum::<f64>() / fastd as f64
}
