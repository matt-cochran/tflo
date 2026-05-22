# Wasm-Powered Live Demos Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make every `<DemoChart>` demo compute live in-browser via wasm compiled from `tflo-core`, and delete every duplicate JavaScript reimplementation of an indicator or detector.

**Architecture:** Add thin wasm bridges to `tflo-core::wasm` (two batch functions: `compute_ema`, `compute_macd`) and to `tflo-wasm` (six `#[wasm_bindgen]` streaming detector structs), each delegating to the real `tflo-core` library — no algorithm is reimplemented. The site generates jittered feeds client-side and computes results live via wasm: indicators in one batch call, detectors fed tick-by-tick into the streaming structs. Each demo is a `{ feed, compute, jitter }` **descriptor** consumed by a single `computeDemo` function — the same seam the follow-on central demo drives from UI controls. `generate-demo-data.mjs`, the static `src/data/demos/*.json`, and all JS calculators are deleted.

This plan is the **foundation**; the playground extension and the central "explorer" demo are separate follow-on plans (see *Follow-on plans* at the end).

**Tech Stack:** Rust + `wasm-bindgen` + `wasm-pack` 0.15; TypeScript / React 19 / Astro 5; recharts 3; vitest 4.

---

## Background facts (verified)

- `tflo-wasm` (crate-type `cdylib`) is a thin `#[wasm_bindgen]` wrapper over `tflo-core::wasm` + `tflo-cel::wasm`. `tflo-core/src/wasm.rs` is `#[cfg(target_arch = "wasm32")]`, JSON-in/JSON-out, and calls the real graph engine (`.tflo()`, `.sma()`, …).
- Currently exported: `compute_sma`, `compute_rsi`, `compute_bollinger`, `detect_cross`, `compute_indicator`, `evaluate_rules`, `evaluate_rules_from_yaml`. **Missing: EMA, MACD, and 5 of 6 detectors.**
- Real library APIs (confirmed):
  - `value.ema(period)` — `tflo-core/src/comp/windowed.rs:29`. `usize` coerces via `impl Into<Window>`.
  - `value.macd_n(fast, slow, signal) -> (Comp, Comp, Comp)` = (macd line, signal line, histogram) — `tflo-core/src/comp/indicators.rs:40`.
  - `CrossDetector::new()`, `update(value, threshold) -> ThresholdCrossEventMode`, `reset()`.
  - `HysteresisCrossDetector::new(hysteresis)`, `update(value, threshold) -> ThresholdCrossEventMode`, `reset()`.
  - `GlitchFilter::new(threshold, min_duration_ms: i64)`, `update(value, ts_ms: i64) -> Option<bool>`, `reset()`.
  - `RuntDetector::new(low, high)`, `update(value) -> Option<RuntResult>`, `reset()`.
  - `PulseWidthDetector::new(threshold, min_width_ms, max_width_ms)`, `update(value, ts_ms) -> Option<PulseWidthResult>`, `reset()`.
  - `WindowDetector::new(low, high)`, `update(value) -> Option<WindowEvent>`, `reset()`.
  - All re-exported from `tflo_core::primitives::*`. Enums: `ThresholdCrossEventMode {Rising,Falling,None}`, `RuntResult {Runt{peak},ValidPulse{peak}}`, `PulseWidthResult {TooShort{width_ms},Valid{width_ms},TooLong{width_ms}}`, `WindowEvent {EnteredWindow,ExitedLow,ExitedHigh}`. **None derive serde** — wrappers convert to strings manually.
- `wasm-pack` 0.15.0 is installed. `npm run build:wasm` runs `wasm-pack build ../tflo-wasm --target web --out-dir ../public/wasm --out-name tflo`. `public/wasm/.gitignore` is `*` (artifacts not committed; built locally / in CI).
- `wasm.ts` lazy-loads `/wasm/tflo.js`, exposes `initWasm()` + typed wrappers.
- `feeds.ts` has `sineWave`, `stepFunction`, `noisyTrend`, `sawtooth`, `gapInjector`, `spikeInjector` — all using unseeded `Math.random()`. No `pulseTrain` / `hoveringSignal`.

---

## Files map

**Create:**
- `tflo-wasm/tests/bridge.rs` — `wasm-bindgen-test` integration tests.
- `tflo-site/src/lib/demo-config.ts` — per-demo descriptors (feed + indicator/detector params).
- `tflo-site/src/lib/demo-compute.ts` — feed generation + wasm compute orchestration.
- `tflo-site/src/lib/__tests__/feeds.spec.ts` — feed generator unit tests.
- `tflo-site/src/lib/__tests__/demo-config.spec.ts` — demo-config sanity tests.

**Modify:**
- `tflo-core/src/wasm.rs` — add `compute_ema`, `compute_macd`; extend `compute_indicator`.
- `tflo-wasm/src/lib.rs` — add `#[wasm_bindgen]` `compute_ema`, `compute_macd` + 6 detector structs.
- `tflo-wasm/Cargo.toml` — add `wasm-bindgen-test` dev-dependency.
- `tflo-site/src/lib/wasm.ts` — add EMA/MACD wrappers + 6 detector classes; extend `WasmModule`.
- `tflo-site/src/lib/feeds.ts` — seeded RNG, `jitter`, `pulseTrain`, `hoveringSignal`.
- `tflo-site/src/components/DemoChart.tsx` — generate jittered feed + compute live via wasm.
- `tflo-site/src/components/sub/GlitchChart.tsx`, `RuntChart.tsx`, `PulseWidthChart.tsx` — derive pulse spans / peak from `value` + `config` instead of from result fields.
- `tflo-site/package.json` — remove the `generate:demos` script.

**Delete:**
- `tflo-site/scripts/generate-demo-data.mjs`
- `tflo-site/src/data/demos/` (all `*.json`)
- `tflo-site/src/lib/__tests__/demo-data.spec.ts`
- `tflo-site/src/lib/__tests__/demo-cross-conversion.spec.ts`
- `tflo-site/src/lib/__tests__/demo-macd-conversion.spec.ts`
- `tflo-site/src/lib/__tests__/demo-detectors.spec.ts`

---

## Phase 1 — Rust wasm bridge

### Task 1: EMA + MACD batch bridge functions

**Files:**
- Modify: `tflo-core/src/wasm.rs` (append after `compute_bollinger`, before `detect_cross`; extend `compute_indicator`)
- Modify: `tflo-wasm/src/lib.rs` (add two `#[wasm_bindgen]` exports after `compute_bollinger`)

- [ ] **Step 1: Add `compute_ema` to `tflo-core/src/wasm.rs`**

Insert after the end of `compute_bollinger` (after its closing `}` near line 176):

```rust
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

/// Compute MACD (line, signal, histogram) on a time series.
///
/// # Arguments
/// * `input_json` — JSON array of `Tick` objects.
/// * `config_json` — JSON object with `"fast"`, `"slow"`, `"signal"` (integers).
///
/// # Returns
/// JSON array of `{"macd": f64, "signal": f64, "histogram": f64}`.
pub fn compute_macd(input_json: &str, config_json: &str) -> String {
    let ticks: Vec<Tick> = match serde_json::from_str(input_json) {
        Ok(t) => t,
        Err(e) => return format!("{{\"error\": \"invalid input: {e}\"}}"),
    };

    #[derive(Deserialize)]
    struct MacdConfig {
        fast: usize,
        slow: usize,
        signal: usize,
    }

    let config: MacdConfig = match serde_json::from_str(config_json) {
        Ok(c) => c,
        Err(e) => return format!("{{\"error\": \"invalid config: {e}\"}}"),
    };

    #[derive(Serialize)]
    struct MacdPoint {
        macd: f64,
        signal: f64,
        histogram: f64,
    }

    let results: Vec<Option<MacdPoint>> = ticks
        .into_iter()
        .tflo(|t| {
            t.timestamp(|x| x.ts);
            let value = t.prop(|x| x.value);
            value.macd_n(config.fast, config.slow, config.signal)
        })
        .map(|(macd, signal, histogram)| {
            Some(MacdPoint {
                macd,
                signal,
                histogram,
            })
        })
        .collect::<Vec<_>>();

    serde_json::to_string(&results).unwrap_or_else(|_| "[]".to_string())
}
```

- [ ] **Step 2: Extend `compute_indicator` dispatch in `tflo-core/src/wasm.rs`**

In the `match meta.indicator.as_str()` block, add two arms before the `_ =>` arm:

```rust
        "ema" => compute_ema(input_json, config_json),
        "macd" => compute_macd(input_json, config_json),
```

- [ ] **Step 3: Add `#[wasm_bindgen]` exports in `tflo-wasm/src/lib.rs`**

Insert after the `compute_bollinger` export (after line 73):

```rust
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
```

- [ ] **Step 4: Verify the crate compiles for wasm**

Run: `cd /home/mc/working/tflo && cargo build -p tflo-wasm --target wasm32-unknown-unknown`
Expected: compiles with no errors. (If `wasm32-unknown-unknown` is missing: `rustup target add wasm32-unknown-unknown`.)

- [ ] **Step 5: Commit**

```bash
cd /home/mc/working/tflo
git add tflo-core/src/wasm.rs tflo-wasm/src/lib.rs
git commit -m "feat(wasm): add compute_ema and compute_macd bridge functions"
```

---

### Task 2: Six streaming detector wasm structs

**Files:**
- Modify: `tflo-wasm/src/lib.rs` (append after the `compute_macd` export)

- [ ] **Step 1: Add the detector imports and a helper at the top of `tflo-wasm/src/lib.rs`**

After `use wasm_bindgen::prelude::*;` (line 26), add:

```rust
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
```

- [ ] **Step 2: Append the six detector structs to `tflo-wasm/src/lib.rs`**

Add at the end of the file:

```rust
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
    #[wasm_bindgen(constructor)]
    pub fn new(threshold: f64, min_duration_ms: i64) -> WasmGlitchFilter {
        WasmGlitchFilter {
            inner: GlitchFilter::new(threshold, min_duration_ms),
        }
    }

    pub fn update(&mut self, value: f64, ts_ms: i64) -> String {
        match self.inner.update(value, ts_ms) {
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
    #[wasm_bindgen(constructor)]
    pub fn new(
        threshold: f64,
        min_width_ms: i64,
        max_width_ms: i64,
    ) -> WasmPulseWidthDetector {
        WasmPulseWidthDetector {
            inner: PulseWidthDetector::new(threshold, min_width_ms, max_width_ms),
        }
    }

    pub fn update(&mut self, value: f64, ts_ms: i64) -> String {
        match self.inner.update(value, ts_ms) {
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
```

- [ ] **Step 3: Verify the crate compiles for wasm**

Run: `cd /home/mc/working/tflo && cargo build -p tflo-wasm --target wasm32-unknown-unknown`
Expected: compiles cleanly. If a variant name mismatch error appears (`no variant named ...`), correct it against `tflo-core/src/primitives/results.rs` / `event_mode.rs` and rebuild.

- [ ] **Step 4: Commit**

```bash
cd /home/mc/working/tflo
git add tflo-wasm/src/lib.rs
git commit -m "feat(wasm): add six streaming detector structs"
```

---

### Task 3: Rust integration tests + wasm build

**Files:**
- Modify: `tflo-wasm/Cargo.toml`
- Create: `tflo-wasm/tests/bridge.rs`

- [ ] **Step 1: Add the test dependency to `tflo-wasm/Cargo.toml`**

After the `[dependencies]` block, add:

```toml
[dev-dependencies]
wasm-bindgen-test = "0.3"
```

- [ ] **Step 2: Write the failing integration test `tflo-wasm/tests/bridge.rs`**

```rust
//! Integration tests for the tflo-wasm bridge — run with `wasm-pack test --node`.

use wasm_bindgen_test::*;
use tflo_wasm::{
    compute_ema, compute_macd, WasmCrossDetector, WasmGlitchFilter,
    WasmHysteresisCrossDetector, WasmPulseWidthDetector, WasmRuntDetector,
    WasmWindowDetector,
};

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
    let mut d = WasmGlitchFilter::new(50.0, 10);
    // short pulse: 0..5 above, falls back -> glitch
    d.update(100.0, 0);
    let r = d.update(0.0, 5);
    assert_eq!(r, "glitch");
    // long pulse: above for 20 ms -> valid
    d.update(100.0, 100);
    let r = d.update(0.0, 120);
    assert_eq!(r, "valid");
}

#[wasm_bindgen_test]
fn runt_detector_classifies_pulses() {
    let mut d = WasmRuntDetector::new(40.0, 85.0);
    d.update(20.0);
    d.update(60.0); // above low, not high
    assert_eq!(d.update(20.0), "runt");
    d.update(100.0); // above high
    assert_eq!(d.update(20.0), "valid");
}

#[wasm_bindgen_test]
fn pulse_width_detector_classifies_pulses() {
    let mut d = WasmPulseWidthDetector::new(50.0, 8, 22);
    d.update(100.0, 0);
    assert_eq!(d.update(0.0, 4), "short");
    d.update(100.0, 100);
    assert_eq!(d.update(0.0, 115), "valid");
    d.update(100.0, 200);
    assert_eq!(d.update(0.0, 240), "long");
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
```

- [ ] **Step 3: Run the tests to verify they fail (then pass)**

Run: `cd /home/mc/working/tflo && wasm-pack test --node tflo-wasm`
Expected: with Tasks 1–2 already done, all 8 tests **PASS**. If a test fails, the assertion shows the actual event string — correct the wrapper `match` arm in `tflo-wasm/src/lib.rs` to match `tflo-core`'s real behavior and re-run. (The detector semantics are authoritative in `tflo-core`; tests adapt to them, not the reverse.)

- [ ] **Step 4: Build the wasm bundle into the site**

Run: `cd /home/mc/working/tflo/tflo-site && npm run build:wasm`
Expected: `[wasm] ✅ tflo-wasm built successfully`. Then verify the new exports are present:
Run: `grep -oE 'compute_ema|compute_macd|WasmGlitchFilter|WasmWindowDetector' /home/mc/working/tflo/tflo-site/public/wasm/tflo.js | sort -u`
Expected: all four names listed.

- [ ] **Step 5: Commit**

```bash
cd /home/mc/working/tflo
git add tflo-wasm/Cargo.toml tflo-wasm/tests/bridge.rs
git commit -m "test(wasm): integration tests for EMA/MACD + detector bridges"
```

---

## Phase 2 — TypeScript wasm bridge

### Task 4: EMA + MACD typed wrappers in `wasm.ts`

**Files:**
- Modify: `tflo-site/src/lib/wasm.ts`

- [ ] **Step 1: Add result/config types after the `CrossEvent` interface (after line 35)**

```ts
/** EMA configuration. */
export interface EmaConfig {
  period: number;
}

/** MACD configuration. */
export interface MacdConfig {
  fast: number;
  slow: number;
  signal: number;
}

/** One MACD output point. */
export interface MacdPoint {
  macd: number;
  signal: number;
  histogram: number;
}
```

- [ ] **Step 2: Extend the `WasmModule` interface (after the `compute_bollinger` line, ~line 85)**

```ts
  compute_ema(input_json: string, config_json: string): string;
  compute_macd(input_json: string, config_json: string): string;
```

- [ ] **Step 3: Add the typed wrappers after `computeBollinger` (after line 183)**

```ts
/** Compute an Exponential Moving Average. */
export function computeEma(
  ticks: Tick[],
  config: EmaConfig,
): (number | null)[] {
  if (!wasmModule)
    throw new WasmError("wasm not initialized — call initWasm() first");
  const result = wasmModule.compute_ema(
    JSON.stringify(ticks),
    JSON.stringify(config),
  );
  return parseResult<(number | null)[]>(result, "computeEma");
}

/** Compute MACD (line, signal, histogram). */
export function computeMacd(
  ticks: Tick[],
  config: MacdConfig,
): (MacdPoint | null)[] {
  if (!wasmModule)
    throw new WasmError("wasm not initialized — call initWasm() first");
  const result = wasmModule.compute_macd(
    JSON.stringify(ticks),
    JSON.stringify(config),
  );
  return parseResult<(MacdPoint | null)[]>(result, "computeMacd");
}
```

- [ ] **Step 4: Verify type-check**

Run: `cd /home/mc/working/tflo/tflo-site && npx astro check`
Expected: 0 errors (2 pre-existing unused-import hints in `engine.spec.ts` / `playground.astro` are unrelated and acceptable).

- [ ] **Step 5: Commit**

```bash
cd /home/mc/working/tflo/tflo-site
git add src/lib/wasm.ts
git commit -m "feat(site): typed EMA/MACD wasm wrappers"
```

---

### Task 5: Streaming detector classes in `wasm.ts`

**Files:**
- Modify: `tflo-site/src/lib/wasm.ts`

- [ ] **Step 1: Declare the detector classes on the `WasmModule` interface**

Inside the `WasmModule` interface, add (after the `compute_macd` line):

```ts
  WasmCrossDetector: new () => WasmDetectorInstance2;
  WasmHysteresisCrossDetector: new (hysteresis: number) => WasmDetectorInstance2;
  WasmGlitchFilter: new (
    threshold: number,
    minDurationMs: number,
  ) => WasmDetectorInstance2;
  WasmRuntDetector: new (low: number, high: number) => WasmDetectorInstance1;
  WasmPulseWidthDetector: new (
    threshold: number,
    minWidthMs: number,
    maxWidthMs: number,
  ) => WasmDetectorInstance2;
  WasmWindowDetector: new (low: number, high: number) => WasmDetectorInstance1;
```

Add these two helper interfaces just above the `WasmModule` interface:

```ts
/** A wasm detector whose `update` takes a value and a second numeric arg. */
interface WasmDetectorInstance2 {
  update(value: number, arg: number): string;
  reset(): void;
  free(): void;
}

/** A wasm detector whose `update` takes only a value. */
interface WasmDetectorInstance1 {
  update(value: number): string;
  reset(): void;
  free(): void;
}
```

- [ ] **Step 2: Add the detector event types and TS wrapper classes at the end of `wasm.ts`**

```ts
// ── Streaming detectors ───────────────────────────────────────────────

export type CrossDetectorEvent = "rising" | "falling" | "none";
export type GlitchEvent = "valid" | "glitch" | "none";
export type RuntEvent = "valid" | "runt" | "none";
export type PulseWidthEvent = "short" | "valid" | "long" | "none";
export type WindowDetectorEvent =
  | "entered"
  | "exitedLow"
  | "exitedHigh"
  | "none";

function requireWasm(): WasmModule {
  if (!wasmModule)
    throw new WasmError("wasm not initialized — call initWasm() first");
  return wasmModule;
}

/** Streaming threshold-cross detector. */
export class CrossDetector {
  private inner: WasmDetectorInstance2;
  constructor() {
    this.inner = new (requireWasm().WasmCrossDetector)();
  }
  update(value: number, threshold: number): CrossDetectorEvent {
    return this.inner.update(value, threshold) as CrossDetectorEvent;
  }
  reset(): void {
    this.inner.reset();
  }
  free(): void {
    this.inner.free();
  }
}

/** Streaming hysteresis-cross detector. */
export class HysteresisCrossDetector {
  private inner: WasmDetectorInstance2;
  constructor(hysteresis: number) {
    this.inner = new (requireWasm().WasmHysteresisCrossDetector)(hysteresis);
  }
  update(value: number, threshold: number): CrossDetectorEvent {
    return this.inner.update(value, threshold) as CrossDetectorEvent;
  }
  reset(): void {
    this.inner.reset();
  }
  free(): void {
    this.inner.free();
  }
}

/** Streaming glitch filter. */
export class GlitchFilter {
  private inner: WasmDetectorInstance2;
  constructor(threshold: number, minDurationMs: number) {
    this.inner = new (requireWasm().WasmGlitchFilter)(threshold, minDurationMs);
  }
  update(value: number, tsMs: number): GlitchEvent {
    return this.inner.update(value, tsMs) as GlitchEvent;
  }
  reset(): void {
    this.inner.reset();
  }
  free(): void {
    this.inner.free();
  }
}

/** Streaming runt detector. */
export class RuntDetector {
  private inner: WasmDetectorInstance1;
  constructor(low: number, high: number) {
    this.inner = new (requireWasm().WasmRuntDetector)(low, high);
  }
  update(value: number): RuntEvent {
    return this.inner.update(value) as RuntEvent;
  }
  reset(): void {
    this.inner.reset();
  }
  free(): void {
    this.inner.free();
  }
}

/** Streaming pulse-width detector. */
export class PulseWidthDetector {
  private inner: WasmDetectorInstance2;
  constructor(threshold: number, minWidthMs: number, maxWidthMs: number) {
    const mod = requireWasm();
    this.inner = new mod.WasmPulseWidthDetector(
      threshold,
      minWidthMs,
      maxWidthMs,
    );
  }
  update(value: number, tsMs: number): PulseWidthEvent {
    return this.inner.update(value, tsMs) as PulseWidthEvent;
  }
  reset(): void {
    this.inner.reset();
  }
  free(): void {
    this.inner.free();
  }
}

/** Streaming window detector. */
export class WindowDetector {
  private inner: WasmDetectorInstance1;
  constructor(low: number, high: number) {
    this.inner = new (requireWasm().WasmWindowDetector)(low, high);
  }
  update(value: number): WindowDetectorEvent {
    return this.inner.update(value) as WindowDetectorEvent;
  }
  reset(): void {
    this.inner.reset();
  }
  free(): void {
    this.inner.free();
  }
}
```

- [ ] **Step 3: Verify type-check**

Run: `cd /home/mc/working/tflo/tflo-site && npx astro check`
Expected: 0 errors.

- [ ] **Step 4: Commit**

```bash
cd /home/mc/working/tflo/tflo-site
git add src/lib/wasm.ts
git commit -m "feat(site): streaming detector classes wrapping the wasm structs"
```

---

## Phase 3 — Feeds

### Task 6: Seeded RNG, jitter, and pulse/hovering feeds in `feeds.ts`

**Files:**
- Modify: `tflo-site/src/lib/feeds.ts`
- Create: `tflo-site/src/lib/__tests__/feeds.spec.ts`

- [ ] **Step 1: Write the failing test `tflo-site/src/lib/__tests__/feeds.spec.ts`**

```ts
import { describe, it, expect } from "vitest";
import {
  createRng,
  pulseTrain,
  hoveringSignal,
  applyJitter,
} from "../feeds";

describe("createRng", () => {
  it("is deterministic for a given seed", () => {
    const a = createRng(42);
    const b = createRng(42);
    expect([a(), a(), a()]).toEqual([b(), b(), b()]);
  });
  it("differs across seeds", () => {
    const a = createRng(1);
    const b = createRng(2);
    expect(a()).not.toEqual(b());
  });
});

describe("pulseTrain", () => {
  it("produces baseline outside pulses and baseline+amplitude inside", () => {
    const ticks = pulseTrain(50, [{ start: 10, width: 5, amplitude: 60 }], 20);
    expect(ticks).toHaveLength(50);
    expect(ticks[0].value).toBe(20);
    expect(ticks[12].value).toBe(80);
    expect(ticks[15].value).toBe(20);
  });
});

describe("hoveringSignal", () => {
  it("produces `duration` ticks and is deterministic for a seed", () => {
    const a = hoveringSignal(100, 50, 7, 3, 2, createRng(7));
    const b = hoveringSignal(100, 50, 7, 3, 2, createRng(7));
    expect(a).toHaveLength(100);
    expect(a.map((t) => t.value)).toEqual(b.map((t) => t.value));
  });
});

describe("applyJitter", () => {
  it("leaves ts untouched and perturbs value within ±amount bounds", () => {
    const base = [
      { ts: 0, value: 50 },
      { ts: 1, value: 50 },
    ];
    const out = applyJitter(base, 3, createRng(1));
    expect(out.map((t) => t.ts)).toEqual([0, 1]);
    for (const t of out) {
      expect(Math.abs(t.value - 50)).toBeLessThanOrEqual(3);
    }
  });
  it("is a no-op when amount is 0", () => {
    const base = [{ ts: 0, value: 50 }];
    expect(applyJitter(base, 0, createRng(1))).toEqual(base);
  });
});
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cd /home/mc/working/tflo/tflo-site && npx vitest run src/lib/__tests__/feeds.spec.ts`
Expected: FAIL — `createRng`, `pulseTrain`, `hoveringSignal`, `applyJitter` are not exported.

- [ ] **Step 3: Add the implementations to `tflo-site/src/lib/feeds.ts`**

Add at the end of the file (after the `randn` helper):

```ts
// ── Seeded RNG & synthetic feeds ──────────────────────────────────────

/**
 * Deterministic seeded PRNG (mulberry32). Returns a function yielding
 * floats in [0, 1). Same seed → same sequence.
 */
export function createRng(seed: number): () => number {
  let s = seed | 0;
  return () => {
    s = (s + 0x6d2b79f5) | 0;
    let t = Math.imul(s ^ (s >>> 15), 1 | s);
    t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

/** Standard-normal sample from a seeded RNG (Box-Muller). */
function randnFrom(rng: () => number): number {
  let u = 0;
  let v = 0;
  while (u === 0) u = rng();
  while (v === 0) v = rng();
  return Math.sqrt(-2.0 * Math.log(u)) * Math.cos(2.0 * Math.PI * v);
}

/** A single pulse spec for {@link pulseTrain}. */
export interface PulseSpec {
  start: number;
  width: number;
  amplitude: number;
}

/**
 * Rectangular pulse train — a flat baseline with pulses of a given
 * start tick, width, and amplitude.
 */
export function pulseTrain(
  duration: number,
  pulses: PulseSpec[],
  baseline: number,
): Tick[] {
  const ticks: Tick[] = [];
  for (let i = 0; i < duration; i++) {
    let value = baseline;
    for (const p of pulses) {
      if (i >= p.start && i < p.start + p.width) {
        value = baseline + p.amplitude;
        break;
      }
    }
    ticks.push({ ts: i, value });
  }
  return ticks;
}

/**
 * A signal that lingers around `level` — a slow swing plus seeded noise,
 * so it repeatedly pokes across a nearby threshold.
 */
export function hoveringSignal(
  duration: number,
  level: number,
  swingAmp: number,
  hz: number,
  noiseAmp: number,
  rng: () => number,
): Tick[] {
  const ticks: Tick[] = [];
  for (let i = 0; i < duration; i++) {
    const swing = swingAmp * Math.sin(2 * Math.PI * hz * (i / duration));
    ticks.push({ ts: i, value: level + swing + noiseAmp * randnFrom(rng) });
  }
  return ticks;
}

/**
 * Add bounded uniform jitter to a feed's values. `ts` is untouched.
 * `amount` is the maximum absolute perturbation; 0 returns the input.
 */
export function applyJitter(
  feed: Tick[],
  amount: number,
  rng: () => number,
): Tick[] {
  if (amount === 0) return feed;
  return feed.map((t) => ({
    ts: t.ts,
    value: t.value + (rng() * 2 - 1) * amount,
  }));
}
```

- [ ] **Step 4: Run the test to verify it passes**

Run: `cd /home/mc/working/tflo/tflo-site && npx vitest run src/lib/__tests__/feeds.spec.ts`
Expected: PASS (8 tests).

- [ ] **Step 5: Commit**

```bash
cd /home/mc/working/tflo/tflo-site
git add src/lib/feeds.ts src/lib/__tests__/feeds.spec.ts
git commit -m "feat(site): seeded RNG, jitter, pulse-train and hovering feeds"
```

---

## Phase 4 — Demo compute layer

### Task 7: `demo-config.ts` — per-demo descriptors

**Files:**
- Create: `tflo-site/src/lib/demo-config.ts`
- Create: `tflo-site/src/lib/__tests__/demo-config.spec.ts`

- [ ] **Step 1: Create `tflo-site/src/lib/demo-config.ts`**

```ts
/**
 * Descriptors for every DemoChart demo: which synthetic feed to generate
 * and which wasm indicator/detector to run on it. The DemoChart component
 * and demo-compute.ts read this — there is exactly one place that knows
 * what each demo is.
 */

/** Every demo key. Must match DemoChart's `dataKey` prop. */
export type DemoKey =
  | "sma"
  | "rsi"
  | "bollinger"
  | "macd"
  | "cross"
  | "hysteresis"
  | "glitch"
  | "runt"
  | "pulse-width"
  | "window";

/** How the demo's feed is generated. */
export type FeedSpec =
  | { kind: "sine"; hz: number; amplitude: number; offset: number }
  | { kind: "noisyTrend"; drift: number; volatility: number; start: number }
  | {
      kind: "pulseTrain";
      baseline: number;
      pulses: { start: number; width: number; amplitude: number }[];
    }
  | {
      kind: "hovering";
      level: number;
      swingAmp: number;
      hz: number;
      noiseAmp: number;
    };

/** Which wasm computation runs on the feed. */
export type ComputeSpec =
  | { kind: "sma"; period: number }
  | { kind: "rsi"; period: number }
  | { kind: "bollinger"; period: number; multiplier: number }
  | { kind: "ema"; period: number }
  | { kind: "macd"; fast: number; slow: number; signal: number }
  | { kind: "cross"; threshold: number }
  | { kind: "hysteresis"; threshold: number; margin: number }
  | { kind: "glitch"; threshold: number; minDuration: number }
  | { kind: "runt"; low: number; high: number }
  | { kind: "pulse-width"; threshold: number; min: number; max: number }
  | { kind: "window"; low: number; high: number };

export interface DemoDescriptor {
  feed: FeedSpec;
  compute: ComputeSpec;
  /** Maximum jitter added to each looped feed regeneration. */
  jitter: number;
  /** Source-feed label shown in the DemoChart footer. */
  sourceFeed: string;
}

/** Number of ticks every demo feed contains. */
export const DEMO_DURATION = 200;

export const DEMO_CONFIG: Record<DemoKey, DemoDescriptor> = {
  sma: {
    feed: { kind: "sine", hz: 5, amplitude: 50, offset: 0 },
    compute: { kind: "sma", period: 20 },
    jitter: 2,
    sourceFeed: "sine",
  },
  rsi: {
    feed: { kind: "noisyTrend", drift: 0.05, volatility: 2.5, start: 50 },
    compute: { kind: "rsi", period: 14 },
    jitter: 0,
    sourceFeed: "noisy",
  },
  bollinger: {
    feed: { kind: "sine", hz: 5, amplitude: 50, offset: 0 },
    compute: { kind: "bollinger", period: 20, multiplier: 2 },
    jitter: 2,
    sourceFeed: "sine",
  },
  macd: {
    feed: { kind: "sine", hz: 5, amplitude: 50, offset: 0 },
    compute: { kind: "macd", fast: 12, slow: 26, signal: 9 },
    jitter: 1.5,
    sourceFeed: "sine",
  },
  cross: {
    feed: {
      kind: "pulseTrain",
      baseline: 50,
      pulses: [
        { start: 70, width: 19, amplitude: 7 },
        { start: 120, width: 20, amplitude: -10 },
        { start: 187, width: 13, amplitude: 7 },
      ],
    },
    compute: { kind: "cross", threshold: 55 },
    jitter: 1.5,
    sourceFeed: "step",
  },
  hysteresis: {
    feed: { kind: "hovering", level: 50, swingAmp: 7, hz: 3, noiseAmp: 2 },
    compute: { kind: "hysteresis", threshold: 50, margin: 4 },
    jitter: 0,
    sourceFeed: "hovering",
  },
  glitch: {
    feed: {
      kind: "pulseTrain",
      baseline: 20,
      pulses: [
        { start: 18, width: 6, amplitude: 60 },
        { start: 44, width: 18, amplitude: 60 },
        { start: 82, width: 4, amplitude: 60 },
        { start: 104, width: 26, amplitude: 60 },
        { start: 150, width: 7, amplitude: 60 },
        { start: 172, width: 16, amplitude: 60 },
      ],
    },
    compute: { kind: "glitch", threshold: 50, minDuration: 10 },
    jitter: 1.5,
    sourceFeed: "pulse",
  },
  runt: {
    feed: {
      kind: "pulseTrain",
      baseline: 20,
      pulses: [
        { start: 20, width: 14, amplitude: 50 },
        { start: 50, width: 16, amplitude: 85 },
        { start: 86, width: 12, amplitude: 45 },
        { start: 116, width: 18, amplitude: 90 },
        { start: 150, width: 14, amplitude: 48 },
        { start: 176, width: 15, amplitude: 88 },
      ],
    },
    compute: { kind: "runt", low: 40, high: 85 },
    jitter: 1.5,
    sourceFeed: "pulse",
  },
  "pulse-width": {
    feed: {
      kind: "pulseTrain",
      baseline: 20,
      pulses: [
        { start: 18, width: 5, amplitude: 55 },
        { start: 40, width: 15, amplitude: 55 },
        { start: 72, width: 32, amplitude: 55 },
        { start: 122, width: 12, amplitude: 55 },
        { start: 150, width: 6, amplitude: 55 },
        { start: 170, width: 20, amplitude: 55 },
      ],
    },
    compute: { kind: "pulse-width", threshold: 50, min: 8, max: 22 },
    jitter: 1.5,
    sourceFeed: "pulse",
  },
  window: {
    feed: { kind: "sine", hz: 2.5, amplitude: 32, offset: 50 },
    compute: { kind: "window", low: 38, high: 68 },
    jitter: 2,
    sourceFeed: "sine",
  },
};
```

> Note: the `cross` feed uses small `pulseTrain` pulses around a baseline of 50 so values both rise above and fall below the threshold of 55; the `amplitude: -10` pulse dips the signal to 40.

- [ ] **Step 2: Write the test `tflo-site/src/lib/__tests__/demo-config.spec.ts`**

```ts
import { describe, it, expect } from "vitest";
import { DEMO_CONFIG, DEMO_DURATION, type DemoKey } from "../demo-config";

const KEYS: DemoKey[] = [
  "sma",
  "rsi",
  "bollinger",
  "macd",
  "cross",
  "hysteresis",
  "glitch",
  "runt",
  "pulse-width",
  "window",
];

describe("DEMO_CONFIG", () => {
  it("has a descriptor for every demo key", () => {
    for (const k of KEYS) {
      expect(DEMO_CONFIG[k]).toBeDefined();
      expect(DEMO_CONFIG[k].sourceFeed).toBeTypeOf("string");
      expect(DEMO_CONFIG[k].jitter).toBeGreaterThanOrEqual(0);
    }
  });

  it("each compute kind matches its demo key (except sma/ema split)", () => {
    expect(DEMO_CONFIG.macd.compute.kind).toBe("macd");
    expect(DEMO_CONFIG.glitch.compute.kind).toBe("glitch");
    expect(DEMO_CONFIG.window.compute.kind).toBe("window");
  });

  it("DEMO_DURATION is positive", () => {
    expect(DEMO_DURATION).toBeGreaterThan(0);
  });
});
```

- [ ] **Step 3: Run the test**

Run: `cd /home/mc/working/tflo/tflo-site && npx vitest run src/lib/__tests__/demo-config.spec.ts`
Expected: PASS (3 tests).

- [ ] **Step 4: Commit**

```bash
cd /home/mc/working/tflo/tflo-site
git add src/lib/demo-config.ts src/lib/__tests__/demo-config.spec.ts
git commit -m "feat(site): demo descriptors (feed + compute spec per demo)"
```

---

### Task 8: `demo-compute.ts` — feed generation + wasm orchestration

**Files:**
- Create: `tflo-site/src/lib/demo-compute.ts`

- [ ] **Step 1: Create `tflo-site/src/lib/demo-compute.ts`**

```ts
/**
 * Builds a demo's feed and runs the real wasm computation on it.
 *
 * Indicators (sma/rsi/bollinger/ema/macd) are computed in one batch wasm
 * call. Detectors (cross/hysteresis/glitch/runt/pulse-width/window) are fed
 * tick-by-tick into a streaming wasm detector struct. No indicator or
 * detector logic is implemented here — every number comes from wasm.
 */

import type { Tick, MacdPoint } from "./wasm";
import {
  computeSma,
  computeRsi,
  computeBollinger,
  computeEma,
  computeMacd,
  CrossDetector,
  HysteresisCrossDetector,
  GlitchFilter,
  RuntDetector,
  PulseWidthDetector,
  WindowDetector,
} from "./wasm";
import { createRng, pulseTrain, hoveringSignal, applyJitter } from "./feeds";
import {
  DEMO_DURATION,
  type DemoDescriptor,
  type FeedSpec,
} from "./demo-config";

/** A per-tick result row. Shape depends on the demo's compute kind. */
export type DemoResult =
  | number
  | null
  | MacdPoint
  | { middle: number; upper: number; lower: number }
  | { value: number; cross: string | null }
  | { value: number; event: string | null };

export interface DemoData {
  ticks: Tick[];
  results: DemoResult[];
}

/** Build a feed from a {@link FeedSpec}. */
function buildFeed(spec: FeedSpec, rng: () => number): Tick[] {
  switch (spec.kind) {
    case "sine": {
      const ticks: Tick[] = [];
      for (let i = 0; i < DEMO_DURATION; i++) {
        ticks.push({
          ts: i,
          value:
            spec.amplitude *
              Math.sin(2 * Math.PI * spec.hz * (i / DEMO_DURATION)) +
            spec.offset,
        });
      }
      return ticks;
    }
    case "noisyTrend": {
      const ticks: Tick[] = [];
      let value = spec.start;
      for (let i = 0; i < DEMO_DURATION; i++) {
        // standard-normal step from the seeded rng
        let u = 0;
        let v = 0;
        while (u === 0) u = rng();
        while (v === 0) v = rng();
        const g = Math.sqrt(-2 * Math.log(u)) * Math.cos(2 * Math.PI * v);
        value += spec.drift + spec.volatility * g;
        value = Math.max(0, Math.min(100, value));
        ticks.push({ ts: i, value });
      }
      return ticks;
    }
    case "pulseTrain":
      return pulseTrain(DEMO_DURATION, spec.pulses, spec.baseline);
    case "hovering":
      return hoveringSignal(
        DEMO_DURATION,
        spec.level,
        spec.swingAmp,
        spec.hz,
        spec.noiseAmp,
        rng,
      );
  }
}

/**
 * Generate a fresh jittered feed and compute a demo's results via wasm.
 * Takes a {@link DemoDescriptor} directly — inline demos pass the curated
 * descriptor from `DEMO_CONFIG`; the central demo passes one built from UI
 * controls. `seed` varies per loop so each iteration shows different live
 * data. Requires `initWasm()` to have resolved.
 */
export function computeDemo(
  descriptor: DemoDescriptor,
  seed: number,
): DemoData {
  const rng = createRng(seed);
  const ticks = applyJitter(buildFeed(descriptor.feed, rng), descriptor.jitter, rng);
  const c = descriptor.compute;

  switch (c.kind) {
    case "sma":
      return { ticks, results: computeSma(ticks, { period: c.period }) };
    case "ema":
      return { ticks, results: computeEma(ticks, { period: c.period }) };
    case "rsi":
      return { ticks, results: computeRsi(ticks, { period: c.period }) };
    case "bollinger":
      return {
        ticks,
        results: computeBollinger(ticks, {
          period: c.period,
          multiplier: c.multiplier,
        }),
      };
    case "macd":
      return {
        ticks,
        results: computeMacd(ticks, {
          fast: c.fast,
          slow: c.slow,
          signal: c.signal,
        }),
      };
    case "cross": {
      const det = new CrossDetector();
      const results = ticks.map((t) => {
        const ev = det.update(t.value, c.threshold);
        return {
          value: t.value,
          cross:
            ev === "rising" ? "above" : ev === "falling" ? "below" : null,
        };
      });
      det.free();
      return { ticks, results };
    }
    case "hysteresis": {
      const det = new HysteresisCrossDetector(c.margin);
      const results = ticks.map((t) => {
        const ev = det.update(t.value, c.threshold);
        return { value: t.value, event: ev === "none" ? null : ev };
      });
      det.free();
      return { ticks, results };
    }
    case "glitch": {
      const det = new GlitchFilter(c.threshold, c.minDuration);
      const results = ticks.map((t) => {
        const ev = det.update(t.value, t.ts);
        return { value: t.value, event: ev === "none" ? null : ev };
      });
      det.free();
      return { ticks, results };
    }
    case "runt": {
      const det = new RuntDetector(c.low, c.high);
      const results = ticks.map((t) => {
        const ev = det.update(t.value);
        return { value: t.value, event: ev === "none" ? null : ev };
      });
      det.free();
      return { ticks, results };
    }
    case "pulse-width": {
      const det = new PulseWidthDetector(c.threshold, c.min, c.max);
      const results = ticks.map((t) => {
        const ev = det.update(t.value, t.ts);
        return { value: t.value, event: ev === "none" ? null : ev };
      });
      det.free();
      return { ticks, results };
    }
    case "window": {
      const det = new WindowDetector(c.low, c.high);
      const results = ticks.map((t) => {
        const ev = det.update(t.value);
        return { value: t.value, event: ev === "none" ? null : ev };
      });
      det.free();
      return { ticks, results };
    }
  }
}
```

- [ ] **Step 2: Verify type-check**

Run: `cd /home/mc/working/tflo/tflo-site && npx astro check`
Expected: 0 errors.

- [ ] **Step 3: Commit**

```bash
cd /home/mc/working/tflo/tflo-site
git add src/lib/demo-compute.ts
git commit -m "feat(site): demo-compute — live wasm computation per demo"
```

---

## Phase 5 — DemoChart + chart reworks

### Task 9: Rework `DemoChart.tsx` to compute live via wasm

**Files:**
- Modify: `tflo-site/src/components/DemoChart.tsx`

The component keeps its play/pause/speed controls, the windowed-pointer animation, and the `renderChart()` switch (the sub-charts are unchanged). What changes: instead of importing static JSON, it loads wasm, generates a jittered feed, and computes results via `computeDemo`; on each loop it regenerates with a new seed.

- [ ] **Step 1: Replace the imports and `DataKey`/`DATA_MAP` block**

Delete the static-JSON imports (the `import smaData ...` … `import windowData ...` lines) and the `DATA_MAP` constant. Replace the top of the file's import section with:

```tsx
"use client";

import React, { useState, useEffect, useRef, useCallback } from "react";
import SmaChart from "./sub/SmaChart";
import RsiChart from "./sub/RsiChart";
import BollingerChart from "./sub/BollingerChart";
import CrossChart from "./sub/CrossChart";
import MacdChart, { type MacdResult } from "./sub/MacdChart";
import HysteresisChart, {
  type HysteresisResult,
} from "./sub/HysteresisChart";
import GlitchChart, { type GlitchResult } from "./sub/GlitchChart";
import RuntChart, { type RuntResult } from "./sub/RuntChart";
import PulseWidthChart, {
  type PulseWidthResult,
} from "./sub/PulseWidthChart";
import WindowChart, { type WindowResult } from "./sub/WindowChart";
import { initWasm, type Band } from "../lib/wasm";
import { computeDemo, type DemoData } from "../lib/demo-compute";
import { DEMO_CONFIG, type DemoKey } from "../lib/demo-config";

type DataKey = DemoKey;
```

- [ ] **Step 2: Replace the dataset acquisition + add wasm/feed state**

Replace this block (currently near the top of the `DemoChart` function body):

```tsx
  const dataset = DATA_MAP[dataKey];
  const { ticks, results, sourceFeed, config } = dataset;
```

with:

```tsx
  const baseDescriptor = DEMO_CONFIG[dataKey];
  const sourceFeed = baseDescriptor.sourceFeed;

  // User-controllable jitter, initialised to the demo's curated default.
  const [jitter, setJitter] = useState(baseDescriptor.jitter);

  // Live wasm-computed demo data; null until wasm has loaded and the
  // first feed has been computed.
  const [demo, setDemo] = useState<DemoData | null>(null);
  const [wasmError, setWasmError] = useState<string | null>(null);
  const loopSeedRef = useRef(0);
  // Latest jitter — read inside the animation interval without re-subscribing.
  const jitterRef = useRef(jitter);
  jitterRef.current = jitter;

  // Load wasm once, then compute the first feed.
  useEffect(() => {
    let cancelled = false;
    initWasm()
      .then(() => {
        if (cancelled) return;
        loopSeedRef.current = Math.floor(Math.random() * 1e9);
        setDemo(
          computeDemo(
            { ...baseDescriptor, jitter: jitterRef.current },
            loopSeedRef.current,
          ),
        );
      })
      .catch((err: unknown) => {
        if (!cancelled) {
          setWasmError(err instanceof Error ? err.message : String(err));
        }
      });
    return () => {
      cancelled = true;
    };
  }, [dataKey]);

  // Recompute immediately when the user changes jitter.
  useEffect(() => {
    if (!demo) return;
    loopSeedRef.current += 1;
    setDemo(computeDemo({ ...baseDescriptor, jitter }, loopSeedRef.current));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [jitter]);

  const ticks = demo?.ticks ?? [];
  const results = demo?.results ?? [];
```

- [ ] **Step 3: Regenerate the feed on each loop restart**

In the animation `useEffect`, the `setPointer` callback wraps to `0` when `next >= ticks.length`. Change that wrap branch so a new jittered feed is computed each loop. Replace the interval body:

```tsx
    intervalRef.current = setInterval(() => {
      setPointer((prev) => {
        const next = prev + 1;
        if (next >= ticks.length) {
          return loopProp ? 0 : Math.max(0, ticks.length - 1);
        }
        return next;
      });
    }, intervalMs);
```

with:

```tsx
    intervalRef.current = setInterval(() => {
      setPointer((prev) => {
        const next = prev + 1;
        if (next >= ticks.length) {
          if (loopProp) {
            // Fresh jittered feed each loop — live, never the same twice.
            loopSeedRef.current += 1;
            setDemo(
              computeDemo(
                { ...baseDescriptor, jitter: jitterRef.current },
                loopSeedRef.current,
              ),
            );
            return 0;
          }
          return Math.max(0, ticks.length - 1);
        }
        return next;
      });
    }, intervalMs);
```

- [ ] **Step 4: Render loading / error states**

In `renderChart()`, before the existing `if (visibleTicks.length === 0)` block, add:

```tsx
    if (wasmError) {
      return (
        <div
          style={{
            height,
            display: "flex",
            alignItems: "center",
            justifyContent: "center",
            color: "#c0392b",
            background: "#fdf0ef",
            borderRadius: 8,
            fontSize: "0.85rem",
            padding: "0 1rem",
            textAlign: "center",
          }}
        >
          Demo unavailable — wasm failed to load: {wasmError}
        </div>
      );
    }
    if (!demo) {
      return (
        <div
          style={{
            height,
            display: "flex",
            alignItems: "center",
            justifyContent: "center",
            color: "#666",
            background: "#f5f5f5",
            borderRadius: 8,
            fontSize: "0.9rem",
          }}
        >
          Loading demo…
        </div>
      );
    }
```

- [ ] **Step 5: Update the `config` references in `renderChart()`**

The cross/detector cases previously read `config.threshold` etc. from the dataset. Replace each detector case's `config={{...}}` prop with a literal pulled from `DEMO_CONFIG[dataKey].compute`. For example the `cross` case becomes:

```tsx
      case "cross": {
        const crossCompute = DEMO_CONFIG.cross.compute as {
          kind: "cross";
          threshold: number;
        };
        const crossEvents = (
          results as { value: number; cross: string | null }[]
        )
          .flatMap((r, i) =>
            r.cross !== null
              ? [
                  {
                    ts: ticks[i]?.ts ?? 0,
                    value: r.value,
                    direction: r.cross as "above" | "below",
                  },
                ]
              : [],
          )
          .filter(
            (ev) =>
              ev.ts >= (visibleTicks[0]?.ts ?? 0) &&
              ev.ts <= (visibleTicks[visibleTicks.length - 1]?.ts ?? 0),
          );
        return (
          <CrossChart
            ticks={visibleTicks}
            results={crossEvents}
            config={{ threshold: crossCompute.threshold }}
            height={height}
          />
        );
      }
```

Apply the same pattern for `hysteresis`, `glitch`, `runt`, `pulse-width`, `window` — read the typed `compute` object from `DEMO_CONFIG[dataKey].compute` and pass the matching `config` prop. The `sma`/`rsi`/`bollinger`/`macd` cases keep passing `results` straight through (their sub-charts are unchanged).

- [ ] **Step 6: Add a jitter slider to the controls bar**

`DemoChart`'s controls bar renders the play button, the speed buttons, and the feed label. Add a jitter slider between the speed buttons and the feed label (the feed label has `marginLeft: "auto"`, so insert before it):

```tsx
        {/* Jitter slider */}
        <span style={{ color: "#666", fontSize: 12, marginLeft: 8 }}>
          Jitter
        </span>
        <input
          type="range"
          min={0}
          max={10}
          step={0.5}
          value={jitter}
          onChange={(e) => setJitter(Number(e.target.value))}
          aria-label="Feed jitter amount"
          style={{ width: 70 }}
        />
        <span style={{ color: "#999", fontSize: 11, minWidth: 22 }}>
          {jitter}
        </span>
```

Each change recomputes the feed via the `[jitter]` effect added in Step 2 — the curated default still applies on first load.

- [ ] **Step 7: Verify type-check and dev render**

Run: `cd /home/mc/working/tflo/tflo-site && npx astro check`
Expected: 0 errors.
Run: `curl -s -o /dev/null -w "%{http_code}\n" http://localhost:4321/docs/signals` (dev server running)
Expected: `200`.

- [ ] **Step 8: Commit**

```bash
cd /home/mc/working/tflo/tflo-site
git add src/components/DemoChart.tsx
git commit -m "feat(site): DemoChart computes live via wasm with per-loop jitter"
```

---

### Task 10: Rework Glitch/Runt/PulseWidth charts to derive spans from data

The wasm detectors emit only an event string. `GlitchChart` and `PulseWidthChart` currently read `results[i].width`; `RuntChart`'s data contract carried `peak`. These must be derived from the `value` series + `config` thresholds instead.

**Files:**
- Modify: `tflo-site/src/components/sub/GlitchChart.tsx`
- Modify: `tflo-site/src/components/sub/PulseWidthChart.tsx`
- Modify: `tflo-site/src/components/sub/RuntChart.tsx`

- [ ] **Step 1: Update `GlitchResult` / `PulseWidthResult` / `RuntResult` interfaces**

In each file, change the exported interface to drop the `width` / `peak` field:

```ts
// GlitchChart.tsx
export interface GlitchResult {
  value: number;
  event: "valid" | "glitch" | null;
}
```
```ts
// PulseWidthChart.tsx
export interface PulseWidthResult {
  value: number;
  event: "short" | "valid" | "long" | null;
}
```
```ts
// RuntChart.tsx
export interface RuntResult {
  value: number;
  event: "valid" | "runt" | null;
}
```

- [ ] **Step 2: Derive pulse spans in `GlitchChart` and `PulseWidthChart`**

In each, the per-pulse `ReferenceArea` previously used `i - r.width` for the start. Replace the start computation with a backward scan from the event tick while the value stays above the threshold. In `GlitchChart`, inside the `results.map((r, i) => …)` that renders pulse `ReferenceArea`s, replace `const startIdx = Math.max(0, i - (r.width ?? 0));` with:

```tsx
            // Walk back from the falling-edge tick to the rising edge.
            let startIdx = i;
            while (startIdx > 0 && chartData[startIdx - 1].value > threshold) {
              startIdx -= 1;
            }
```

`threshold` must be in scope — add `const threshold = config.threshold;` near the top of the component body if not already present (it is present in `PulseWidthChart`; add it to `GlitchChart`).

In `PulseWidthChart`, apply the same backward-scan and compute the width label from the span: replace the `label` value `` `${r.width}t` `` with `` `${i - startIdx}t` ``.

- [ ] **Step 3: Derive the peak in `RuntChart` (if it used `peak`)**

If `RuntChart` references `r.peak`, replace it with a backward+forward scan computing the max value over the pulse the event belongs to. Where the event is at index `i`, the pulse is the contiguous run around `i` with `value >= config.low`:

```tsx
            let lo = i;
            while (lo > 0 && chartData[lo - 1].value >= config.low) lo -= 1;
            let hi = i;
            while (
              hi < chartData.length - 1 &&
              chartData[hi + 1].value >= config.low
            )
              hi += 1;
            let peak = chartData[lo].value;
            for (let k = lo; k <= hi; k++) {
              if (chartData[k].value > peak) peak = chartData[k].value;
            }
```

If `RuntChart` does not reference `peak` at all, this step is a no-op — just confirm by `grep -n peak src/components/sub/RuntChart.tsx`.

- [ ] **Step 4: Verify type-check**

Run: `cd /home/mc/working/tflo/tflo-site && npx astro check`
Expected: 0 errors.

- [ ] **Step 5: Commit**

```bash
cd /home/mc/working/tflo/tflo-site
git add src/components/sub/GlitchChart.tsx src/components/sub/PulseWidthChart.tsx src/components/sub/RuntChart.tsx
git commit -m "refactor(site): detector charts derive pulse spans from data"
```

---

## Phase 6 — Delete the duplication + verify

### Task 11: Remove the JS reimplementations and static data

**Files:**
- Delete: `tflo-site/scripts/generate-demo-data.mjs`
- Delete: `tflo-site/src/data/demos/` (whole directory)
- Delete: `tflo-site/src/lib/__tests__/demo-data.spec.ts`
- Delete: `tflo-site/src/lib/__tests__/demo-cross-conversion.spec.ts`
- Delete: `tflo-site/src/lib/__tests__/demo-macd-conversion.spec.ts`
- Delete: `tflo-site/src/lib/__tests__/demo-detectors.spec.ts`
- Modify: `tflo-site/package.json`

- [ ] **Step 1: Delete the script, data, and obsolete tests**

```bash
cd /home/mc/working/tflo/tflo-site
git rm scripts/generate-demo-data.mjs
git rm -r src/data/demos
git rm src/lib/__tests__/demo-data.spec.ts \
       src/lib/__tests__/demo-cross-conversion.spec.ts \
       src/lib/__tests__/demo-macd-conversion.spec.ts \
       src/lib/__tests__/demo-detectors.spec.ts
```

- [ ] **Step 2: Remove the `generate:demos` script from `package.json`**

In `tflo-site/package.json`, delete the line:

```json
    "generate:demos": "node scripts/generate-demo-data.mjs",
```

and change the `build` script from:

```json
    "build": "npm run generate:demos && npm run build:wasm; astro check && astro build",
```

to:

```json
    "build": "npm run build:wasm && astro check && astro build",
```

- [ ] **Step 3: Confirm nothing still imports the deleted files**

Run: `cd /home/mc/working/tflo/tflo-site && grep -rn "data/demos\|generate-demo-data" src astro.config.mjs 2>/dev/null`
Expected: no output. (If anything matches, it is a missed reference — fix it before continuing.)

- [ ] **Step 4: Run the full test suite**

Run: `cd /home/mc/working/tflo/tflo-site && npx vitest run`
Expected: PASS — `feeds.spec.ts`, `demo-config.spec.ts`, and `engine.spec.ts` only. No failures, no references to deleted specs.

- [ ] **Step 5: Commit**

```bash
cd /home/mc/working/tflo/tflo-site
git add package.json
git commit -m "chore(site): delete JS demo reimplementations and static data"
```

---

### Task 12: Full verification

**Files:** none (verification only)

- [ ] **Step 1: Rebuild wasm and type-check**

Run: `cd /home/mc/working/tflo/tflo-site && npm run build:wasm && npx astro check`
Expected: wasm builds; `astro check` reports 0 errors.

- [ ] **Step 2: Restart the dev server**

Stop any running `astro dev`, then: `cd /home/mc/working/tflo/tflo-site && npm run dev` (background). Wait for `astro v… ready`.

- [ ] **Step 3: Verify every demo module transforms and pages render**

Run:
```bash
cd /home/mc/working/tflo/tflo-site
for f in src/components/DemoChart.tsx src/lib/demo-compute.ts src/lib/demo-config.ts src/lib/feeds.ts src/lib/wasm.ts; do
  printf "%-34s -> %s\n" "$f" "$(curl -s -o /dev/null -w '%{http_code}' "http://localhost:4321/$f")"
done
for p in docs/signals docs/indicators docs/demos blog/five-signal-detectors; do
  printf "%-28s -> %s\n" "$p" "$(curl -s -o /dev/null -w '%{http_code}' "http://localhost:4321/$p")"
done
```
Expected: every line `200`.

- [ ] **Step 4: Verify wasm serves and exports the new names**

Run:
```bash
curl -s -o /dev/null -w "%{http_code}\n" http://localhost:4321/wasm/tflo.js
grep -oE 'compute_ema|compute_macd|WasmCrossDetector|WasmHysteresisCrossDetector|WasmGlitchFilter|WasmRuntDetector|WasmPulseWidthDetector|WasmWindowDetector' /home/mc/working/tflo/tflo-site/public/wasm/tflo.js | sort -u
```
Expected: `200`, and all 8 names listed.

- [ ] **Step 5: Browser smoke test (each demo computes and animates)**

Open `http://localhost:4321/blog/five-signal-detectors` and `http://localhost:4321/docs/signals` in a browser. For each demo: confirm it leaves the "Loading demo…" state, renders a chart, animates, and on loop shows a visibly different (jittered) feed. Confirm no errors in the browser console. If the `chrome-devtools` MCP is available, use it; otherwise verify manually.

- [ ] **Step 6: Final commit**

```bash
cd /home/mc/working/tflo
git add -A
git commit -m "feat: wasm-powered live demos — single source of truth in tflo-core"
```

---

## Follow-on plans

This plan is the **foundation**. Two further plans build on it — each ships independently and gets its own document. Both depend only on Phases 1–4 here (wasm bridge + `wasm.ts` + `feeds.ts` + `demo-compute.ts`).

**Plan B — Playground on the full signal set.** Today the playground (`playground.astro` → `LiveFeedEngine` in `engine.ts` → `wasm.ts`) already computes via real wasm, but only for SMA/RSI/Bollinger/Cross. Extend `engine.ts` (`IndicatorConfig`, `recomputeIndicators`) and the playground UI so it can also run EMA, MACD, and the six streaming detectors through the new bridge — widening it from 4 signals to all 11, with **zero JS algorithms**. This also completes the in-flight vanilla-`<script>` → React refactor (`PlaygroundChart`, `KnobPanel`). The streaming detectors fit `LiveFeedEngine`'s tick model directly (one wasm detector instance per active detector, fed each tick).

**Plan C — Central demo (light explorer).** A standalone "pick a series, pick a signal, dial in jitter" component, kept **separate** from the playground (playground = signal/CEL builder; explorer = quick concept exploration). A control panel builds a `DemoDescriptor` from a series dropdown + a signal dropdown + a jitter slider, and feeds it straight to the `computeDemo` / `DemoChart` foundation from this plan — **no new compute path**. `KnobPanel.tsx` is the natural starting point for the control panel.

Neither plan reimplements an algorithm: both route through the single `wasm.ts` bridge to `tflo-core`.

## Risks & notes

- **wasm rebuild required:** `public/wasm/` is git-ignored; whoever runs the site must `npm run build:wasm` after Phase 1. CI (`.github/workflows/publish-wasm.yml`) should run it too.
- **Enum variant names:** Task 2 assumes `ThresholdCrossEventMode {Rising,Falling,None}`, `RuntResult {Runt,ValidPulse}`, `PulseWidthResult {TooShort,Valid,TooLong}`, `WindowEvent {EnteredWindow,ExitedLow,ExitedHigh}`. If `cargo build` reports a variant mismatch, correct against `tflo-core/src/primitives/results.rs` and `event_mode.rs`.
- **`wasm-pack test --node`** needs the wasm-bindgen test runner; it is fetched automatically by `wasm-pack` on first run.
- **Per-tick wasm calls:** detectors are fed one tick at a time inside `computeDemo` (≤200 calls per loop) — negligible cost.
- **Detector memory:** each `computeDemo` call creates wasm detector instances and calls `.free()` after use to release wasm memory.
- The five detector chart components from earlier work are reused unchanged except for the `width`/`peak` derivation in Task 10. `MacdChart` and the indicator sub-charts are unchanged.

---

## Self-review

- **Spec coverage:** wasm gap closed (Tasks 1–2); streaming-detector API shape (Task 2, 5, 8); live jittered looped feeds (Tasks 6–9); all JS reimplementations deleted (Task 11); every demo verified (Task 12). ✔
- **Placeholder scan:** no TBD/TODO; every code step shows complete code. ✔
- **Type consistency:** `DemoKey` is the single key type (demo-config.ts) and `DataKey = DemoKey` in DemoChart; `computeDemo` returns `DemoData`; detector event strings (`"rising"`/`"valid"`/`"entered"`/…) match between the Rust wrappers (Task 2), the TS classes (Task 5), and `computeDemo` (Task 8). ✔
