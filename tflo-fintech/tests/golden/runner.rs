//! Golden vector test runner.
//!
//! Runs actual `tflo-core` graph computations via `.tflo()` and `Comp`,
//! then aligns the raw output with golden vector expected values using
//! explicit warmup padding.

use super::vector::GoldenVector;
use tflo_core::prelude::*;
use tflo_fintech::prelude::*;
use tflo_ops::prelude::*;

// ── Input record ─────────────────────────────────────────────────────

/// Runner input record, synthesised from golden vector input series.
#[derive(Clone)]
struct TFloRecord {
    idx: i64,
    close: f64,
    high: f64,
    low: f64,
    volume: f64,
}

// ── Helpers ──────────────────────────────────────────────────────────

fn get_period(params: &serde_json::Value) -> Result<usize, super::GoldenError> {
    params
        .get("period")
        .and_then(serde_json::Value::as_u64)
        .map(|n| n as usize)
        .ok_or_else(|| super::GoldenError::Validation("Missing 'period' parameter".to_string()))
}

fn param(params: &serde_json::Value, name: &str) -> Result<usize, super::GoldenError> {
    params
        .get(name)
        .and_then(serde_json::Value::as_u64)
        .map(|n| n as usize)
        .ok_or_else(|| super::GoldenError::Validation(format!("Missing '{name}' parameter")))
}

/// Build `TFloRecords` from a golden vector's input.
fn build_records(vector: &GoldenVector) -> Vec<TFloRecord> {
    vector
        .input
        .iter()
        .enumerate()
        .map(|(i, &close)| TFloRecord {
            idx: i as i64,
            close,
            high: close * 1.01,
            low: close * 0.99,
            volume: 1.0, // constant volume for volume-dependent indicators
        })
        .collect()
}

// ── Single-output graph execution ────────────────────────────────────

/// Run a single-output `.tflo()` graph and return raw computed values.
///
/// Returns only the values the graph produces after warmup; warmup
/// alignment is done by `align_single`.
fn run_tflo_single<F>(records: &[TFloRecord], build: F) -> Vec<f64>
where
    F: FnOnce(&mut TFlowBuilder<TFloRecord>) -> Comp<TFloRecord, f64>,
{
    records.iter().cloned().tflo(build).collect()
}

/// Run a single-output graph with the standard builder closure pattern.
fn run_tflo_close<F>(records: &[TFloRecord], build_close: F) -> Vec<f64>
where
    F: FnOnce(Comp<TFloRecord, f64>) -> Comp<TFloRecord, f64>,
{
    run_tflo_single(records, |t| {
        t.timestamp(|r: &TFloRecord| r.idx);
        let close = t.prop(|r: &TFloRecord| r.close);
        build_close(close)
    })
}

// ── Multi-output graph execution (tuples) ────────────────────────────

/// Run a two-output `.tflo()` graph and return raw computed tuple values.
fn run_tflo_two<F>(records: &[TFloRecord], build: F) -> Vec<(f64, f64)>
where
    F: FnOnce(&mut TFlowBuilder<TFloRecord>) -> (Comp<TFloRecord, f64>, Comp<TFloRecord, f64>),
{
    records.iter().cloned().tflo(build).collect()
}

/// Run a three-output `.tflo()` graph and return raw computed tuple values.
fn run_tflo_three<F>(records: &[TFloRecord], build: F) -> Vec<(f64, f64, f64)>
where
    F: FnOnce(
        &mut TFlowBuilder<TFloRecord>,
    ) -> (
        Comp<TFloRecord, f64>,
        Comp<TFloRecord, f64>,
        Comp<TFloRecord, f64>,
    ),
{
    records.iter().cloned().tflo(build).collect()
}

// ── Warmup alignment ─────────────────────────────────────────────────

/// Find the index of the first `Some` value in a slice of optional f64s.
fn first_non_null(values: &[Option<f64>]) -> Option<usize> {
    values.iter().position(std::option::Option::is_some)
}

/// Align raw single-output `.tflo()` values with the expected output
/// by padding leading `None`s.
fn align_single(
    expected: &[Option<f64>],
    raw_values: &[f64],
    input_len: usize,
) -> Vec<Option<f64>> {
    let target_start = first_non_null(expected).unwrap_or(0);
    let mut result = vec![None; input_len];

    let trimmed_start = raw_values
        .iter()
        .position(|v| !v.is_nan())
        .unwrap_or(raw_values.len());
    let values = &raw_values[trimmed_start..];
    let slots = input_len.saturating_sub(target_start);
    let skip = values.len().saturating_sub(slots);

    for (i, &val) in values.iter().skip(skip).take(slots).enumerate() {
        result[target_start + i] = Some(val);
    }

    result
}

/// Align raw two-output tuple values.
fn align_two(
    expected: &[Vec<Option<f64>>],
    raw_values: &[(f64, f64)],
    input_len: usize,
) -> Vec<Vec<Option<f64>>> {
    let num_outputs = 2;
    let mut results: Vec<Vec<Option<f64>>> = vec![vec![None; input_len]; num_outputs];

    if raw_values.is_empty() || expected.len() < num_outputs {
        return results;
    }

    let starts: Vec<usize> = (0..num_outputs)
        .map(|o| expected.get(o).and_then(|e| first_non_null(e)).unwrap_or(0))
        .collect();

    for o in 0..num_outputs {
        let first_finite = raw_values
            .iter()
            .position(|&(v0, v1)| if o == 0 { !v0.is_nan() } else { !v1.is_nan() })
            .unwrap_or(raw_values.len());
        let values = &raw_values[first_finite..];
        let slots = input_len.saturating_sub(starts[o]);
        let skip = values.len().saturating_sub(slots);
        for (i, &(v0, v1)) in values.iter().skip(skip).take(slots).enumerate() {
            results[o][starts[o] + i] = Some(if o == 0 { v0 } else { v1 });
        }
    }

    results
}

/// Align raw three-output tuple values.
fn align_three(
    expected: &[Vec<Option<f64>>],
    raw_values: &[(f64, f64, f64)],
    input_len: usize,
) -> Vec<Vec<Option<f64>>> {
    let num_outputs = 3;
    let mut results: Vec<Vec<Option<f64>>> = vec![vec![None; input_len]; num_outputs];

    if raw_values.is_empty() || expected.len() < num_outputs {
        return results;
    }

    let starts: Vec<usize> = (0..num_outputs)
        .map(|o| expected.get(o).and_then(|e| first_non_null(e)).unwrap_or(0))
        .collect();

    for o in 0..num_outputs {
        let first_finite = raw_values
            .iter()
            .position(|&(v0, v1, v2)| {
                let vals = [v0, v1, v2];
                !vals[o].is_nan()
            })
            .unwrap_or(raw_values.len());
        let values = &raw_values[first_finite..];
        let slots = input_len.saturating_sub(starts[o]);
        let skip = values.len().saturating_sub(slots);
        for (i, &(v0, v1, v2)) in values.iter().skip(skip).take(slots).enumerate() {
            let vals = [v0, v1, v2];
            results[o][starts[o] + i] = Some(vals[o]);
        }
    }

    results
}

// ── GoldenRunner ─────────────────────────────────────────────────────

#[derive(Debug)]
pub struct GoldenRunner;

impl GoldenRunner {
    /// Run a single-output golden vector against real `tflo-core` graph execution.
    pub fn run(vector: &GoldenVector) -> Result<Vec<Option<f64>>, super::GoldenError> {
        let records = build_records(vector);
        let input_len = records.len();
        let name = vector.metadata.indicator.as_str();
        let params = &vector.params;

        // Build expected first, so we can use it for alignment
        let expected = vector.expected_output_single()?;

        let raw = match name {
            "sma_tv_count" | "sma_talib_count" => {
                let period = get_period(params)?;
                run_tflo_close(&records, |close| close.sma(period))
            }
            "ema_tv_count" | "ema_talib_count" => {
                let period = get_period(params)?;
                run_tflo_close(&records, |close| close.ema(period))
            }
            "wma_tv_count" | "wma_talib_count" => {
                let period = get_period(params)?;
                run_tflo_close(&records, |close| close.wma(period))
            }
            "rsi_tv_count" | "rsi_talib_count" => {
                let period = get_period(params)?;
                run_tflo_close(&records, |close| close.rsi_wilder_n(period))
            }
            "zscore_tv_count" | "zscore_talib_count" => {
                let period = get_period(params)?;
                run_tflo_close(&records, |close| close.zscore(period))
            }
            "williams_r_tv_count" | "williams_r_talib_count" => {
                let period = get_period(params)?;
                run_tflo_single(&records, |t| {
                    t.timestamp(|r: &TFloRecord| r.idx);
                    let close = t.prop(|r: &TFloRecord| r.close);
                    let high = t.prop(|r: &TFloRecord| r.high);
                    let low = t.prop(|r: &TFloRecord| r.low);
                    close.williams_r_ohlc_n(&high, &low, period)
                })
            }
            "cci_tv_count" | "cci_talib_count" => {
                let period = get_period(params)?;
                run_tflo_single(&records, |t| {
                    t.timestamp(|r: &TFloRecord| r.idx);
                    let close = t.prop(|r: &TFloRecord| r.close);
                    let high = t.prop(|r: &TFloRecord| r.high);
                    let low = t.prop(|r: &TFloRecord| r.low);
                    let tp = close.typical_price(&high, &low);
                    tp.cci_n(period)
                })
            }
            "mom_tv_count" | "mom_talib_count" => {
                let period = get_period(params)?;
                run_tflo_close(&records, |close| close.momentum(period))
            }
            "roc_tv_count" | "roc_talib_count" => {
                let period = get_period(params)?;
                run_tflo_close(&records, |close| close.rate_of_change(period))
            }
            "adx_tv_count" | "adx_talib_count" => {
                let period = get_period(params)?;
                run_tflo_single(&records, |t| {
                    t.timestamp(|r: &TFloRecord| r.idx);
                    let close = t.prop(|r: &TFloRecord| r.close);
                    let high = t.prop(|r: &TFloRecord| r.high);
                    let low = t.prop(|r: &TFloRecord| r.low);
                    close.adx_n(&high, &low, period)
                })
            }
            "atr_talib_count" => {
                let period = get_period(params)?;
                run_tflo_single(&records, |t| {
                    t.timestamp(|r: &TFloRecord| r.idx);
                    let close = t.prop(|r: &TFloRecord| r.close);
                    let high = t.prop(|r: &TFloRecord| r.high);
                    let low = t.prop(|r: &TFloRecord| r.low);
                    close.atr_wilder_n(&high, &low, period)
                })
            }
            "plus_di_talib_count" => {
                let period = get_period(params)?;
                run_tflo_single(&records, |t| {
                    t.timestamp(|r: &TFloRecord| r.idx);
                    let close = t.prop(|r: &TFloRecord| r.close);
                    let high = t.prop(|r: &TFloRecord| r.high);
                    let low = t.prop(|r: &TFloRecord| r.low);
                    close.plus_di_n(&high, &low, period)
                })
            }
            "minus_di_talib_count" => {
                let period = get_period(params)?;
                run_tflo_single(&records, |t| {
                    t.timestamp(|r: &TFloRecord| r.idx);
                    let close = t.prop(|r: &TFloRecord| r.close);
                    let high = t.prop(|r: &TFloRecord| r.high);
                    let low = t.prop(|r: &TFloRecord| r.low);
                    close.minus_di_n(&high, &low, period)
                })
            }
            "obv_talib_count" => run_tflo_single(&records, |t| {
                t.timestamp(|r: &TFloRecord| r.idx);
                let close = t.prop(|r: &TFloRecord| r.close);
                let volume = t.prop(|r: &TFloRecord| r.volume);
                close.obv(&volume)
            }),
            "cmo_talib_count" => {
                let period = get_period(params)?;
                run_tflo_close(&records, |close| close.cmo_n(period))
            }
            "linearreg_slope_talib_count" => {
                let period = get_period(params)?;
                run_tflo_close(&records, |close| close.linearreg_slope_n(period))
            }
            "mfi_talib_count" => {
                let period = get_period(params)?;
                run_tflo_single(&records, |t| {
                    t.timestamp(|r: &TFloRecord| r.idx);
                    let close = t.prop(|r: &TFloRecord| r.close);
                    let high = t.prop(|r: &TFloRecord| r.high);
                    let low = t.prop(|r: &TFloRecord| r.low);
                    let volume = t.prop(|r: &TFloRecord| r.volume);
                    let tp = close.typical_price(&high, &low);
                    tp.mfi_n(&volume, period)
                })
            }
            "ppo_tv_count" | "ppo_talib_count" => {
                let fast = param(params, "fast")?;
                let slow = param(params, "slow")?;
                run_tflo_close(&records, |close| close.ppo_n(fast, slow))
            }
            "trix_tv_count" | "trix_talib_count" => {
                let period = get_period(params)?;
                run_tflo_close(&records, |close| close.trix_n(period))
            }
            "trima_talib_count" => {
                let period = get_period(params)?;
                run_tflo_close(&records, |close| close.trima(period))
            }
            "dema_talib_count" => {
                let period = get_period(params)?;
                run_tflo_close(&records, |close| close.dema_n(period))
            }
            "tema_talib_count" => {
                let period = get_period(params)?;
                run_tflo_close(&records, |close| close.tema_n(period))
            }
            "kama_tv_count" | "kama_talib_count" => {
                let period = get_period(params)?;
                run_tflo_close(&records, |close| close.kama_n(period))
            }
            "macd_tv_count"
            | "macd_talib_count"
            | "bollinger_bands_tv_count"
            | "bollinger_bands_talib_count"
            | "stochastic_tv_count"
            | "stochastic_talib_count"
            | "stochrsi_tv_count"
            | "stochrsi_talib_count" => {
                return Err(super::GoldenError::Validation(format!(
                    "{name} is multi-output, use run_multi_output"
                )));
            }
            _ => return Err(super::GoldenError::UnsupportedIndicator(name.to_string())),
        };

        Ok(align_single(&expected, &raw, input_len))
    }

    /// Run a multi-output golden vector against real `tflo-core` graph execution.
    pub fn run_multi_output(
        vector: &GoldenVector,
    ) -> Result<Vec<Vec<Option<f64>>>, super::GoldenError> {
        let records = build_records(vector);
        let input_len = records.len();
        let name = vector.metadata.indicator.as_str();
        let params = &vector.params;

        // Build expected first, so we can use it for alignment
        let expected = vector.expected_output_multi()?;

        let result: Vec<Vec<Option<f64>>> = match name {
            "macd_tv_count" | "macd_talib_count" => {
                let fast = param(params, "fast")?;
                let slow = param(params, "slow")?;
                let signal = param(params, "signal")?;
                let raw = run_tflo_three(&records, |t| {
                    t.timestamp(|r: &TFloRecord| r.idx);
                    let close = t.prop(|r: &TFloRecord| r.close);
                    close.macd_n(fast, slow, signal)
                });
                align_three(&expected, &raw, input_len)
            }
            "bollinger_bands_tv_count" | "bollinger_bands_talib_count" => {
                let period = get_period(params)?;
                let nbdev = params
                    .get("nbdev")
                    .and_then(serde_json::Value::as_f64)
                    .ok_or_else(|| super::GoldenError::Validation("Missing 'nbdev'".to_string()))?;
                // deviation_band returns (middle, upper, lower)
                // Fixture order: [upper, middle, lower]
                let raw = run_tflo_three(&records, |t| {
                    t.timestamp(|r: &TFloRecord| r.idx);
                    let close = t.prop(|r: &TFloRecord| r.close);
                    close.deviation_band(period, nbdev)
                });
                // Reorder from (middle, upper, lower) to (upper, middle, lower)
                let reordered: Vec<(f64, f64, f64)> = raw
                    .into_iter()
                    .map(|(middle, upper, lower)| (upper, middle, lower))
                    .collect();
                align_three(&expected, &reordered, input_len)
            }
            "stochastic_tv_count" | "stochastic_talib_count" => {
                let kp = param(params, "k_period")?;
                let dp = param(params, "d_period")?;
                let raw = run_tflo_two(&records, |t| {
                    t.timestamp(|r: &TFloRecord| r.idx);
                    let close = t.prop(|r: &TFloRecord| r.close);
                    let high = t.prop(|r: &TFloRecord| r.high);
                    let low = t.prop(|r: &TFloRecord| r.low);
                    close.stochastic_ohlc_n(&high, &low, kp, dp)
                });
                align_two(&expected, &raw, input_len)
            }
            "stochrsi_tv_count" | "stochrsi_talib_count" => {
                let period = get_period(params)?;
                let fastk = param(params, "fastk")?;
                let fastd = param(params, "fastd")?;
                let raw = run_tflo_two(&records, |t| {
                    t.timestamp(|r: &TFloRecord| r.idx);
                    let close = t.prop(|r: &TFloRecord| r.close);
                    close.stochrsi_n(period, fastk, fastd)
                });
                align_two(&expected, &raw, input_len)
            }
            _ => {
                return Err(super::GoldenError::UnsupportedIndicator(name.to_string()));
            }
        };

        Ok(result)
    }
}
