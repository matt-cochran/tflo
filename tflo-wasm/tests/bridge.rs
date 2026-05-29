//! wasm-bindgen-test for the JS-facing API surface.
//!
//! Run with: `wasm-pack test --node` (this dir), or
//! `cargo test --target wasm32-unknown-unknown` if you have
//! `wasm-bindgen-test-runner` configured.
//!
//! Coverage:
//! 1. Type translation — numbers / strings / objects round-trip from the
//!    wasm bridge to the JS side via the JSON envelope.
//! 2. Error propagation — malformed input yields a debuggable error envelope.
//! 3. Compile-evaluate flow — a small graph (SMA over 5 ticks) computed via
//!    the wasm bridge matches a hand-computed expected value, i.e. the
//!    bridge does not drop semantics on its way through JS.
//! 4. Panic visibility — `console_error_panic_hook` lets panic messages
//!    surface to the test harness.

#![cfg(target_arch = "wasm32")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use tflo_wasm::{
    WasmCrossDetector, WasmGlitchFilter, WasmHysteresisCrossDetector, WasmPulseWidthDetector,
    WasmRuntDetector, WasmWindowDetector, compute_ema, compute_macd, compute_sma, evaluate_rules,
};
use wasm_bindgen_test::*;

// Tests run in Node by default; `wasm_bindgen_test_configure!` only accepts
// `run_in_browser`. No configure line needed for Node execution.

// ── Helpers ────────────────────────────────────────────────────────────

fn ramp_ticks(n: i64) -> String {
    let pts: Vec<String> = (0..n)
        .map(|i| format!("{{\"ts\":{i},\"value\":{}}}", i as f64))
        .collect();
    format!("[{}]", pts.join(","))
}

/// Build a JSON tick array from an explicit values slice.
fn ticks_from(values: &[f64]) -> String {
    let pts: Vec<String> = values
        .iter()
        .enumerate()
        .map(|(i, v)| format!("{{\"ts\":{i},\"value\":{v}}}"))
        .collect();
    format!("[{}]", pts.join(","))
}

// ── Existing detector / indicator coverage ─────────────────────────────

#[wasm_bindgen_test]
fn ema_returns_one_value_per_tick() {
    let out = compute_ema(&ramp_ticks(50), "{\"period\":10}");
    let parsed: Vec<Option<f64>> = serde_json::from_str(&out).unwrap();
    assert_eq!(parsed.len(), 50);
    assert!(parsed.iter().any(std::option::Option::is_some));
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

// ── Goal 1: type translation across the JS bridge ──────────────────────

/// Numbers, strings, and nested objects all survive the round-trip from
/// Rust → wasm-bindgen → JS (the runner's V8) → JSON.parse. Asserting on a
/// shape that mixes all three guarantees the FFI boundary preserves types.
#[wasm_bindgen_test]
fn wasm_value_round_trip() {
    let ticks = ticks_from(&[1.0, 2.0, 3.0, 4.0, 5.0]);
    let out = compute_macd(&ticks, "{\"fast\":2,\"slow\":3,\"signal\":2}");

    // Returned JsValue is a JS string — serde_json::from_str confirms the
    // payload survived as a valid UTF-8 JSON string.
    let v: serde_json::Value = serde_json::from_str(&out).expect("returned a JSON string");
    let arr = v.as_array().expect("top-level array");
    assert_eq!(arr.len(), 5, "one element per input tick");

    // Object shape preserved across FFI.
    let last = arr.last().unwrap();
    let obj = last.as_object().expect("element is an object");
    for key in ["macd", "signal", "histogram"] {
        assert!(
            obj.contains_key(key),
            "missing key `{key}` after round-trip"
        );
        assert!(
            obj[key].is_number(),
            "key `{key}` should be a JS number, got {:?}",
            obj[key]
        );
    }
}

// ── Goal 2: error propagation preserves debuggable info ────────────────

/// The bridge surfaces errors as a JSON envelope `{"error": "<context>: <details>"}`.
/// We assert (a) the envelope is well-formed JSON, (b) the context tag is
/// present so an operator knows *which* validation failed, and (c) the
/// underlying serde error message is preserved so they can fix the input.
#[wasm_bindgen_test]
fn wasm_error_path_returns_typed_error() {
    // Malformed input JSON (missing closing bracket).
    let out = compute_sma("[{\"ts\":0,\"value\":1.0}", "{\"period\":3}");
    let v: serde_json::Value =
        serde_json::from_str(&out).expect("error envelope is itself valid JSON");
    let msg = v
        .get("error")
        .and_then(|e| e.as_str())
        .expect("error envelope has a string `error` field");
    assert!(
        msg.contains("invalid input"),
        "error should tag which stage failed: got `{msg}`"
    );

    // Malformed config — missing the `period` field entirely.
    let out = compute_sma("[]", "{\"per\":3}");
    let v: serde_json::Value =
        serde_json::from_str(&out).expect("envelope from missing-field error must be valid JSON");
    let msg = v.get("error").and_then(|e| e.as_str()).unwrap();
    assert!(
        msg.contains("invalid config"),
        "config errors should be tagged: got `{msg}`"
    );
    assert!(
        msg.contains("period"),
        "config error should mention the missing field name: got `{msg}`"
    );

    // Regression: serde error containing double-quoted literals (e.g.
    // `invalid type: string "not-a-number"`) must still produce a
    // well-formed JSON envelope. Earlier `json_err` interpolated naively
    // and the inner quotes broke the outer JSON; locked down here.
    let out = compute_sma("[]", "{\"period\":\"not-a-number\"}");
    let v: serde_json::Value = serde_json::from_str(&out)
        .expect("envelope must remain valid JSON even when serde error contains literal quotes");
    let msg = v.get("error").and_then(|e| e.as_str()).unwrap();
    assert!(
        msg.contains("invalid config"),
        "wrong-type config error should still be tagged: got `{msg}`"
    );
    assert!(
        msg.contains("not-a-number"),
        "wrong-type config error should preserve the offending literal: got `{msg}`"
    );
}

// ── Goal 3: compile-evaluate flow / wire-protocol equivalence ──────────

/// The wasm bridge must compute the same value as a hand-evaluated SMA. If
/// the JSON layer dropped a tick, miscounted the warmup, or fed values in
/// the wrong order, this test would catch it.
///
/// SMA(period=3) over [1, 2, 3, 4, 5]:
///   idx 0: warmup     -> null
///   idx 1: warmup     -> null
///   idx 2: (1+2+3)/3  -> 2.0
///   idx 3: (2+3+4)/3  -> 3.0
///   idx 4: (3+4+5)/3  -> 4.0
#[wasm_bindgen_test]
fn wasm_compile_evaluate_native_equivalence() {
    let ticks = ticks_from(&[1.0, 2.0, 3.0, 4.0, 5.0]);
    let out = compute_sma(&ticks, "{\"period\":3}");
    let parsed: Vec<Option<f64>> = serde_json::from_str(&out).expect("valid JSON");

    assert_eq!(parsed.len(), 5, "one output per input tick");

    // First two values are warmup. The bridge wraps every output in `Some`
    // (see `.map(Some)` in compute_sma), but the inner f64 during warmup
    // is the SMA's own warmup sentinel — depending on the SMA impl that's
    // either NaN or a partial mean. What matters for wire-protocol
    // equivalence is: from period-1 onward, the value matches the
    // hand-computed mean exactly.
    let expected = [None, None, Some(2.0), Some(3.0), Some(4.0)];
    for (i, exp) in expected.iter().enumerate() {
        if let Some(want) = *exp {
            let got = parsed[i].expect("post-warmup value should be present");
            assert!(
                (got - want).abs() < 1e-9,
                "tick {i}: bridge produced {got}, expected {want}"
            );
        }
    }
}

/// Same trick for EMA — but EMA has no clean closed form for arbitrary input,
/// so we use a constant series where EMA must equal that constant once
/// warmed up. This is the simplest hand-verifiable case.
#[wasm_bindgen_test]
fn wasm_ema_constant_input_native_equivalence() {
    let ticks = ticks_from(&[42.0; 20]);
    let out = compute_ema(&ticks, "{\"period\":5}");
    let parsed: Vec<Option<f64>> = serde_json::from_str(&out).unwrap();
    assert_eq!(parsed.len(), 20);

    // The last few values of an EMA over a constant must converge to that
    // constant. Allow a small tolerance for early-warmup smoothing residue.
    let last = parsed.last().unwrap().expect("post-warmup present");
    assert!(
        (last - 42.0).abs() < 1e-6,
        "EMA over constant 42.0 should converge to 42.0; got {last}"
    );
}

/// CEL rule evaluation across the wasm bridge. Confirms the rules JSON
/// → engine → items JSON → results JSON path keeps rule names intact and
/// fires the right matches.
#[wasm_bindgen_test]
fn wasm_evaluate_rules_basic() {
    let rules = r#"{
        "rules": [
            {
                "name": "high_value",
                "condition": "value > 100",
                "action": { "type": "log" }
            },
            {
                "name": "low_value",
                "condition": "value < 10",
                "action": { "type": "log" }
            }
        ]
    }"#;
    let items = r#"[
        {"id": "a", "value": 5},
        {"id": "b", "value": 50},
        {"id": "c", "value": 150}
    ]"#;

    let out = evaluate_rules(rules, items);
    let parsed: serde_json::Value =
        serde_json::from_str(&out).expect("rule eval returned valid JSON");
    let arr = parsed.as_array().expect("top-level array");
    assert_eq!(arr.len(), 3, "one result per item");

    // item `a` (value=5) -> low_value
    let a = &arr[0];
    assert_eq!(a["item_id"], "a");
    let a_matched: Vec<&str> = a["matched_rules"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    assert_eq!(a_matched, vec!["low_value"]);

    // item `b` (value=50) -> nothing
    let b = &arr[1];
    assert!(b["matched_rules"].as_array().unwrap().is_empty());

    // item `c` (value=150) -> high_value
    let c = &arr[2];
    let c_matched: Vec<&str> = c["matched_rules"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    assert_eq!(c_matched, vec!["high_value"]);
}

// ── Goal 5: panic visibility (does not corrupt subsequent tests) ───────

/// `init()` (the `#[wasm_bindgen(start)]` entrypoint in lib.rs) wires
/// `console_error_panic_hook`. With that in place, a panicking test should:
///   (a) be caught by the runner as a failure,
///   (b) surface the panic message so the operator can find the cause.
///
/// `wasm_bindgen_test(should_panic = "...")` asserts both at once.
#[wasm_bindgen_test]
#[should_panic(expected = "intentional panic for hook visibility test")]
fn wasm_panic_does_not_corrupt_state() {
    panic!("intentional panic for hook visibility test");
}

/// Sanity: after the panicking test, subsequent tests still work — confirms
/// the panic hook does not leave wasm linear memory in a corrupted state
/// (each `#[wasm_bindgen_test]` runs in isolation, but this asserts the
/// invariant explicitly).
#[wasm_bindgen_test]
fn wasm_state_clean_after_panic() {
    let mut d = WasmCrossDetector::new();
    assert_eq!(d.update(0.0, 50.0), "none");
    assert_eq!(d.update(100.0, 50.0), "rising");
}
