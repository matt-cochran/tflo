//! Per-indicator scalar/series helpers used by the `FintechIndicators`
//! trait impl. Moved out of `composites/mod.rs` by `StructureOS` `move`
//! action so the trait surface stays as the public face of the module.
//!
//! These are pure functions over `&[f64]`; they are not part of the
//! `tflo` graph and don't touch any `Comp` types. The bit-exact output
//! of `tflo-fintech`'s indicators is pinned by the golden-fixture suite,
//! so moving these between files is safe — but rewriting their
//! arithmetic is not.

use std::collections::VecDeque;

#[derive(Default)]
pub(super) struct ObvState {
    pub(super) prev_close: Option<f64>,
    pub(super) obv: f64,
}

pub(super) struct MfiState {
    period: usize,
    prev_typical_price: Option<f64>,
    positive_flows: VecDeque<f64>,
    negative_flows: VecDeque<f64>,
    positive_sum: f64,
    negative_sum: f64,
}

impl MfiState {
    pub(super) fn new(period: usize) -> Self {
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

pub(super) fn obv_step(state: &mut ObvState, close: f64, volume: f64) -> f64 {
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

pub(super) fn mfi_step(state: &mut MfiState, typical_price: f64, volume: f64) -> f64 {
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

pub(super) fn ema_series(data: &[f64], period: usize) -> Vec<Option<f64>> {
    let n = data.len();
    let mut out = vec![None; n];
    if n < period || period == 0 {
        return out;
    }
    let alpha = 2.0 / (period as f64 + 1.0);
    let mut val = data[..period].iter().sum::<f64>() / period as f64;
    // SAFETY: `period >= 1` (guarded above), so `period - 1` cannot underflow.
    #[allow(clippy::arithmetic_side_effects)]
    let seed_idx = period - 1;
    out[seed_idx] = Some(val);
    for i in period..n {
        if data[i].is_nan() {
            continue;
        }
        val = alpha * data[i] + (1.0 - alpha) * val;
        out[i] = Some(val);
    }
    out
}

pub(super) fn trima_last(data: &[f64], period: usize) -> f64 {
    let n = data.len();
    if period == 0 || n < period {
        return f64::NAN;
    }

    // SAFETY: `n >= period` (guarded above), so `n - period` cannot underflow.
    #[allow(clippy::arithmetic_side_effects)]
    let window = &data[n - period..n];
    // SAFETY: `period == 0` is rejected by the guard above, so this
    // division cannot panic. The integer-division precision-loss is the
    // intended TRIMA half-window calculation.
    #[allow(clippy::integer_division)]
    let half = period / 2;
    let mut weighted_sum = 0.0;
    let mut weight_sum = 0.0;

    for (i, value) in window.iter().enumerate() {
        // SAFETY: `i < window.len() == period` from enumerate; `i + 1`
        // cannot overflow usize for any realistic `period`, and
        // `period - i` is positive because `i < period`.
        #[allow(clippy::arithmetic_side_effects)]
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

    // FINTECH-002: guard against the (algebraically impossible but
    // defensively required) zero-denominator case. For any `period >= 1`
    // the triangular weights sum to a positive integer, so this should
    // never fire — `debug_assert!` documents that invariant in dev
    // builds while the runtime branch keeps release builds NaN-safe.
    debug_assert!(weight_sum > 0.0, "TRIMA weight_sum invariant violated");
    if weight_sum == 0.0 {
        return f64::NAN;
    }
    weighted_sum / weight_sum
}

pub(super) fn dema_last(data: &[f64], period: usize) -> f64 {
    let n = data.len();
    if period == 0 {
        return f64::NAN;
    }
    // SAFETY: `period >= 1` (guarded immediately above); `2 * period`
    // fits in usize for any realistic indicator period; `2 * period - 1 >= 1`.
    #[allow(clippy::arithmetic_side_effects)]
    let min_n = 2 * period - 1;
    if n < min_n {
        return f64::NAN;
    }

    let ema1 = ema_series(data, period);
    // SAFETY: `n >= 2*period - 1 >= 1`, so `n - 1` cannot underflow.
    #[allow(clippy::arithmetic_side_effects)]
    let last_idx = n - 1;
    let ema1_last = match ema1[last_idx] {
        Some(v) => v,
        None => return f64::NAN,
    };

    // SAFETY: `period >= 1`, so `period - 1` cannot underflow.
    #[allow(clippy::arithmetic_side_effects)]
    let ema1_start = period - 1;
    let ema1_values: Vec<f64> = ema1[ema1_start..]
        .iter()
        .map(|v| v.unwrap_or(f64::NAN))
        .collect();
    let ema2 = ema_series(&ema1_values, period);
    // SAFETY: `n >= 2*period - 1 >= period`, so `n - period` cannot underflow.
    #[allow(clippy::arithmetic_side_effects)]
    let ema2_idx = n - period;
    let ema2_last = match ema2.get(ema2_idx).copied().flatten() {
        Some(v) => v,
        None => return f64::NAN,
    };

    2.0 * ema1_last - ema2_last
}

pub(super) fn tema_last(data: &[f64], period: usize) -> f64 {
    let n = data.len();
    if period == 0 {
        return f64::NAN;
    }
    // SAFETY: `period >= 1` (guarded immediately above);
    // `3 * period - 2 >= 1` for any realistic indicator period.
    #[allow(clippy::arithmetic_side_effects)]
    let min_n = 3 * period - 2;
    if n < min_n {
        return f64::NAN;
    }

    let ema1 = ema_series(data, period);
    // SAFETY: `n >= 3*period - 2 >= 1`, so `n - 1` cannot underflow.
    #[allow(clippy::arithmetic_side_effects)]
    let last_idx = n - 1;
    let ema1_last = match ema1[last_idx] {
        Some(v) => v,
        None => return f64::NAN,
    };

    // SAFETY: `period >= 1`, so `period - 1` cannot underflow.
    #[allow(clippy::arithmetic_side_effects)]
    let ema1_start = period - 1;
    let ema1_values: Vec<f64> = ema1[ema1_start..]
        .iter()
        .map(|v| v.unwrap_or(f64::NAN))
        .collect();
    let ema2 = ema_series(&ema1_values, period);
    // SAFETY: `n >= 3*period - 2 >= period`, so `n - period` cannot underflow.
    #[allow(clippy::arithmetic_side_effects)]
    let ema2_idx = n - period;
    let ema2_last = match ema2.get(ema2_idx).copied().flatten() {
        Some(v) => v,
        None => return f64::NAN,
    };

    // SAFETY: `period >= 1`, so `period - 1` cannot underflow.
    #[allow(clippy::arithmetic_side_effects)]
    let ema2_start = period - 1;
    let ema2_values: Vec<f64> = ema2[ema2_start..]
        .iter()
        .map(|v| v.unwrap_or(f64::NAN))
        .collect();
    let ema3 = ema_series(&ema2_values, period);
    // SAFETY: `n >= 3*period - 2 = period + (2*period - 1)`, so
    // `n - (2 * period - 1)` cannot underflow; `2 * period - 1 >= 1`.
    #[allow(clippy::arithmetic_side_effects)]
    let ema3_idx = n - (2 * period - 1);
    let ema3_last = match ema3.get(ema3_idx).copied().flatten() {
        Some(v) => v,
        None => return f64::NAN,
    };

    3.0 * ema1_last - 3.0 * ema2_last + ema3_last
}

pub(super) fn ppo_last(data: &[f64], fast: usize, slow: usize) -> f64 {
    // PPO is the EMA-based Percentage Price Oscillator:
    //     ((EMA(fast) - EMA(slow)) / EMA(slow)) * 100
    // matching TA-Lib's default `matype=EMA`. An earlier implementation
    // computed a single-window SMA on each side, which is neither the
    // TA-Lib default nor a usable PPO once tolerance is tightened.
    let n = data.len();
    if fast == 0 || slow == 0 || n < fast.max(slow) {
        return f64::NAN;
    }
    let fast_ema = ema_series(data, fast);
    let slow_ema = ema_series(data, slow);
    // SAFETY: `n >= max(fast, slow) >= 1`, so `n - 1` cannot underflow.
    #[allow(clippy::arithmetic_side_effects)]
    let last_idx = n - 1;
    let fast_last = match fast_ema[last_idx] {
        Some(v) => v,
        None => return f64::NAN,
    };
    let slow_last = match slow_ema[last_idx] {
        Some(v) => v,
        None => return f64::NAN,
    };
    if slow_last == 0.0 {
        0.0
    } else {
        (fast_last - slow_last) / slow_last * 100.0
    }
}

pub(super) fn macd_last(
    data: &[f64],
    fast: usize,
    slow: usize,
    signal: usize,
    output: usize,
) -> f64 {
    let n = data.len();
    if n == 0 || slow == 0 || signal == 0 {
        return f64::NAN;
    }
    // SAFETY: `slow >= 1` and `signal >= 1` (guarded immediately above);
    // `slow + signal` fits in usize for any realistic indicator periods;
    // `slow + signal - 1 >= 1`.
    #[allow(clippy::arithmetic_side_effects)]
    let min_n = slow + signal - 1;
    if n < min_n {
        return f64::NAN;
    }
    let fast_ema = ema_series(data, fast);
    let slow_ema = ema_series(data, slow);
    // SAFETY: `slow >= 1` (guarded above), so `slow - 1` cannot underflow.
    #[allow(clippy::arithmetic_side_effects)]
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

pub(super) fn trix_last(data: &[f64], period: usize) -> f64 {
    let n = data.len();
    // NOTE: the `period == 0` branch is checked after the size compare; for
    // `period == 0` the subtraction `0 * 3 - 2` would underflow, so move the
    // zero-period gate first.
    if period == 0 {
        return f64::NAN;
    }
    // SAFETY: `period >= 1` (guarded immediately above), so `period * 3 >= 3`
    // and `period * 3 - 2 >= 1`. The multiplication fits in usize for any
    // realistic indicator period.
    #[allow(clippy::arithmetic_side_effects)]
    let min_n = period * 3 - 2;
    if n < min_n {
        return f64::NAN;
    }
    let ema1 = ema_series(data, period);
    let flat1: Vec<f64> = ema1.iter().map(|v| v.unwrap_or(f64::NAN)).collect();
    // SAFETY: `period >= 1` (guarded above), so `period - 1` cannot underflow.
    #[allow(clippy::arithmetic_side_effects)]
    let ema2_start = period - 1;
    if flat1.len() <= ema2_start {
        return f64::NAN;
    }
    let ema2_vals = ema_series(&flat1[ema2_start..], period);
    let flat2: Vec<f64> = ema2_vals.iter().map(|v| v.unwrap_or(f64::NAN)).collect();
    // SAFETY: `period >= 1` (guarded above), so `period - 1` cannot underflow.
    #[allow(clippy::arithmetic_side_effects)]
    let ema3_start = period - 1;
    if flat2.len() <= ema3_start {
        return f64::NAN;
    }
    let ema3_vals = ema_series(&flat2[ema3_start..], period);
    // SAFETY: `n >= 3*period - 2 = 2*(period - 1) + 1`, so
    // `n - (period - 1) - (period - 1) - 1 >= 0`. All subtractions are
    // well-defined because each intermediate stays non-negative.
    #[allow(clippy::arithmetic_side_effects)]
    let idx = n - (period - 1) - (period - 1) - 1;
    if idx == 0 || idx >= ema3_vals.len() {
        return f64::NAN;
    }
    // SAFETY: `idx >= 1` (the `idx == 0` early-return guarantees this),
    // so `idx - 1` cannot underflow.
    #[allow(clippy::arithmetic_side_effects)]
    let prev_idx = idx - 1;
    match (ema3_vals[prev_idx], ema3_vals[idx]) {
        (Some(prev), Some(cur)) if prev != 0.0 => (cur - prev) / prev * 100.0,
        _ => f64::NAN,
    }
}

pub(super) fn rsi_series(data: &[f64], period: usize) -> Vec<Option<f64>> {
    let n = data.len();
    let mut out = vec![None; n];
    // SAFETY: `period + 1` cannot overflow usize for any realistic indicator
    // period (well under usize::MAX).
    #[allow(clippy::arithmetic_side_effects)]
    let min_n = period + 1;
    if n < min_n || period == 0 {
        return out;
    }
    // SAFETY: `n >= period + 1 >= 2` (combined with `period != 0` guard),
    // so `n - 1 >= 1` cannot underflow.
    #[allow(clippy::arithmetic_side_effects)]
    let cap = n - 1;
    let mut gains = Vec::with_capacity(cap);
    let mut losses = Vec::with_capacity(cap);
    for i in 1..n {
        // SAFETY: `i >= 1` from the `1..n` range, so `i - 1` cannot
        // underflow; `i < n` from the range bound.
        #[allow(clippy::arithmetic_side_effects)]
        let prev = i - 1;
        let d = data[i] - data[prev];
        gains.push(if d > 0.0 { d } else { 0.0 });
        losses.push(if d < 0.0 { -d } else { 0.0 });
    }
    let mut avg_gain = gains[..period].iter().sum::<f64>() / period as f64;
    let mut avg_loss = losses[..period].iter().sum::<f64>() / period as f64;
    out[period] = Some(rsi_value(avg_gain, avg_loss));
    // SAFETY: `period + 1` cannot overflow (same as above).
    #[allow(clippy::arithmetic_side_effects)]
    let start = period + 1;
    // The loop body indexes multiple arrays (`gains`, `losses`, `out`) at
    // related but distinct offsets; converting to `enumerate()` over `out`
    // would obscure that.
    #[allow(clippy::needless_range_loop)]
    for i in start..n {
        // SAFETY: `period >= 1`, so `period - 1` cannot underflow. `i >= 1`
        // because `i >= period + 1 >= 2`, so `i - 1` cannot underflow.
        // `i - 1 < n - 1`, so `gains[i - 1]` is in bounds.
        #[allow(clippy::arithmetic_side_effects)]
        let pm1 = period - 1;
        #[allow(clippy::arithmetic_side_effects)]
        let im1 = i - 1;
        avg_gain = (avg_gain * pm1 as f64 + gains[im1]) / period as f64;
        avg_loss = (avg_loss * pm1 as f64 + losses[im1]) / period as f64;
        out[i] = Some(rsi_value(avg_gain, avg_loss));
    }
    out
}

pub(super) fn rsi_value(avg_gain: f64, avg_loss: f64) -> f64 {
    if avg_loss == 0.0 {
        if avg_gain == 0.0 { 50.0 } else { 100.0 }
    } else {
        100.0 - 100.0 / (1.0 + avg_gain / avg_loss)
    }
}

pub(super) fn stochrsi_last(
    data: &[f64],
    period: usize,
    fastk: usize,
    fastd: usize,
    output: usize,
) -> f64 {
    let rsi_vals = rsi_series(data, period);
    let defined: Vec<f64> = rsi_vals.iter().filter_map(|&v| v).collect();
    let n_def = defined.len();
    if n_def < fastk {
        return f64::NAN;
    }
    let mut fast_k_raw = vec![f64::NAN; n_def];
    // SAFETY: `n_def >= fastk` (guarded above); `rsi_series` returns NaN
    // for `period == 0`, and reaching here implies `fastk >= 1` because
    // `n_def >= fastk` is consistent only when `fastk == 0` and the loop
    // would also be skipped; for `fastk == 0` we would still want
    // `fastk - 1` to short-circuit. We rely on caller `stochrsi_n` always
    // passing `fastk >= 1`.
    #[allow(clippy::arithmetic_side_effects)]
    let start = fastk.saturating_sub(1);
    for i in start..n_def {
        // SAFETY: `i >= fastk - 1`, so `i + 1 >= fastk` and
        // `i + 1 - fastk <= i < n_def`. No overflow possible because
        // `i < n_def` and `fastk >= 1`.
        #[allow(clippy::arithmetic_side_effects)]
        let win_start = i + 1 - fastk;
        let slice = &defined[win_start..=i];
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
    // SAFETY: `fastk + fastd` fits in usize for any realistic indicator
    // periods; `fastk + fastd - 1 >= 1` whenever `fastk >= 1` (which the
    // earlier loop relies on) and `fastd >= 1`.
    #[allow(clippy::arithmetic_side_effects)]
    let min_n = fastk + fastd - 1;
    if n_def < min_n {
        return f64::NAN;
    }
    // SAFETY: `n_def >= fastk + fastd - 1 >= 1`, so `n_def - 1` cannot
    // underflow.
    #[allow(clippy::arithmetic_side_effects)]
    let i = n_def - 1;
    // SAFETY: `i + 1 = n_def >= fastk + fastd - 1 >= fastd`, so
    // `i + 1 - fastd <= i` and indexing is in bounds.
    #[allow(clippy::arithmetic_side_effects)]
    let d_start = i + 1 - fastd;
    fast_k_raw[d_start..=i].iter().sum::<f64>() / fastd as f64
}
