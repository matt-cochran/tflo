//! Streaming detector wrapper classes for the wasm bindgen surface.
//! Moved out of `lib.rs`. The `#[wasm_bindgen]`
//! attribute generates JS classes named after each `Wasm*` struct; the
//! `pub use` re-export in `lib.rs` keeps the crate-root path stable so
//! the generated `tflo_wasm.js` glue doesn't change.

use crate::cross_mode_str;
use tflo_ops::primitives::{
    CrossDetector, GlitchFilter, HysteresisCrossDetector, PulseWidthDetector, PulseWidthResult,
    RuntDetector, RuntResult, WindowDetector, WindowEvent,
};
use wasm_bindgen::prelude::*;

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
