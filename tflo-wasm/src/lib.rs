#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing, clippy::arithmetic_side_effects))]
#![deny(clippy::print_stdout)] // library code must not write to stdout
// `#[wasm_bindgen]` impl blocks require the explicit struct name in return
// types and field initializers — `Self` confuses the macro's name-resolution
// for the generated JS class binding. Constructors marked
// `#[wasm_bindgen(constructor)]` likewise cannot be `const fn` because the
// macro expansion is not const-eval-compatible.
#![allow(clippy::use_self, clippy::missing_const_for_fn)]
// Numeric streaming-engine intent-allows (see tflo-core for full rationale).
#![allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    clippy::cast_lossless,
    clippy::float_cmp,
    clippy::suboptimal_flops
)]
//! WebAssembly bindings for tflo.
//!
//! This crate provides `#[wasm_bindgen]` exports that bridge between
//! TypeScript/JavaScript callers and the `tflo` Rust crates.
//!
//! # Design
//!
//! - **Self-contained bridge**: All computation logic for the wasm entry
//!   points lives here.  The bridge uses `tflo-ops` and `tflo-fintech`
//!   extension traits directly — it is no longer re-exporting from
//!   `tflo_core::wasm`, which was removed during the tflo-ops split.
//! - **JSON-in/JSON-out**: All functions accept and return JSON strings.
//!   Complex types stay on the Rust side — only serialized data crosses FFI.
//! - **`wasm-pack` compatible**: Built with `wasm-pack build --target web`.
//!
//! # Build
//!
//! ```bash
//! wasm-pack build --target web
//! ```
//!
//! Output goes to `pkg/` by default, containing:
//! - `tflo_wasm_bg.wasm` — compiled wasm binary
//! - `tflo_wasm.js` — JS glue code (init + exports)
//! - `tflo_wasm.d.ts` — TypeScript declarations for intellisense
//! - `package.json` — ready to publish to npm

use wasm_bindgen::prelude::*;

pub mod detectors;
pub use detectors::{
    WasmCrossDetector, WasmGlitchFilter, WasmHysteresisCrossDetector, WasmPulseWidthDetector,
    WasmRuntDetector, WasmWindowDetector,
};

use tflo_ops::primitives::ThresholdCrossEventMode;

use serde::{Deserialize, Serialize};
use tflo_core::iter_ext::TFlowIteratorExt;
use tflo_fintech::composites::FintechIndicators;
use tflo_ops::events::ThresholdCrossEventMode as OpsCrossMode;
use tflo_ops::prelude::*;

// ── Shared bridge types ───────────────────────────────────────────────

/// A single time-series data point.
#[derive(Debug, Clone, Deserialize, Serialize)]
struct Tick {
    /// Timestamp in milliseconds since epoch.
    ts: i64,
    /// The value at this tick.
    value: f64,
}

// ── Internal helpers ──────────────────────────────────────────────────

/// Convert a threshold-cross event mode to a stable string for JS.
pub(crate) const fn cross_mode_str(mode: ThresholdCrossEventMode) -> &'static str {
    match mode {
        ThresholdCrossEventMode::Rising => "rising",
        ThresholdCrossEventMode::Falling => "falling",
        ThresholdCrossEventMode::None => "none",
    }
}

/// Shared JSON error response. Uses `serde_json::json!` so double quotes
/// and backslashes in the underlying error message get escaped properly
/// — naive string interpolation produced malformed JSON when serde
/// errors contained literals like `invalid type: string "not-a-number"`.
fn json_err(context: &str, e: impl std::fmt::Display) -> String {
    serde_json::json!({ "error": format!("{context}: {e}") }).to_string()
}

// ── Initialize ────────────────────────────────────────────────────────

/// Initialize panic hook for better error messages in the browser.
#[wasm_bindgen(start)]
pub fn init() {
    std::panic::set_hook(Box::new(console_error_panic_hook::hook));
}

// ── tflo indicators ───────────────────────────────────────────────────

/// Compute a Simple Moving Average.
///
/// # Arguments
/// * `input_json` — JSON array of `{"ts": i64, "value": f64}`.
/// * `config_json` — JSON object with `"period"` (usize).
///
/// # Returns
/// JSON array of SMA values (null during warmup).
#[wasm_bindgen]
pub fn compute_sma(input_json: &str, config_json: &str) -> String {
    let ticks: Vec<Tick> = match serde_json::from_str(input_json) {
        Ok(t) => t,
        Err(e) => return json_err("invalid input", e),
    };

    #[derive(Deserialize)]
    struct Config {
        period: usize,
    }

    let config: Config = match serde_json::from_str(config_json) {
        Ok(c) => c,
        Err(e) => return json_err("invalid config", e),
    };

    let results: Vec<Option<f64>> = ticks
        .into_iter()
        .tflo(|t| {
            t.timestamp(|x| x.ts);
            let value = t.prop(|x| x.value);
            value.sma(config.period)
        })
        .map(Some)
        .collect();

    serde_json::to_string(&results).unwrap_or_else(|_| "[]".to_string())
}

/// Compute a Relative Strength Index.
///
/// # Arguments
/// * `input_json` — JSON array of `{"ts": i64, "value": f64}`.
/// * `config_json` — JSON object with `"period"` (usize).
///
/// # Returns
/// JSON array of RSI values (0–100).
#[wasm_bindgen]
pub fn compute_rsi(input_json: &str, config_json: &str) -> String {
    let ticks: Vec<Tick> = match serde_json::from_str(input_json) {
        Ok(t) => t,
        Err(e) => return json_err("invalid input", e),
    };

    #[derive(Deserialize)]
    struct Config {
        period: usize,
    }

    let config: Config = match serde_json::from_str(config_json) {
        Ok(c) => c,
        Err(e) => return json_err("invalid config", e),
    };

    let results: Vec<Option<f64>> = ticks
        .into_iter()
        .tflo(|t| {
            t.timestamp(|x| x.ts);
            let value = t.prop(|x| x.value);
            value.rsi(config.period)
        })
        .map(Some)
        .collect();

    serde_json::to_string(&results).unwrap_or_else(|_| "[]".to_string())
}

/// Compute Bollinger Bands.
///
/// # Arguments
/// * `input_json` — JSON array of `{"ts": i64, "value": f64}`.
/// * `config_json` — JSON object with `"period"` (usize) and optional `"multiplier"` (f64).
///
/// # Returns
/// JSON array of `{"middle": f64, "upper": f64, "lower": f64} | null`.
#[wasm_bindgen]
pub fn compute_bollinger(input_json: &str, config_json: &str) -> String {
    let ticks: Vec<Tick> = match serde_json::from_str(input_json) {
        Ok(t) => t,
        Err(e) => return json_err("invalid input", e),
    };

    #[derive(Deserialize)]
    struct Config {
        period: usize,
        #[serde(default = "default_multiplier")]
        multiplier: f64,
    }

    const fn default_multiplier() -> f64 {
        2.0
    }

    let config: Config = match serde_json::from_str(config_json) {
        Ok(c) => c,
        Err(e) => return json_err("invalid config", e),
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
        .collect();

    serde_json::to_string(&results).unwrap_or_else(|_| "[]".to_string())
}

/// Compute an Exponential Moving Average.
///
/// # Arguments
/// * `input_json` — JSON array of `{"ts": i64, "value": f64}`.
/// * `config_json` — JSON object with `"period"` (usize).
///
/// # Returns
/// JSON array of EMA values.
#[wasm_bindgen]
pub fn compute_ema(input_json: &str, config_json: &str) -> String {
    let ticks: Vec<Tick> = match serde_json::from_str(input_json) {
        Ok(t) => t,
        Err(e) => return json_err("invalid input", e),
    };

    #[derive(Deserialize)]
    struct Config {
        period: usize,
    }

    let config: Config = match serde_json::from_str(config_json) {
        Ok(c) => c,
        Err(e) => return json_err("invalid config", e),
    };

    let results: Vec<Option<f64>> = ticks
        .into_iter()
        .tflo(|t| {
            t.timestamp(|x| x.ts);
            let value = t.prop(|x| x.value);
            value.ema(config.period)
        })
        .map(Some)
        .collect();

    serde_json::to_string(&results).unwrap_or_else(|_| "[]".to_string())
}

/// Compute MACD (line, signal, histogram).
///
/// # Arguments
/// * `input_json` — JSON array of `{"ts": i64, "value": f64}`.
/// * `config_json` — JSON object with `"fast"`, `"slow"`, `"signal"` (usize).
///
/// # Returns
/// JSON array of `{"macd": f64, "signal": f64, "histogram": f64}`.
#[wasm_bindgen]
pub fn compute_macd(input_json: &str, config_json: &str) -> String {
    let ticks: Vec<Tick> = match serde_json::from_str(input_json) {
        Ok(t) => t,
        Err(e) => return json_err("invalid input", e),
    };

    #[derive(Deserialize)]
    struct Config {
        fast: usize,
        slow: usize,
        signal: usize,
    }

    #[derive(Serialize)]
    struct MacdRow {
        macd: f64,
        signal: f64,
        histogram: f64,
    }

    let config: Config = match serde_json::from_str(config_json) {
        Ok(c) => c,
        Err(e) => return json_err("invalid config", e),
    };

    let results: Vec<MacdRow> = ticks
        .into_iter()
        .tflo(|t| {
            t.timestamp(|x| x.ts);
            let value = t.prop(|x| x.value);
            value.macd_n(config.fast, config.slow, config.signal)
        })
        .map(|(macd, signal, histogram)| MacdRow {
            macd,
            signal,
            histogram,
        })
        .collect();

    serde_json::to_string(&results).unwrap_or_else(|_| "[]".to_string())
}

/// Detect threshold crossings.
///
/// # Arguments
/// * `input_json` — JSON array of `{"ts": i64, "value": f64}`.
/// * `config_json` — JSON object with `"threshold"` (f64) and optional `"direction"`.
///
/// # Returns
/// JSON array of `{"ts": i64, "value": f64, "direction": string}`.
#[wasm_bindgen]
pub fn detect_cross(input_json: &str, config_json: &str) -> String {
    #[derive(Serialize)]
    struct CrossEvent {
        ts: i64,
        value: f64,
        direction: String,
    }

    let ticks: Vec<Tick> = match serde_json::from_str(input_json) {
        Ok(t) => t,
        Err(e) => return json_err("invalid input", e),
    };

    #[derive(Deserialize)]
    struct Config {
        threshold: f64,
        #[serde(default = "default_direction")]
        direction: String,
    }

    fn default_direction() -> String {
        "both".to_string()
    }

    let config: Config = match serde_json::from_str(config_json) {
        Ok(c) => c,
        Err(e) => return json_err("invalid config", e),
    };

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
            OpsCrossMode::Rising => Some(CrossEvent {
                ts: 0,
                value: val,
                direction: "above".to_string(),
            }),
            OpsCrossMode::Falling => Some(CrossEvent {
                ts: 0,
                value: val,
                direction: "below".to_string(),
            }),
            OpsCrossMode::None => None,
        })
        .collect();

    serde_json::to_string(&events).unwrap_or_else(|_| "[]".to_string())
}

/// Generic indicator computation entry point.
///
/// Dispatches to the correct indicator based on `config.indicator`.
/// Supported: `"sma"`, `"rsi"`, `"bollinger"`, `"cross"`, `"ema"`, `"macd"`.
#[wasm_bindgen]
pub fn compute_indicator(input_json: &str, config_json: &str) -> String {
    #[derive(Deserialize)]
    struct Meta {
        indicator: String,
    }

    let meta: Meta = match serde_json::from_str(config_json) {
        Ok(m) => m,
        Err(e) => return json_err("invalid config", e),
    };

    match meta.indicator.as_str() {
        "sma" => compute_sma(input_json, config_json),
        "rsi" => compute_rsi(input_json, config_json),
        "bollinger" => compute_bollinger(input_json, config_json),
        "cross" => detect_cross(input_json, config_json),
        "ema" => compute_ema(input_json, config_json),
        "macd" => compute_macd(input_json, config_json),
        _ => format!("{{\"error\": \"unknown indicator: {}\"}}", meta.indicator),
    }
}

// ── tflo-cel rule evaluation ──────────────────────────────────────────
//
// `tflo_cel::wasm` is only compiled for wasm32 targets; gate these exports
// accordingly so that `cargo build -p tflo-wasm` (native) stays clean.

/// Evaluate CEL rules (JSON format) against items.
///
/// # Arguments
/// * `rules_json` — JSON string of rule definitions.
/// * `items_json` — JSON array of items with flattened fields.
///
/// # Returns
/// JSON array of `{"item_id": string, "matched_rules": string[]}`.
#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
pub fn evaluate_rules(rules_json: &str, items_json: &str) -> String {
    tflo_cel::wasm::evaluate_rules(rules_json, items_json)
}

/// Evaluate CEL rules (YAML format) against items.
///
/// # Arguments
/// * `rules_yaml` — YAML string of rule definitions.
/// * `items_json` — JSON array of items.
///
/// # Returns
/// JSON array of `{"item_id": string, "matched_rules": string[]}`.
#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
pub fn evaluate_rules_from_yaml(rules_yaml: &str, items_json: &str) -> String {
    tflo_cel::wasm::evaluate_rules_from_yaml(rules_yaml, items_json)
}

// ── Streaming detectors ───────────────────────────────────────────────
//
// Each struct holds the real `tflo_ops::primitives` detector and forwards
// `update`/`reset`. `update` returns a stable event string ("none" when no
// event fires) so no enum needs to cross the FFI boundary.

