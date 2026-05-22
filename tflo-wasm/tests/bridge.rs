//! Integration tests for the tflo-wasm bridge — run with `wasm-pack test --node`.

use tflo_wasm::{
    WasmCrossDetector, WasmGlitchFilter, WasmHysteresisCrossDetector, WasmPulseWidthDetector,
    WasmRuntDetector, WasmWindowDetector, compute_ema, compute_macd,
};
use wasm_bindgen_test::*;

fn ramp_ticks(n: i64) -> String {
    let pts: Vec<String> = (0..n)
        .map(|i| format!("{{\"ts\":{i},\"value\":{}}}", i as f64))
        .collect();
    format!("[{}]", pts.join(","))
}

#[wasm_bindgen_test]
fn ema_returns_one_value_per_tick() {
    let out = compute_ema(&ramp_ticks(50), "{\"period\":10}");
    let parsed: Vec<Option<f64>> = serde_json::from_str(&out).unwrap();
    assert_eq!(parsed.len(), 50);
    assert!(parsed.iter().any(|v| v.is_some()));
}

#[wasm_bindgen_test]
fn macd_returns_three_series_per_tick() {
    let out = compute_macd(&ramp_ticks(80), "{\"fast\":12,\"slow\":26,\"signal\":9}");
    let v: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert_eq!(v.as_array().unwrap().len(), 80);
    let last = v.as_array().unwrap().last().unwrap();
    assert!(last.get("macd").is_some());
    assert!(last.get("signal").is_some());
    assert!(last.get("histogram").is_some());
}

#[wasm_bindgen_test]
fn cross_detector_fires_rising_and_falling() {
    let mut d = WasmCrossDetector::new();
    assert_eq!(d.update(0.0, 50.0), "none");
    assert_eq!(d.update(100.0, 50.0), "rising");
    assert_eq!(d.update(10.0, 50.0), "falling");
    d.reset();
    assert_eq!(d.update(100.0, 50.0), "none");
}

#[wasm_bindgen_test]
fn hysteresis_detector_constructs_and_updates() {
    let mut d = WasmHysteresisCrossDetector::new(5.0);
    let _ = d.update(0.0, 50.0);
    assert_eq!(d.update(100.0, 50.0), "rising");
}

#[wasm_bindgen_test]
fn glitch_filter_classifies_pulses() {
    let mut d = WasmGlitchFilter::new(50.0, 10.0);
    d.update(100.0, 0.0);
    let r = d.update(0.0, 5.0);
    assert_eq!(r, "glitch");
    d.update(100.0, 100.0);
    let r = d.update(0.0, 120.0);
    assert_eq!(r, "valid");
}

#[wasm_bindgen_test]
fn runt_detector_classifies_pulses() {
    let mut d = WasmRuntDetector::new(40.0, 85.0);
    d.update(20.0);
    d.update(60.0);
    assert_eq!(d.update(20.0), "runt");
    d.update(100.0);
    assert_eq!(d.update(20.0), "valid");
}

#[wasm_bindgen_test]
fn pulse_width_detector_classifies_pulses() {
    let mut d = WasmPulseWidthDetector::new(50.0, 8.0, 22.0);
    d.update(100.0, 0.0);
    assert_eq!(d.update(0.0, 4.0), "short");
    d.update(100.0, 100.0);
    assert_eq!(d.update(0.0, 115.0), "valid");
    d.update(100.0, 200.0);
    assert_eq!(d.update(0.0, 240.0), "long");
}

#[wasm_bindgen_test]
fn window_detector_fires_zone_events() {
    let mut d = WasmWindowDetector::new(38.0, 68.0);
    d.update(10.0);
    assert_eq!(d.update(50.0), "entered");
    assert_eq!(d.update(10.0), "exitedLow");
    d.update(50.0);
    assert_eq!(d.update(100.0), "exitedHigh");
}
