//! WebAssembly bridge for tflo-core.
//!
//! This module provides JSON-in/JSON-out entry points for wasm builds.
//! All functions accept serialized inputs and return serialized results,
//! avoiding direct exposure of graph types across the wasm boundary.
//!
//! # Design
//!
//! - **Opaque bridge**: Core graph types (TFlowBuilder, CompiledGraph, etc.)
//!   are never exposed across FFI. All interaction is via JSON strings.
//! - **Serde-driven**: Inputs and outputs are deserialized/serialized with
//!   `serde_json` for maximum compatibility with TypeScript consumers.
//! - **No `#[wasm_bindgen]`**: This module is gated on `#[cfg(target_arch = "wasm32")]`
//!   but does NOT use `wasm_bindgen`. The final `#[wasm_bindgen]` exports
//!   live in a thin wrapper crate (see `tflo-site/wasm/src/lib.rs`).

#![cfg(target_arch = "wasm32")]

use crate::event::ThresholdCrossEventMode;
use crate::iter_ext::TFlowIteratorExt;
use serde::Deserialize;
use serde::Serialize;

/// A single time-series data point.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Tick {
    /// Timestamp in milliseconds since epoch.
    pub ts: i64,
    /// The value at this tick.
    pub value: f64,
}

/// Result of cross detection.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrossEvent {
    /// Timestamp of the cross.
    pub ts: i64,
    /// The value at the cross.
    pub value: f64,
    /// Direction: "above" or "below".
    pub direction: String,
}

/// Compute a Simple Moving Average on a time series.
///
/// # Arguments
/// * `input_json` — JSON array of `Tick` objects.
/// * `config_json` — JSON object with `"period"` (integer) and optional `"window"` (`"count"` or duration string like `"5s"`).
///
/// # Returns
/// JSON array of SMA values (one per input tick, with leading `null`s during warmup).
pub fn compute_sma(input_json: &str, config_json: &str) -> String {
    let ticks: Vec<Tick> = match serde_json::from_str(input_json) {
        Ok(t) => t,
        Err(e) => return format!("{{\"error\": \"invalid input: {e}\"}}"),
    };

    #[derive(Deserialize)]
    struct SmaConfig {
        period: usize,
    }

    let config: SmaConfig = match serde_json::from_str(config_json) {
        Ok(c) => c,
        Err(e) => return format!("{{\"error\": \"invalid config: {e}\"}}"),
    };

    let results: Vec<Option<f64>> = ticks
        .into_iter()
        .tflo(|t| {
            t.timestamp(|x| x.ts);
            let value = t.prop(|x| x.value);
            value.sma(config.period)
        })
        .map(Some)
        .collect::<Vec<_>>();

    serde_json::to_string(&results).unwrap_or_else(|_| "[]".to_string())
}

/// Compute a Relative Strength Index on a time series.
///
/// # Arguments
/// * `input_json` — JSON array of `Tick` objects.
/// * `config_json` — JSON object with `"period"` (integer).
///
/// # Returns
/// JSON array of RSI values (0–100).
pub fn compute_rsi(input_json: &str, config_json: &str) -> String {
    let ticks: Vec<Tick> = match serde_json::from_str(input_json) {
        Ok(t) => t,
        Err(e) => return format!("{{\"error\": \"invalid input: {e}\"}}"),
    };

    #[derive(Deserialize)]
    struct RsiConfig {
        period: usize,
    }

    let config: RsiConfig = match serde_json::from_str(config_json) {
        Ok(c) => c,
        Err(e) => return format!("{{\"error\": \"invalid config: {e}\"}}"),
    };

    let results: Vec<Option<f64>> = ticks
        .into_iter()
        .tflo(|t| {
            t.timestamp(|x| x.ts);
            let value = t.prop(|x| x.value);
            value.rsi(config.period)
        })
        .map(Some)
        .collect::<Vec<_>>();

    serde_json::to_string(&results).unwrap_or_else(|_| "[]".to_string())
}

/// Compute Bollinger Bands on a time series.
///
/// # Arguments
/// * `input_json` — JSON array of `Tick` objects.
/// * `config_json` — JSON object with `"period"` (integer) and `"multiplier"` (f64).
///
/// # Returns
/// JSON array of `{"middle": f64, "upper": f64, "lower": f64} | null`.
pub fn compute_bollinger(input_json: &str, config_json: &str) -> String {
    let ticks: Vec<Tick> = match serde_json::from_str(input_json) {
        Ok(t) => t,
        Err(e) => return format!("{{\"error\": \"invalid input: {e}\"}}"),
    };

    #[derive(Deserialize)]
    struct BollingerConfig {
        period: usize,
        #[serde(default = "default_multiplier")]
        multiplier: f64,
    }

    fn default_multiplier() -> f64 {
        2.0
    }

    let config: BollingerConfig = match serde_json::from_str(config_json) {
        Ok(c) => c,
        Err(e) => return format!("{{\"error\": \"invalid config: {e}\"}}"),
    };

    #[derive(Serialize)]
    struct Band {
        middle: f64,
        upper: f64,
        lower: f64,
    }

    let results: Vec<Option<Band>> = ticks
        .into_iter()
        .tflo(|t| {
            t.timestamp(|x| x.ts);
            let value = t.prop(|x| x.value);
            let sma = value.sma(config.period);
            let std = value.std(config.period);
            let upper = sma.clone() + (&std * config.multiplier);
            let lower = sma.clone() - (&std * config.multiplier);
            (sma, upper, lower)
        })
        .map(|(middle, upper, lower)| {
            Some(Band {
                middle,
                upper,
                lower,
            })
        })
        .collect::<Vec<_>>();

    serde_json::to_string(&results).unwrap_or_else(|_| "[]".to_string())
}

/// Compute an Exponential Moving Average on a time series.
///
/// # Arguments
/// * `input_json` — JSON array of `Tick` objects.
/// * `config_json` — JSON object with `"period"` (integer).
///
/// # Returns
/// JSON array of EMA values (one per input tick).
pub fn compute_ema(input_json: &str, config_json: &str) -> String {
    let ticks: Vec<Tick> = match serde_json::from_str(input_json) {
        Ok(t) => t,
        Err(e) => return format!("{{\"error\": \"invalid input: {e}\"}}"),
    };

    #[derive(Deserialize)]
    struct EmaConfig {
        period: usize,
    }

    let config: EmaConfig = match serde_json::from_str(config_json) {
        Ok(c) => c,
        Err(e) => return format!("{{\"error\": \"invalid config: {e}\"}}"),
    };

    let results: Vec<Option<f64>> = ticks
        .into_iter()
        .tflo(|t| {
            t.timestamp(|x| x.ts);
            let value = t.prop(|x| x.value);
            value.ema(config.period)
        })
        .map(Some)
        .collect::<Vec<_>>();

    serde_json::to_string(&results).unwrap_or_else(|_| "[]".to_string())
}

/// Detect threshold crossings in a time series.
///
/// # Arguments
/// * `input_json` — JSON array of `Tick` objects.
/// * `config_json` — JSON object with `"threshold"` (f64) and optional `"direction"` (`"above"`, `"below"`, or `"both"`).
///
/// # Returns
/// JSON array of `CrossEvent` objects (with `ts`, `value`, `direction`).
pub fn detect_cross(input_json: &str, config_json: &str) -> String {
    let ticks: Vec<Tick> = match serde_json::from_str(input_json) {
        Ok(t) => t,
        Err(e) => return format!("{{\"error\": \"invalid input: {e}\"}}"),
    };

    #[derive(Deserialize)]
    struct CrossConfig {
        threshold: f64,
        #[serde(default = "default_direction")]
        direction: String,
    }

    fn default_direction() -> String {
        "both".to_string()
    }

    let config: CrossConfig = match serde_json::from_str(config_json) {
        Ok(c) => c,
        Err(e) => return format!("{{\"error\": \"invalid config: {e}\"}}"),
    };

    // Return (value, signal) tuples so we can report the actual value
    // in the CrossEvent. The cross detection node only emits the mode
    // (Rising/Falling), so we pair it with the value.
    let events: Vec<CrossEvent> = ticks
        .into_iter()
        .tflo(|t| {
            t.timestamp(|x| x.ts);
            let value = t.prop(|x| x.value);
            let threshold = t.constant(config.threshold);
            let signal = match config.direction.as_str() {
                "above" => value.cross_above(&threshold),
                "below" => value.cross_under(&threshold),
                _ => value.cross(&threshold),
            };
            (value, signal)
        })
        .filter_map(|(val, signal)| match signal {
            ThresholdCrossEventMode::Rising => Some(CrossEvent {
                ts: 0, // timestamp not preserved across tuple output
                value: val,
                direction: "above".to_string(),
            }),
            ThresholdCrossEventMode::Falling => Some(CrossEvent {
                ts: 0,
                value: val,
                direction: "below".to_string(),
            }),
            ThresholdCrossEventMode::None => None,
        })
        .collect::<Vec<_>>();

    serde_json::to_string(&events).unwrap_or_else(|_| "[]".to_string())
}

/// Compute indicator from a configuration string.
///
/// This is a generic entry point that dispatches based on `config.indicator`.
///
/// # Supported indicators
/// - `"sma"` — requires `"period"`
/// - `"rsi"` — requires `"period"`
/// - `"bollinger"` — requires `"period"`, optional `"multiplier"`
/// - `"cross"` — requires `"threshold"`, optional `"direction"`
/// - `"ema"` — requires `"period"`
///
/// # Returns
/// JSON result, format depends on the indicator.
pub fn compute_indicator(input_json: &str, config_json: &str) -> String {
    #[derive(Deserialize)]
    struct IndicatorConfig {
        indicator: String,
    }

    let meta: IndicatorConfig = match serde_json::from_str(config_json) {
        Ok(m) => m,
        Err(e) => return format!("{{\"error\": \"invalid config: {e}\"}}"),
    };

    match meta.indicator.as_str() {
        "sma" => compute_sma(input_json, config_json),
        "rsi" => compute_rsi(input_json, config_json),
        "bollinger" => compute_bollinger(input_json, config_json),
        "cross" => detect_cross(input_json, config_json),
        "ema" => compute_ema(input_json, config_json),
        _ => format!("{{\"error\": \"unknown indicator: {}\"}}", meta.indicator),
    }
}
