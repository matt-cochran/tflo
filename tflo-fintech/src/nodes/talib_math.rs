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
    // SAFETY: `period >= 2` (guarded above ensures `period >= 2`); `2 * period`
    // fits in usize for any realistic indicator period.
    #[allow(clippy::arithmetic_side_effects)]
    let min_n = 2 * period;
    if period < 2 || n < min_n {
        return None;
    }

    // SAFETY: `n >= 2*period >= 4 >= 1`, so `n - 1` cannot underflow.
    #[allow(clippy::arithmetic_side_effects)]
    let end_idx = n - 1;
    // SAFETY: `period >= 2`, so `2*period - 1 >= 3` cannot underflow.
    #[allow(clippy::arithmetic_side_effects)]
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

    // SAFETY: `period >= 2`, so `period - 1 >= 1` cannot underflow.
    #[allow(clippy::arithmetic_side_effects)]
    let warmup = period - 1;
    for _ in 0..warmup {
        // SAFETY: this loop runs `period - 1` times and starts from
        // `today = 0`; combined with the loops below, `today` reaches at
        // most `end_idx < n`, so `today + 1` stays in bounds.
        #[allow(clippy::arithmetic_side_effects)]
        {
            today += 1;
        }
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
        // SAFETY: combined with the loop above, `today` ends at
        // `(period - 1) + period = 2*period - 1 = lookback_total <= end_idx
        // = n - 1`, so `today + 1 <= n - 1` stays in bounds.
        #[allow(clippy::arithmetic_side_effects)]
        {
            today += 1;
        }
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
        // SAFETY: `today < end_idx = n - 1`, so `today + 1 <= n - 1 < n`
        // stays in bounds.
        #[allow(clippy::arithmetic_side_effects)]
        {
            today += 1;
        }
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
                // SAFETY: `period >= 2` (guarded at top of fn), so
                // `period - 1 >= 1` cannot underflow.
                #[allow(clippy::arithmetic_side_effects)]
                let pm1 = period - 1;
                prev_adx = ((prev_adx * pm1 as f64) + dx) / period as f64;
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
    // SAFETY: `period >= 1` (the `period == 0` branch returns above), so
    // `period - 1` cannot underflow; `period - 1 < n` follows from
    // `n > period`.
    #[allow(clippy::arithmetic_side_effects)]
    let seed_idx = period - 1;
    let mut kama = data[seed_idx];
    for i in period..n {
        // SAFETY: `i >= period` from the loop range, so `i - period` cannot
        // underflow and stays in `[0, n - period)` ⊂ `[0, n)`.
        #[allow(clippy::arithmetic_side_effects)]
        let lag_idx = i - period;
        let change = (data[i] - data[lag_idx]).abs();
        let sum_abs = (1..=period)
            .map(|j| {
                // SAFETY: `j` runs in `1..=period`, so `j <= period <= i`;
                // `i - j` cannot underflow (`i >= period >= j`), and
                // `i - j + 1 <= i` stays in bounds (`i < n`).
                #[allow(clippy::arithmetic_side_effects)]
                let hi = i - j + 1;
                #[allow(clippy::arithmetic_side_effects)]
                let lo = i - j;
                (data[hi] - data[lo]).abs()
            })
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
