#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]
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

use tflo_ops::primitives::{
    CrossDetector, GlitchFilter, HysteresisCrossDetector, PulseWidthDetector, PulseWidthResult,
    RuntDetector, RuntResult, ThresholdCrossEventMode, WindowDetector, WindowEvent,
};

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
fn cross_mode_str(mode: ThresholdCrossEventMode) -> &'static str {
    match mode {
        ThresholdCrossEventMode::Rising => "rising",
        ThresholdCrossEventMode::Falling => "falling",
        ThresholdCrossEventMode::None => "none",
    }
}

/// Shared JSON error response.
fn json_err(context: &str, e: impl std::fmt::Display) -> String {
    format!("{{\"error\": \"{context}: {e}\"}}")
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

    fn default_multiplier() -> f64 {
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

/// Streaming threshold-cross detector. `update(value, threshold)` returns
/// `"rising"`, `"falling"`, or `"none"`.
#[wasm_bindgen]
pub struct WasmCrossDetector {
    inner: CrossDetector,
}

#[wasm_bindgen]
impl WasmCrossDetector {
    #[wasm_bindgen(constructor)]
    #[allow(clippy::new_without_default)]
    pub fn new() -> WasmCrossDetector {
        WasmCrossDetector {
            inner: CrossDetector::new(),
        }
    }

    pub fn update(&mut self, value: f64, threshold: f64) -> String {
        cross_mode_str(self.inner.update(value, threshold)).to_string()
    }

    pub fn reset(&mut self) {
        self.inner.reset();
    }
}

/// Streaming hysteresis-cross detector. `update(value, threshold)` returns
/// `"rising"`, `"falling"`, or `"none"`.
#[wasm_bindgen]
pub struct WasmHysteresisCrossDetector {
    inner: HysteresisCrossDetector,
}

#[wasm_bindgen]
impl WasmHysteresisCrossDetector {
    #[wasm_bindgen(constructor)]
    pub fn new(hysteresis: f64) -> WasmHysteresisCrossDetector {
        WasmHysteresisCrossDetector {
            inner: HysteresisCrossDetector::new(hysteresis),
        }
    }

    pub fn update(&mut self, value: f64, threshold: f64) -> String {
        cross_mode_str(self.inner.update(value, threshold)).to_string()
    }

    pub fn reset(&mut self) {
        self.inner.reset();
    }
}

/// Streaming glitch filter. `update(value, ts_ms)` returns `"valid"`,
/// `"glitch"`, or `"none"`.
#[wasm_bindgen]
pub struct WasmGlitchFilter {
    inner: GlitchFilter,
}

#[wasm_bindgen]
impl WasmGlitchFilter {
    // `min_duration_ms` / `ts_ms` are `f64`, not `i64`: wasm-bindgen maps a
    // Rust `i64` to a JS `BigInt`, but the JS callers pass plain numbers.
    // They are cast to the library's `i64` here.
    #[wasm_bindgen(constructor)]
    pub fn new(threshold: f64, min_duration_ms: f64) -> WasmGlitchFilter {
        WasmGlitchFilter {
            inner: GlitchFilter::new(threshold, min_duration_ms as i64),
        }
    }

    pub fn update(&mut self, value: f64, ts_ms: f64) -> String {
        match self.inner.update(value, ts_ms as i64) {
            Some(true) => "valid",
            Some(false) => "glitch",
            None => "none",
        }
        .to_string()
    }

    pub fn reset(&mut self) {
        self.inner.reset();
    }
}

/// Streaming runt detector. `update(value)` returns `"valid"`, `"runt"`,
/// or `"none"`.
#[wasm_bindgen]
pub struct WasmRuntDetector {
    inner: RuntDetector,
}

#[wasm_bindgen]
impl WasmRuntDetector {
    #[wasm_bindgen(constructor)]
    pub fn new(low: f64, high: f64) -> WasmRuntDetector {
        WasmRuntDetector {
            inner: RuntDetector::new(low, high),
        }
    }

    pub fn update(&mut self, value: f64) -> String {
        match self.inner.update(value) {
            Some(RuntResult::ValidPulse { .. }) => "valid",
            Some(RuntResult::Runt { .. }) => "runt",
            None => "none",
        }
        .to_string()
    }

    pub fn reset(&mut self) {
        self.inner.reset();
    }
}

/// Streaming pulse-width detector. `update(value, ts_ms)` returns
/// `"short"`, `"valid"`, `"long"`, or `"none"`.
#[wasm_bindgen]
pub struct WasmPulseWidthDetector {
    inner: PulseWidthDetector,
}

#[wasm_bindgen]
impl WasmPulseWidthDetector {
    // `*_ms` params are `f64`, not `i64`: wasm-bindgen maps a Rust `i64` to a
    // JS `BigInt`, but the JS callers pass plain numbers. Cast to `i64` here.
    #[wasm_bindgen(constructor)]
    pub fn new(threshold: f64, min_width_ms: f64, max_width_ms: f64) -> WasmPulseWidthDetector {
        WasmPulseWidthDetector {
            inner: PulseWidthDetector::new(threshold, min_width_ms as i64, max_width_ms as i64),
        }
    }

    pub fn update(&mut self, value: f64, ts_ms: f64) -> String {
        match self.inner.update(value, ts_ms as i64) {
            Some(PulseWidthResult::TooShort { .. }) => "short",
            Some(PulseWidthResult::Valid { .. }) => "valid",
            Some(PulseWidthResult::TooLong { .. }) => "long",
            None => "none",
        }
        .to_string()
    }

    pub fn reset(&mut self) {
        self.inner.reset();
    }
}

/// Streaming window detector. `update(value)` returns `"entered"`,
/// `"exitedLow"`, `"exitedHigh"`, or `"none"`.
#[wasm_bindgen]
pub struct WasmWindowDetector {
    inner: WindowDetector,
}

#[wasm_bindgen]
impl WasmWindowDetector {
    #[wasm_bindgen(constructor)]
    pub fn new(low: f64, high: f64) -> WasmWindowDetector {
        WasmWindowDetector {
            inner: WindowDetector::new(low, high),
        }
    }

    pub fn update(&mut self, value: f64) -> String {
        match self.inner.update(value) {
            Some(WindowEvent::EnteredWindow) => "entered",
            Some(WindowEvent::ExitedLow) => "exitedLow",
            Some(WindowEvent::ExitedHigh) => "exitedHigh",
            None => "none",
        }
        .to_string()
    }

    pub fn reset(&mut self) {
        self.inner.reset();
    }
}
