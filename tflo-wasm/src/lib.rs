//! WebAssembly bindings for tflo.
//!
//! This crate provides `#[wasm_bindgen]` exports that delegate to the
//! `tflo_core::wasm` and `tflo_cel::wasm` bridge modules.
//!
//! # Design
//!
//! - **Thin wrapper**: Only `#[wasm_bindgen]` boilerplate lives here.
//!   All computation logic lives in the respective bridge modules.
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

use tflo_core::primitives::{
    CrossDetector, GlitchFilter, HysteresisCrossDetector, PulseWidthDetector,
    PulseWidthResult, RuntDetector, RuntResult, ThresholdCrossEventMode,
    WindowDetector, WindowEvent,
};

/// Convert a threshold-cross event mode to a stable string for JS.
fn cross_mode_str(mode: ThresholdCrossEventMode) -> &'static str {
    match mode {
        ThresholdCrossEventMode::Rising => "rising",
        ThresholdCrossEventMode::Falling => "falling",
        ThresholdCrossEventMode::None => "none",
    }
}

/// Initialize panic hook for better error messages in the browser.
#[wasm_bindgen(start)]
pub fn init() {
    std::panic::set_hook(Box::new(console_error_panic_hook::hook));
}

// ── tflo-core indicators ──────────────────────────────────────────────

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
    tflo_core::wasm::compute_sma(input_json, config_json)
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
    tflo_core::wasm::compute_rsi(input_json, config_json)
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
    tflo_core::wasm::compute_bollinger(input_json, config_json)
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
    tflo_core::wasm::compute_ema(input_json, config_json)
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
    tflo_core::wasm::compute_macd(input_json, config_json)
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
    tflo_core::wasm::detect_cross(input_json, config_json)
}

/// Generic indicator computation entry point.
///
/// Dispatches to the correct indicator based on `config.indicator`.
#[wasm_bindgen]
pub fn compute_indicator(input_json: &str, config_json: &str) -> String {
    tflo_core::wasm::compute_indicator(input_json, config_json)
}

// ── tflo-cel rule evaluation ──────────────────────────────────────────

/// Evaluate CEL rules (JSON format) against items.
///
/// # Arguments
/// * `rules_json` — JSON string of rule definitions.
/// * `items_json` — JSON array of items with flattened fields.
///
/// # Returns
/// JSON array of `{"item_id": string, "matched_rules": string[]}`.
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
#[wasm_bindgen]
pub fn evaluate_rules_from_yaml(rules_yaml: &str, items_json: &str) -> String {
    tflo_cel::wasm::evaluate_rules_from_yaml(rules_yaml, items_json)
}

// ── Streaming detectors ───────────────────────────────────────────────
//
// Each struct holds the real `tflo_core::primitives` detector and forwards
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
    pub fn new(
        threshold: f64,
        min_width_ms: f64,
        max_width_ms: f64,
    ) -> WasmPulseWidthDetector {
        WasmPulseWidthDetector {
            inner: PulseWidthDetector::new(
                threshold,
                min_width_ms as i64,
                max_width_ms as i64,
            ),
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
