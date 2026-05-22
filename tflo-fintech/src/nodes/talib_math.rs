//! TA-Lib-compatible directional-movement and KAMA math.
//!
//! These functions are moved verbatim from `tflo-core`'s former in-engine
//! indicator helpers. They are reproduced unchanged so the golden vectors
//! continue to match the TA-Lib C reference bit-for-bit.

/// Gap-aware true range: `max(high-low, |high-prev_close|, |low-prev_close|)`.
pub(crate) fn true_range(high: f64, low: f64, prev_close: f64) -> f64 {
    (high - low)
        .max((high - prev_close).abs())
        .max((low - prev_close).abs())
}

/// ADX value for the last bar of the series.
pub(crate) fn ta_adx_last(high: &[f64], low: &[f64], close: &[f64], period: usize) -> f64 {
    ta_dmi_last(high, low, close, period)
        .map(|(adx, _, _)| adx)
        .unwrap_or(f64::NAN)
}

/// +DI value for the last bar of the series.
pub(crate) fn ta_plus_di_last(high: &[f64], low: &[f64], close: &[f64], period: usize) -> f64 {
    let n = close.len();
    // Need period+1 bars: `period` for Wilder seed, then 1 more for first DI
    if period < 1 || n <= period {
        return f64::NAN;
    }

    // Wilder-seeded +DM, -DM, and TR accumulators
    let mut prev_high = high[0];
    let mut prev_low = low[0];
    let mut prev_close = close[0];
    let mut prev_plus_dm = 0.0_f64;
    let mut prev_minus_dm = 0.0_f64;
    let mut prev_tr = 0.0_f64;

    for i in 1..period {
        let diff_p = high[i] - prev_high;
        prev_high = high[i];
        let diff_m = prev_low - low[i];
        prev_low = low[i];

        if diff_m > 0.0 && diff_p < diff_m {
            prev_minus_dm += diff_m;
        } else if diff_p > 0.0 && diff_p > diff_m {
            prev_plus_dm += diff_p;
        }

        prev_tr += true_range(prev_high, prev_low, prev_close);
        prev_close = close[i];
    }

    // Now Wilder-smooth for each subsequent bar, tracking +DI
    let mut plus_di = 0.0_f64;
    let period_f = period as f64;
    for i in period..n {
        let diff_p = high[i] - prev_high;
        prev_high = high[i];
        let diff_m = prev_low - low[i];
        prev_low = low[i];

        prev_plus_dm = prev_plus_dm - (prev_plus_dm / period_f);
        prev_minus_dm = prev_minus_dm - (prev_minus_dm / period_f);

        if diff_m > 0.0 && diff_p < diff_m {
            prev_minus_dm += diff_m;
        } else if diff_p > 0.0 && diff_p > diff_m {
            prev_plus_dm += diff_p;
        }

        prev_tr = prev_tr - (prev_tr / period_f) + true_range(prev_high, prev_low, prev_close);
        prev_close = close[i];

        if prev_tr.abs() > 1.0e-14 {
            plus_di = 100.0 * (prev_plus_dm / prev_tr);
        }
    }

    plus_di
}

/// -DI value for the last bar of the series.
pub(crate) fn ta_minus_di_last(high: &[f64], low: &[f64], close: &[f64], period: usize) -> f64 {
    let n = close.len();
    // Need period+1 bars: `period` for Wilder seed, then 1 more for first DI
    if period < 1 || n <= period {
        return f64::NAN;
    }

    let mut prev_high = high[0];
    let mut prev_low = low[0];
    let mut prev_close = close[0];
    let mut prev_plus_dm = 0.0_f64;
    let mut prev_minus_dm = 0.0_f64;
    let mut prev_tr = 0.0_f64;

    for i in 1..period {
        let diff_p = high[i] - prev_high;
        prev_high = high[i];
        let diff_m = prev_low - low[i];
        prev_low = low[i];

        if diff_m > 0.0 && diff_p < diff_m {
            prev_minus_dm += diff_m;
        } else if diff_p > 0.0 && diff_p > diff_m {
            prev_plus_dm += diff_p;
        }

        prev_tr += true_range(prev_high, prev_low, prev_close);
        prev_close = close[i];
    }

    let mut minus_di = 0.0_f64;
    let period_f = period as f64;
    for i in period..n {
        let diff_p = high[i] - prev_high;
        prev_high = high[i];
        let diff_m = prev_low - low[i];
        prev_low = low[i];

        prev_plus_dm = prev_plus_dm - (prev_plus_dm / period_f);
        prev_minus_dm = prev_minus_dm - (prev_minus_dm / period_f);

        if diff_m > 0.0 && diff_p < diff_m {
            prev_minus_dm += diff_m;
        } else if diff_p > 0.0 && diff_p > diff_m {
            prev_plus_dm += diff_p;
        }

        prev_tr = prev_tr - (prev_tr / period_f) + true_range(prev_high, prev_low, prev_close);
        prev_close = close[i];

        if prev_tr.abs() > 1.0e-14 {
            minus_di = 100.0 * (prev_minus_dm / prev_tr);
        }
    }

    minus_di
}

/// Compute `(adx, plus_di, minus_di)` for the last bar.
fn ta_dmi_last(high: &[f64], low: &[f64], close: &[f64], period: usize) -> Option<(f64, f64, f64)> {
    let n = close.len();
    if period < 2 || n < 2 * period {
        return None;
    }

    let end_idx = n - 1;
    let lookback_total = 2 * period - 1;
    if end_idx < lookback_total {
        return None;
    }

    let mut today = 0;
    let mut prev_high = high[today];
    let mut prev_low = low[today];
    let mut prev_close = close[today];
    let mut prev_minus_dm = 0.0_f64;
    let mut prev_plus_dm = 0.0_f64;
    let mut prev_tr = 0.0_f64;

    for _ in 0..(period - 1) {
        today += 1;
        let diff_p = high[today] - prev_high;
        prev_high = high[today];
        let diff_m = prev_low - low[today];
        prev_low = low[today];

        if diff_m > 0.0 && diff_p < diff_m {
            prev_minus_dm += diff_m;
        } else if diff_p > 0.0 && diff_p > diff_m {
            prev_plus_dm += diff_p;
        }

        prev_tr += true_range(prev_high, prev_low, prev_close);
        prev_close = close[today];
    }

    let mut sum_dx = 0.0_f64;
    let mut last_plus_di = 0.0_f64;
    let mut last_minus_di = 0.0_f64;
    for _ in 0..period {
        today += 1;
        let diff_p = high[today] - prev_high;
        prev_high = high[today];
        let diff_m = prev_low - low[today];
        prev_low = low[today];

        let period_f = period as f64;
        prev_minus_dm -= prev_minus_dm / period_f;
        prev_plus_dm -= prev_plus_dm / period_f;

        if diff_m > 0.0 && diff_p < diff_m {
            prev_minus_dm += diff_m;
        } else if diff_p > 0.0 && diff_p > diff_m {
            prev_plus_dm += diff_p;
        }

        prev_tr = prev_tr - (prev_tr / period_f) + true_range(prev_high, prev_low, prev_close);
        prev_close = close[today];

        if prev_tr.abs() > 1.0e-14 {
            let minus_di = 100.0 * (prev_minus_dm / prev_tr);
            let plus_di = 100.0 * (prev_plus_dm / prev_tr);
            last_plus_di = plus_di;
            last_minus_di = minus_di;
            let sum_di = minus_di + plus_di;
            if sum_di.abs() > 1.0e-14 {
                sum_dx += 100.0 * ((minus_di - plus_di).abs() / sum_di);
            }
        }
    }

    let mut prev_adx = sum_dx / period as f64;
    if today == end_idx {
        return Some((prev_adx, last_plus_di, last_minus_di));
    }

    while today < end_idx {
        today += 1;
        let diff_p = high[today] - prev_high;
        prev_high = high[today];
        let diff_m = prev_low - low[today];
        prev_low = low[today];

        let period_f = period as f64;
        prev_minus_dm -= prev_minus_dm / period_f;
        prev_plus_dm -= prev_plus_dm / period_f;

        if diff_m > 0.0 && diff_p < diff_m {
            prev_minus_dm += diff_m;
        } else if diff_p > 0.0 && diff_p > diff_m {
            prev_plus_dm += diff_p;
        }

        prev_tr = prev_tr - (prev_tr / period_f) + true_range(prev_high, prev_low, prev_close);
        prev_close = close[today];

        if prev_tr.abs() > 1.0e-14 {
            let minus_di = 100.0 * (prev_minus_dm / prev_tr);
            let plus_di = 100.0 * (prev_plus_dm / prev_tr);
            last_plus_di = plus_di;
            last_minus_di = minus_di;
            let sum_di = minus_di + plus_di;
            if sum_di.abs() > 1.0e-14 {
                let dx = 100.0 * ((minus_di - plus_di).abs() / sum_di);
                prev_adx = ((prev_adx * (period - 1) as f64) + dx) / period as f64;
            }
        }
    }

    Some((prev_adx, last_plus_di, last_minus_di))
}

/// KAMA value for the last bar of the series.
pub(crate) fn kama_last(data: &[f64], period: usize) -> f64 {
    let n = data.len();
    if n <= period || period == 0 {
        return f64::NAN;
    }
    let fast_alpha = 2.0 / 3.0;
    let slow_alpha = 2.0 / 31.0;
    let mut kama = data[period - 1];
    for i in period..n {
        let change = (data[i] - data[i - period]).abs();
        let sum_abs = (1..=period)
            .map(|j| (data[i - j + 1] - data[i - j]).abs())
            .sum::<f64>();
        let er = if sum_abs == 0.0 {
            0.0
        } else {
            change / sum_abs
        };
        let sc = (er * (fast_alpha - slow_alpha) + slow_alpha).powi(2);
        kama += sc * (data[i] - kama);
    }
    kama
}
