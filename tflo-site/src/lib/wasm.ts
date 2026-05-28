/**
 * Wasm loader and typed bridge for tflo's WebAssembly module.
 *
 * This module lazy-loads the wasm binary and provides typed TypeScript
 * wrappers around the raw JSON-in/JSON-out exports.
 *
 * Usage:
 * ```typescript
 * import { initWasm, computeSma } from "./lib/wasm";
 * await initWasm();
 * const result = computeSma(ticks, { period: 14 });
 * ```
 */

// ── Types ─────────────────────────────────────────────────────────────

/** A single time-series data point. */
export interface Tick {
  ts: number;
  value: number;
}

/** Single band of Bollinger output. */
export interface Band {
  middle: number;
  upper: number;
  lower: number;
}

/** Threshold crossing event. */
export interface CrossEvent {
  ts: number;
  value: number;
  direction: "above" | "below";
}

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

/** SMA configuration. */
export interface SmaConfig {
  period: number;
}

/** RSI configuration. */
export interface RsiConfig {
  period: number;
}

/** Bollinger Bands configuration. */
export interface BollingerConfig {
  period: number;
  multiplier?: number;
}

/** Cross detection configuration. */
export interface CrossConfig {
  threshold: number;
  direction?: "above" | "below" | "both";
}

/** Generic indicator configuration. */
export interface IndicatorConfig {
  indicator: "sma" | "rsi" | "bollinger" | "cross";
  period?: number;
  multiplier?: number;
  threshold?: number;
  direction?: "above" | "below" | "both";
}

/** CEL rule evaluation item. */
export interface CelRuleItem {
  id: string;
  [key: string]: unknown;
}

/** CEL rule evaluation result. */
export interface CelEvaluationResult {
  item_id: string;
  matched_rules: string[];
}

// ── Wasm module type declaration ──────────────────────────────────────

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

interface WasmModule {
  compute_sma(input_json: string, config_json: string): string;
  compute_rsi(input_json: string, config_json: string): string;
  compute_bollinger(input_json: string, config_json: string): string;
  compute_ema(input_json: string, config_json: string): string;
  compute_macd(input_json: string, config_json: string): string;
  detect_cross(input_json: string, config_json: string): string;
  compute_indicator(input_json: string, config_json: string): string;
  evaluate_rules(rules_json: string, items_json: string): string;
  evaluate_rules_from_yaml(rules_yaml: string, items_json: string): string;
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
}

let wasmModule: WasmModule | null = null;
let initPromise: Promise<void> | null = null;

/**
 * Lazy-load the wasm module.
 *
 * Safe to call multiple times — subsequent calls return the same promise.
 */
export async function initWasm(): Promise<void> {
  if (wasmModule) return;
  if (initPromise) return initPromise;

  initPromise = (async () => {
    try {
      // The wasm-pack output lives in `public/wasm/` (copied as-is at
      // build time). In dev, Vite normally appends `?import` to dynamic
      // import URLs for HMR tracking — but files in `public/` aren't
      // supposed to be imported and the dev server rejects the request
      // with 500. The `/* @vite-ignore */` comment only suppresses
      // static analysis; the runtime `__vite__injectQuery` helper still
      // wraps the URL. Workaround: use a Function-constructed import
      // expression, which the Vite transformer cannot see, so no
      // `?import` query is appended.
      const jsUrl = new URL("/wasm/tflo.js", import.meta.url).href;
      const wasmBinaryUrl = new URL("/wasm/tflo_bg.wasm", import.meta.url).href;
      const rawImport = new Function(
        "u",
        "return import(u);",
      ) as (url: string) => Promise<unknown>;
      const module = (await rawImport(jsUrl)) as {
        default: (opts?: { module_or_path?: string }) => Promise<unknown>;
      } & Record<string, unknown>;
      // Pass the wasm binary URL explicitly. When the JS glue is loaded
      // via `new Function(...)` the `import.meta.url` it sees may not be
      // resolvable to the wasm file's location, so we don't rely on the
      // default `new URL('tflo_bg.wasm', import.meta.url)` resolution.
      await module.default({ module_or_path: wasmBinaryUrl });
      wasmModule = module as unknown as WasmModule;
    } catch (err) {
      initPromise = null; // Allow retry on failure
      throw new Error(
        `Failed to load tflo wasm module: ${err instanceof Error ? err.message : String(err)}`,
      );
    }
  })();

  return initPromise;
}

// ── Error-handling helpers ────────────────────────────────────────────

class WasmError extends Error {
  constructor(message: string) {
    super(message);
    this.name = "WasmError";
  }
}

function parseResult<T>(json: string, label: string): T {
  const parsed = JSON.parse(json) as T & { error?: string };
  if (parsed && typeof parsed === "object" && "error" in parsed) {
    throw new WasmError(`${label}: ${(parsed as { error: string }).error}`);
  }
  return parsed as T;
}

// ── Indicator wrappers ────────────────────────────────────────────────

/** Compute a Simple Moving Average. */
export function computeSma(
  ticks: Tick[],
  config: SmaConfig,
): (number | null)[] {
  if (!wasmModule)
    throw new WasmError("wasm not initialized — call initWasm() first");
  const result = wasmModule.compute_sma(
    JSON.stringify(ticks),
    JSON.stringify(config),
  );
  return parseResult<(number | null)[]>(result, "computeSma");
}

/** Compute a Relative Strength Index. */
export function computeRsi(
  ticks: Tick[],
  config: RsiConfig,
): (number | null)[] {
  if (!wasmModule)
    throw new WasmError("wasm not initialized — call initWasm() first");
  const result = wasmModule.compute_rsi(
    JSON.stringify(ticks),
    JSON.stringify(config),
  );
  return parseResult<(number | null)[]>(result, "computeRsi");
}

/** Compute Bollinger Bands. */
export function computeBollinger(
  ticks: Tick[],
  config: BollingerConfig,
): (Band | null)[] {
  if (!wasmModule)
    throw new WasmError("wasm not initialized — call initWasm() first");
  const result = wasmModule.compute_bollinger(
    JSON.stringify(ticks),
    JSON.stringify(config),
  );
  return parseResult<(Band | null)[]>(result, "computeBollinger");
}

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

/** Detect threshold crossings. */
export function detectCross(ticks: Tick[], config: CrossConfig): CrossEvent[] {
  if (!wasmModule)
    throw new WasmError("wasm not initialized — call initWasm() first");
  const result = wasmModule.detect_cross(
    JSON.stringify(ticks),
    JSON.stringify(config),
  );
  return parseResult<CrossEvent[]>(result, "detectCross");
}

/** Generic indicator computation. */
export function computeIndicator(
  ticks: Tick[],
  config: IndicatorConfig,
): unknown {
  if (!wasmModule)
    throw new WasmError("wasm not initialized — call initWasm() first");
  const result = wasmModule.compute_indicator(
    JSON.stringify(ticks),
    JSON.stringify(config),
  );
  return JSON.parse(result);
}

/** Evaluate CEL rules (JSON) against items. */
export function evaluateCelRules(
  rules: Record<string, unknown>,
  items: CelRuleItem[],
): CelEvaluationResult[] {
  if (!wasmModule)
    throw new WasmError("wasm not initialized — call initWasm() first");
  const result = wasmModule.evaluate_rules(
    JSON.stringify(rules),
    JSON.stringify(items),
  );
  return parseResult<CelEvaluationResult[]>(result, "evaluateCelRules");
}

/** Evaluate CEL rules (YAML) against items. */
export function evaluateCelRulesFromYaml(
  rulesYaml: string,
  items: CelRuleItem[],
): CelEvaluationResult[] {
  if (!wasmModule)
    throw new WasmError("wasm not initialized — call initWasm() first");
  const result = wasmModule.evaluate_rules_from_yaml(
    rulesYaml,
    JSON.stringify(items),
  );
  return parseResult<CelEvaluationResult[]>(result, "evaluateCelRulesFromYaml");
}

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
