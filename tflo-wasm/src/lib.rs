#![cfg_attr(
    test,
    allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::indexing_slicing,
        clippy::arithmetic_side_effects
    )
)]
#![deny(clippy::print_stdout)]
// library code must not write to stdout
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
            // Bollinger arithmetic on the streaming graph nodes: `+ -` here
            // dispatch to overloaded `Add` / `Sub` impls on `Comp` that
            // compose nodes, not numeric ops. The lint can't see through.
            #[allow(clippy::arithmetic_side_effects)]
            let upper = sma.clone() + (&std * config.multiplier);
            #[allow(clippy::arithmetic_side_effects)]
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

// ── Event-time temporal join ──────────────────────────────────────────
//
// Surfaces `tflo_core::combinators::keyed_window_join` — a keyed, forward
// event-time windowed join — to JS. This is the capability the validation
// suite showed no JS library provides: an order→payment join on a key within
// an event-time window, correct regardless of arrival order.

/// Keyed, forward-windowed event-time join of two streams.
///
/// # Arguments
/// * `left_json` / `right_json` — JSON arrays of objects.
/// * `config_json` — `{ "key": "orderId", "ts": "ts", "window": 3000 }`.
///   `ts` defaults to `"ts"`.
///
/// # Returns
/// JSON array of `{ "left": <obj>, "right": <obj>, "deltaMs": i64 }` for every
/// pair whose `key` fields are equal and whose
/// `right.ts ∈ [left.ts, left.ts + window]`.
#[wasm_bindgen]
pub fn window_join_keyed(left_json: &str, right_json: &str, config_json: &str) -> String {
    use tflo_core::combinators::keyed_window_join;

    #[derive(Deserialize)]
    struct Config {
        key: String,
        #[serde(default = "default_ts_field")]
        ts: String,
        window: i64,
    }
    fn default_ts_field() -> String {
        "ts".to_string()
    }

    let cfg: Config = match serde_json::from_str(config_json) {
        Ok(c) => c,
        Err(e) => return json_err("invalid config", e),
    };
    let left: Vec<serde_json::Value> = match serde_json::from_str(left_json) {
        Ok(v) => v,
        Err(e) => return json_err("invalid left", e),
    };
    let right: Vec<serde_json::Value> = match serde_json::from_str(right_json) {
        Ok(v) => v,
        Err(e) => return json_err("invalid right", e),
    };

    let key_field = cfg.key.clone();
    let ts_field = cfg.ts.clone();
    let key_of = |v: &serde_json::Value| -> String {
        match v.get(&key_field) {
            Some(serde_json::Value::String(s)) => s.clone(),
            Some(other) => other.to_string(),
            None => String::new(),
        }
    };
    let ts_of =
        |v: &serde_json::Value| -> i64 { v.get(&ts_field).and_then(|x| x.as_i64()).unwrap_or(0) };

    let pairs = keyed_window_join(&left, &right, &key_of, &key_of, &ts_of, &ts_of, cfg.window);

    #[derive(Serialize)]
    struct Pair {
        left: serde_json::Value,
        right: serde_json::Value,
        #[serde(rename = "deltaMs")]
        delta_ms: i64,
    }
    let out: Vec<Pair> = pairs
        .into_iter()
        .map(|(left, right)| {
            let delta = ts_of(&right) - ts_of(&left);
            Pair {
                left,
                right,
                delta_ms: delta,
            }
        })
        .collect();
    serde_json::to_string(&out).unwrap_or_else(|_| "[]".to_string())
}

// ── Event-time tumbling window with out-of-order tolerance ────────────
//
// Surfaces the emit-trigger tumbling-window aggregator
// (`tflo_ops::EmitWindowOps::tumbling_sum`) driven through the keyed-graph
// executor (`tflo_core::keyed::KeyedGraphState`) to JS. The capability JS
// libraries lack: event-time bucketing that stays correct when records
// arrive out of order, as long as they fall inside the configured lateness
// bound. Late events land in their *event-time* bucket, not their
// arrival-time bucket.

/// Run a single-key tumbling-sum graph over the given `(value, ts)` events
/// and return every fired window as `(window_end_fire_ts, sum)`.
///
/// `events` may be in any arrival order. Out-of-order events within
/// `allowed_lateness_ms` of the running max are buffered and released in
/// event-time order by [`OutOfOrderPolicy::Buffer`], so they accumulate
/// into the correct bucket. After each event the watermark is advanced to
/// `(max_ts_seen - allowed_lateness_ms)`; at end-of-stream it is advanced
/// past the last bucket edge to flush every open window.
fn drive_tumbling(
    events: &[(f64, i64)],
    window_ms: i64,
    allowed_lateness_ms: i64,
) -> Result<Vec<(i64, f64)>, String> {
    use std::time::Duration;
    use tflo_core::builder::Compile;
    use tflo_core::comp::Comp;
    use tflo_core::compile::CompiledGraph;
    use tflo_core::keyed::{KeyedGraphState, OutOfOrderPolicy};
    use tflo_core::prelude::*;
    use tflo_core::timer::EventTimeMs;
    use tflo_ops::ops::EmitWindowOps;

    // Build a single-key graph: prop(value) → tumbling_sum(window).
    let mut builder: TFlowBuilder<f64> = TFlowBuilder::new();
    builder.timestamp(|_v| 0_i64);
    let value = builder.prop(|v: &f64| *v);
    let comp: Comp<f64, Option<f64>> =
        value.tumbling_sum(Window::tumbling(Duration::from_millis(window_ms.max(0) as u64)));
    let output_ids = comp.output_ids();
    let timestamp_fn = builder
        .get_timestamp_fn()
        .ok_or_else(|| "missing timestamp fn".to_string())?;
    let nodes = builder.into_nodes();
    let graph = CompiledGraph::compile(timestamp_fn, nodes, output_ids);
    let mut state: KeyedGraphState<f64, Option<f64>, &'static str> = KeyedGraphState::new(
        graph,
        OutOfOrderPolicy::Buffer {
            max_lateness_ms: allowed_lateness_ms,
        },
    );

    let mut fired: Vec<(i64, f64)> = Vec::new();
    let mut max_ts = i64::MIN;
    for &(v, ts) in events {
        let items = state
            .step(v, ts, "")
            .map_err(|e| format!("step failed: {e}"))?;
        for item in items {
            if let Some(sum) = item.value {
                fired.push((item.ctx.timestamp(), sum));
            }
        }
        max_ts = max_ts.max(ts);
        // Advance the watermark to release buffered records whose lateness
        // window has closed (and fire any bucket edges they cross).
        let wm = max_ts.saturating_sub(allowed_lateness_ms);
        let items = state
            .advance_event_time_watermark(EventTimeMs::new(wm), "")
            .map_err(|e| format!("watermark advance failed: {e}"))?;
        for item in items {
            if let Some(sum) = item.value {
                fired.push((item.ctx.timestamp(), sum));
            }
        }
    }

    // End-of-stream: flush remaining buffered records, then advance the
    // watermark past the final bucket edge so every open window fires.
    if max_ts != i64::MIN {
        let flushed = state.flush("").map_err(|e| format!("flush failed: {e}"))?;
        for item in flushed {
            if let Some(sum) = item.value {
                fired.push((item.ctx.timestamp(), sum));
            }
        }
        let final_wm = max_ts
            .saturating_add(window_ms)
            .saturating_add(allowed_lateness_ms)
            .saturating_add(1);
        let items = state
            .advance_event_time_watermark(EventTimeMs::new(final_wm), "")
            .map_err(|e| format!("final watermark advance failed: {e}"))?;
        for item in items {
            if let Some(sum) = item.value {
                fired.push((item.ctx.timestamp(), sum));
            }
        }
    }

    Ok(fired)
}

/// Event-time tumbling window with out-of-order tolerance.
///
/// # Arguments
/// * `input_json` — JSON array of `{"ts": i64, "value": f64}` in **any**
///   arrival order.
/// * `config_json` — `{ "windowMs": i64, "allowedLatenessMs": i64 }`
///   (`allowedLatenessMs` defaults to `0`).
///
/// # Returns
/// JSON array of
/// `{ "windowStart": i64, "windowEnd": i64, "count": i64, "sum": f64 }`,
/// ordered by `windowStart`. `windowStart = windowEnd - windowMs`.
///
/// Out-of-order events within `allowedLatenessMs` land in their correct
/// event-time bucket regardless of arrival order.
#[wasm_bindgen]
pub fn tumbling_window(input_json: &str, config_json: &str) -> String {
    let ticks: Vec<Tick> = match serde_json::from_str(input_json) {
        Ok(t) => t,
        Err(e) => return json_err("invalid input", e),
    };

    #[derive(Deserialize)]
    struct Config {
        #[serde(rename = "windowMs")]
        window_ms: i64,
        #[serde(rename = "allowedLatenessMs", default)]
        allowed_lateness_ms: i64,
    }

    let config: Config = match serde_json::from_str(config_json) {
        Ok(c) => c,
        Err(e) => return json_err("invalid config", e),
    };

    if config.window_ms <= 0 {
        return json_err("invalid config", "windowMs must be > 0");
    }

    // Sum graph: drive the real values.
    let sum_events: Vec<(f64, i64)> = ticks.iter().map(|t| (t.value, t.ts)).collect();
    let sum_fired = match drive_tumbling(
        &sum_events,
        config.window_ms,
        config.allowed_lateness_ms,
    ) {
        Ok(f) => f,
        Err(e) => return json_err("tumbling sum failed", e),
    };

    // Count graph: drive value=1.0 per event; the "sum" is the count.
    let count_events: Vec<(f64, i64)> = ticks.iter().map(|t| (1.0_f64, t.ts)).collect();
    let count_fired = match drive_tumbling(
        &count_events,
        config.window_ms,
        config.allowed_lateness_ms,
    ) {
        Ok(f) => f,
        Err(e) => return json_err("tumbling count failed", e),
    };

    // Combine by window-end fire_ts. Each fire_ts is unique per window edge
    // in a single-key tumbling stream, so a map keyed on it is sufficient.
    use std::collections::BTreeMap;
    let mut by_end: BTreeMap<i64, (f64, i64)> = BTreeMap::new();
    for (end, sum) in sum_fired {
        by_end.entry(end).or_insert((0.0, 0)).0 = sum;
    }
    for (end, count) in count_fired {
        by_end.entry(end).or_insert((0.0, 0)).1 = count as i64;
    }

    #[derive(Serialize)]
    struct WindowRow {
        #[serde(rename = "windowStart")]
        window_start: i64,
        #[serde(rename = "windowEnd")]
        window_end: i64,
        count: i64,
        sum: f64,
    }

    // BTreeMap iterates in ascending key (windowEnd) order; since
    // windowStart = windowEnd - windowMs and windowMs is fixed, this is
    // also ascending windowStart order.
    let rows: Vec<WindowRow> = by_end
        .into_iter()
        .map(|(end, (sum, count))| WindowRow {
            window_start: end.saturating_sub(config.window_ms),
            window_end: end,
            count,
            sum,
        })
        .collect();

    serde_json::to_string(&rows).unwrap_or_else(|_| "[]".to_string())
}

// ── tflo-cel rule evaluation ──────────────────────────────────────────
//
// `tflo_cel::wasm` is only compiled for wasm32 targets; gate these exports
// on that AND the `cel` feature so that (a) native `cargo build -p tflo-wasm`
// stays clean and (b) the default math-only `ops` bundle never links the CEL
// interpreter. Build with `--features cel` for the CEL-enabled artifact.

/// Evaluate CEL rules (JSON format) against items.
///
/// # Arguments
/// * `rules_json` — JSON string of rule definitions.
/// * `items_json` — JSON array of items with flattened fields.
///
/// # Returns
/// JSON array of `{"item_id": string, "matched_rules": string[]}`.
#[cfg(all(target_arch = "wasm32", feature = "cel"))]
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
#[cfg(all(target_arch = "wasm32", feature = "cel"))]
#[wasm_bindgen]
pub fn evaluate_rules_from_yaml(rules_yaml: &str, items_json: &str) -> String {
    tflo_cel::wasm::evaluate_rules_from_yaml(rules_yaml, items_json)
}

// ── Streaming detectors ───────────────────────────────────────────────
//
// Each struct holds the real `tflo_ops::primitives` detector and forwards
// `update`/`reset`. `update` returns a stable event string ("none" when no
// event fires) so no enum needs to cross the FFI boundary.

#[cfg(all(test, not(target_arch = "wasm32")))]
mod bridge_native_tests {
    use super::{tumbling_window, window_join_keyed};

    // Validation scenario 2 (event-time temporal join) through the bridge.
    // O1's payment at ts=2000 is within [0,3000] → joins (deltaMs=2000).
    // O2's payment at ts=9000 is outside [1000,4000] → no join.
    #[test]
    fn window_join_keyed_matches_scenario_2() {
        let orders = r#"[{"orderId":"O1","ts":0,"amount":100},{"orderId":"O2","ts":1000,"amount":50}]"#;
        let payments = r#"[{"orderId":"O1","ts":2000},{"orderId":"O2","ts":9000}]"#;
        let config = r#"{"key":"orderId","ts":"ts","window":3000}"#;

        let result = window_join_keyed(orders, payments, config);
        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
        let arr = parsed.as_array().unwrap();

        assert_eq!(arr.len(), 1, "exactly one join expected, got: {result}");
        assert_eq!(arr[0]["left"]["orderId"], "O1");
        assert_eq!(arr[0]["left"]["amount"], 100);
        assert_eq!(arr[0]["right"]["orderId"], "O1");
        assert_eq!(arr[0]["deltaMs"], 2000);
    }

    // Event-time tumbling window with out-of-order tolerance.
    //
    // Arrival order is NOT sorted: ts=1000 ARRIVES AFTER ts=3000, and
    // ts=4000 arrives after ts=5000. With a generous allowedLatenessMs the
    // engine must bucket by EVENT TIME, not arrival time.
    //
    // windowMs=2000 → buckets [0,2000), [2000,4000), [4000,6000).
    // The CORE CLAIM: the late ts=1000 (value 2) lands in [0,2000) with
    // ts=0 (value 1) → count=2, sum=3.
    #[test]
    fn tumbling_window_out_of_order_event_time_bucketing() {
        let input = r#"[{"ts":0,"value":1},{"ts":3000,"value":10},{"ts":1000,"value":2},{"ts":2000,"value":20},{"ts":5000,"value":100},{"ts":4000,"value":40}]"#;
        let config = r#"{"windowMs":2000,"allowedLatenessMs":2000}"#;

        let result = tumbling_window(input, config);
        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
        let arr = parsed.as_array().unwrap_or_else(|| {
            panic!("expected array, got: {result}");
        });

        // Index windows by windowStart for order-independent assertions.
        let mut by_start = std::collections::BTreeMap::new();
        for row in arr {
            let start = row["windowStart"].as_i64().unwrap();
            by_start.insert(start, row.clone());
        }

        // [0,2000): the out-of-order ts=1000 (value 2) must join ts=0
        // (value 1) in its EVENT-TIME bucket despite arriving after ts=3000.
        let w0 = by_start
            .get(&0)
            .unwrap_or_else(|| panic!("[0,2000) window missing; got: {result}"));
        assert_eq!(w0["windowEnd"], 2000);
        assert_eq!(w0["count"], 2, "[0,2000) count; full result: {result}");
        assert_eq!(w0["sum"], 3.0, "[0,2000) sum; full result: {result}");

        // [2000,4000): ts=2000 (20) + ts=3000 (10) → count 2, sum 30.
        let w2 = by_start
            .get(&2000)
            .unwrap_or_else(|| panic!("[2000,4000) window missing; got: {result}"));
        assert_eq!(w2["windowEnd"], 4000);
        assert_eq!(w2["count"], 2, "[2000,4000) count; full result: {result}");
        assert_eq!(w2["sum"], 30.0, "[2000,4000) sum; full result: {result}");

        // [4000,6000): ts=4000 (40) + ts=5000 (100) → count 2, sum 140.
        let w4 = by_start
            .get(&4000)
            .unwrap_or_else(|| panic!("[4000,6000) window missing; got: {result}"));
        assert_eq!(w4["windowEnd"], 6000);
        assert_eq!(w4["count"], 2, "[4000,6000) count; full result: {result}");
        assert_eq!(w4["sum"], 140.0, "[4000,6000) sum; full result: {result}");
    }
}
