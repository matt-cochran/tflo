//! Financial technical-analysis indicators as an extension trait on `Comp`.
//!
//! Every method here builds on `tflo-core`'s public `Comp` API — windowed
//! aggregations, closure scans, arithmetic — plus the OHLC-bound custom nodes
//! in [`crate::nodes`]. Bring [`FintechIndicators`] into scope to use them.

mod helpers;

use crate::nodes::{AdxNode, AtrNode, KamaNode, MinusDiNode, PlusDiNode};
use helpers::{
    MfiState, ObvState, dema_last, macd_last, mfi_step, obv_step, ppo_last, stochrsi_last,
    tema_last, trima_last, trix_last,
};
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
        // SAFETY: `Comp<R, f64>` arithmetic operates on `f64` streams; the
        // overloads cannot panic (zero range yields `inf`/`NaN` by design).
        // The lint flags them anyway because `Comp` overloads `Sub`/`Div`.
        #[allow(clippy::arithmetic_side_effects)]
        let range = &highest - &lowest;
        #[allow(clippy::arithmetic_side_effects)]
        let k = ((self - &lowest) / &range) * 100.0;
        k
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
        // SAFETY: `Comp<R, f64>` arithmetic operates on `f64` streams; the
        // overloads cannot panic (zero range yields `inf`/`NaN` by design).
        #[allow(clippy::arithmetic_side_effects)]
        let range = &highest - &lowest;
        #[allow(clippy::arithmetic_side_effects)]
        let fast_k = ((self - &lowest) / &range) * 100.0;
        let slow_k = fast_k.sma(d_period);
        let slow_d = slow_k.sma(d_period);
        (slow_k, slow_d)
    }

    fn williams_r_n(&self, n: usize) -> Self {
        let highest = self.max(n);
        let lowest = self.min(n);
        // SAFETY: `Comp<R, f64>` arithmetic operates on `f64` streams; the
        // overloads cannot panic (zero range yields `inf`/`NaN` by design).
        #[allow(clippy::arithmetic_side_effects)]
        let range = &highest - &lowest;
        #[allow(clippy::arithmetic_side_effects)]
        let r = ((&highest - self) / &range) * -100.0;
        r
    }

    fn williams_r_ohlc_n(&self, high: &Self, low: &Self, n: usize) -> Self {
        let highest = high.max(n);
        let lowest = low.min(n);
        // SAFETY: `Comp<R, f64>` arithmetic operates on `f64` streams; the
        // overloads cannot panic (zero range yields `inf`/`NaN` by design).
        #[allow(clippy::arithmetic_side_effects)]
        let range = &highest - &lowest;
        #[allow(clippy::arithmetic_side_effects)]
        let r = ((&highest - self) / &range) * -100.0;
        r
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
        // SAFETY: `Comp<R, f64>` arithmetic operates on `f64` streams; the
        // overloads cannot panic (NaN propagates).
        #[allow(clippy::arithmetic_side_effects)]
        let hl = high - low;
        #[allow(clippy::arithmetic_side_effects)]
        let hpc = (high - &prev_close).abs();
        #[allow(clippy::arithmetic_side_effects)]
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
        // SAFETY: `Comp<R, f64>` arithmetic operates on `f64` streams; the
        // overloads cannot panic (NaN propagates).
        #[allow(clippy::arithmetic_side_effects)]
        let sum = high + low;
        #[allow(clippy::arithmetic_side_effects)]
        let tp = (&sum + self) / 3.0;
        tp
    }

    fn vwap(&self, volume: &Self) -> Self {
        // SAFETY: `Comp<R, f64>` arithmetic operates on `f64` streams; the
        // overloads cannot panic (NaN/inf propagates if `vol_sum == 0`).
        #[allow(clippy::arithmetic_side_effects)]
        let pv = self * volume;
        let pv_sum = pv.cumsum();
        let vol_sum = volume.cumsum();
        #[allow(clippy::arithmetic_side_effects)]
        let result = &pv_sum / &vol_sum;
        result
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
            // SAFETY: `state.count` is bounded by the input stream length
            // observed in a single `tflo` pipeline run; saturating against
            // `usize::MAX` is defensive — at 1 increment per record, reaching
            // saturation would require an effectively unbounded stream that
            // CMO would never produce a meaningful value for. Using
            // `saturating_add` keeps the comparison `state.count <= period`
            // well-defined even at the (unreachable) ceiling.
            state.count = state.count.saturating_add(1);

            if state.count <= period {
                state.sum_gain += gain;
                state.sum_loss += loss;
                if state.count < period {
                    return f64::NAN;
                }
                state.avg_gain = state.sum_gain / period as f64;
                state.avg_loss = state.sum_loss / period as f64;
            } else {
                // SAFETY: the `state.count <= period` branch above means
                // reaching this `else` requires `period >= 1` (otherwise
                // `state.count <= 0` could never hold with `count >= 1`),
                // so `period - 1` cannot underflow. The `if period == 0`
                // guard at the top of the closure also rules it out.
                #[allow(clippy::arithmetic_side_effects)]
                let pm1 = period - 1;
                state.avg_gain = (state.avg_gain * pm1 as f64 + gain) / period as f64;
                state.avg_loss = (state.avg_loss * pm1 as f64 + loss) / period as f64;
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

