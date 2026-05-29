/* tslint:disable */
/* eslint-disable */

/**
 * Streaming threshold-cross detector. `update(value, threshold)` returns
 * `"rising"`, `"falling"`, or `"none"`.
 */
export class WasmCrossDetector {
    free(): void;
    [Symbol.dispose](): void;
    constructor();
    reset(): void;
    update(value: number, threshold: number): string;
}

/**
 * Streaming glitch filter. `update(value, ts_ms)` returns `"valid"`,
 * `"glitch"`, or `"none"`.
 */
export class WasmGlitchFilter {
    free(): void;
    [Symbol.dispose](): void;
    constructor(threshold: number, min_duration_ms: number);
    reset(): void;
    update(value: number, ts_ms: number): string;
}

/**
 * Streaming hysteresis-cross detector. `update(value, threshold)` returns
 * `"rising"`, `"falling"`, or `"none"`.
 */
export class WasmHysteresisCrossDetector {
    free(): void;
    [Symbol.dispose](): void;
    constructor(hysteresis: number);
    reset(): void;
    update(value: number, threshold: number): string;
}

/**
 * Streaming pulse-width detector. `update(value, ts_ms)` returns
 * `"short"`, `"valid"`, `"long"`, or `"none"`.
 */
export class WasmPulseWidthDetector {
    free(): void;
    [Symbol.dispose](): void;
    constructor(threshold: number, min_width_ms: number, max_width_ms: number);
    reset(): void;
    update(value: number, ts_ms: number): string;
}

/**
 * Streaming runt detector. `update(value)` returns `"valid"`, `"runt"`,
 * or `"none"`.
 */
export class WasmRuntDetector {
    free(): void;
    [Symbol.dispose](): void;
    constructor(low: number, high: number);
    reset(): void;
    update(value: number): string;
}

/**
 * Streaming window detector. `update(value)` returns `"entered"`,
 * `"exitedLow"`, `"exitedHigh"`, or `"none"`.
 */
export class WasmWindowDetector {
    free(): void;
    [Symbol.dispose](): void;
    constructor(low: number, high: number);
    reset(): void;
    update(value: number): string;
}

/**
 * Compute Bollinger Bands.
 *
 * # Arguments
 * * `input_json` — JSON array of `{"ts": i64, "value": f64}`.
 * * `config_json` — JSON object with `"period"` (usize) and optional `"multiplier"` (f64).
 *
 * # Returns
 * JSON array of `{"middle": f64, "upper": f64, "lower": f64} | null`.
 */
export function compute_bollinger(input_json: string, config_json: string): string;

/**
 * Compute an Exponential Moving Average.
 *
 * # Arguments
 * * `input_json` — JSON array of `{"ts": i64, "value": f64}`.
 * * `config_json` — JSON object with `"period"` (usize).
 *
 * # Returns
 * JSON array of EMA values.
 */
export function compute_ema(input_json: string, config_json: string): string;

/**
 * Generic indicator computation entry point.
 *
 * Dispatches to the correct indicator based on `config.indicator`.
 * Supported: `"sma"`, `"rsi"`, `"bollinger"`, `"cross"`, `"ema"`, `"macd"`.
 */
export function compute_indicator(input_json: string, config_json: string): string;

/**
 * Compute MACD (line, signal, histogram).
 *
 * # Arguments
 * * `input_json` — JSON array of `{"ts": i64, "value": f64}`.
 * * `config_json` — JSON object with `"fast"`, `"slow"`, `"signal"` (usize).
 *
 * # Returns
 * JSON array of `{"macd": f64, "signal": f64, "histogram": f64}`.
 */
export function compute_macd(input_json: string, config_json: string): string;

/**
 * Compute a Relative Strength Index.
 *
 * # Arguments
 * * `input_json` — JSON array of `{"ts": i64, "value": f64}`.
 * * `config_json` — JSON object with `"period"` (usize).
 *
 * # Returns
 * JSON array of RSI values (0–100).
 */
export function compute_rsi(input_json: string, config_json: string): string;

/**
 * Compute a Simple Moving Average.
 *
 * # Arguments
 * * `input_json` — JSON array of `{"ts": i64, "value": f64}`.
 * * `config_json` — JSON object with `"period"` (usize).
 *
 * # Returns
 * JSON array of SMA values (null during warmup).
 */
export function compute_sma(input_json: string, config_json: string): string;

/**
 * Detect threshold crossings.
 *
 * # Arguments
 * * `input_json` — JSON array of `{"ts": i64, "value": f64}`.
 * * `config_json` — JSON object with `"threshold"` (f64) and optional `"direction"`.
 *
 * # Returns
 * JSON array of `{"ts": i64, "value": f64, "direction": string}`.
 */
export function detect_cross(input_json: string, config_json: string): string;

/**
 * Evaluate CEL rules (JSON format) against items.
 *
 * # Arguments
 * * `rules_json` — JSON string of rule definitions.
 * * `items_json` — JSON array of items with flattened fields.
 *
 * # Returns
 * JSON array of `{"item_id": string, "matched_rules": string[]}`.
 */
export function evaluate_rules(rules_json: string, items_json: string): string;

/**
 * Evaluate CEL rules (YAML format) against items.
 *
 * # Arguments
 * * `rules_yaml` — YAML string of rule definitions.
 * * `items_json` — JSON array of items.
 *
 * # Returns
 * JSON array of `{"item_id": string, "matched_rules": string[]}`.
 */
export function evaluate_rules_from_yaml(rules_yaml: string, items_json: string): string;

/**
 * Initialize panic hook for better error messages in the browser.
 */
export function init(): void;

export type InitInput = RequestInfo | URL | Response | BufferSource | WebAssembly.Module;

export interface InitOutput {
    readonly memory: WebAssembly.Memory;
    readonly __wbg_wasmcrossdetector_free: (a: number, b: number) => void;
    readonly __wbg_wasmglitchfilter_free: (a: number, b: number) => void;
    readonly __wbg_wasmhysteresiscrossdetector_free: (a: number, b: number) => void;
    readonly __wbg_wasmpulsewidthdetector_free: (a: number, b: number) => void;
    readonly __wbg_wasmruntdetector_free: (a: number, b: number) => void;
    readonly __wbg_wasmwindowdetector_free: (a: number, b: number) => void;
    readonly wasmcrossdetector_new: () => number;
    readonly wasmcrossdetector_reset: (a: number) => void;
    readonly wasmcrossdetector_update: (a: number, b: number, c: number) => [number, number];
    readonly wasmglitchfilter_new: (a: number, b: number) => number;
    readonly wasmglitchfilter_reset: (a: number) => void;
    readonly wasmglitchfilter_update: (a: number, b: number, c: number) => [number, number];
    readonly wasmhysteresiscrossdetector_new: (a: number) => number;
    readonly wasmhysteresiscrossdetector_reset: (a: number) => void;
    readonly wasmhysteresiscrossdetector_update: (a: number, b: number, c: number) => [number, number];
    readonly wasmpulsewidthdetector_new: (a: number, b: number, c: number) => number;
    readonly wasmpulsewidthdetector_reset: (a: number) => void;
    readonly wasmpulsewidthdetector_update: (a: number, b: number, c: number) => [number, number];
    readonly wasmruntdetector_new: (a: number, b: number) => number;
    readonly wasmruntdetector_reset: (a: number) => void;
    readonly wasmruntdetector_update: (a: number, b: number) => [number, number];
    readonly wasmwindowdetector_new: (a: number, b: number) => number;
    readonly wasmwindowdetector_reset: (a: number) => void;
    readonly wasmwindowdetector_update: (a: number, b: number) => [number, number];
    readonly compute_bollinger: (a: number, b: number, c: number, d: number) => [number, number];
    readonly compute_ema: (a: number, b: number, c: number, d: number) => [number, number];
    readonly compute_indicator: (a: number, b: number, c: number, d: number) => [number, number];
    readonly compute_macd: (a: number, b: number, c: number, d: number) => [number, number];
    readonly compute_rsi: (a: number, b: number, c: number, d: number) => [number, number];
    readonly compute_sma: (a: number, b: number, c: number, d: number) => [number, number];
    readonly detect_cross: (a: number, b: number, c: number, d: number) => [number, number];
    readonly evaluate_rules: (a: number, b: number, c: number, d: number) => [number, number];
    readonly evaluate_rules_from_yaml: (a: number, b: number, c: number, d: number) => [number, number];
    readonly init: () => void;
    readonly __wbindgen_free: (a: number, b: number, c: number) => void;
    readonly __wbindgen_malloc: (a: number, b: number) => number;
    readonly __wbindgen_realloc: (a: number, b: number, c: number, d: number) => number;
    readonly __wbindgen_externrefs: WebAssembly.Table;
    readonly __wbindgen_start: () => void;
}

export type SyncInitInput = BufferSource | WebAssembly.Module;

/**
 * Instantiates the given `module`, which can either be bytes or
 * a precompiled `WebAssembly.Module`.
 *
 * @param {{ module: SyncInitInput }} module - Passing `SyncInitInput` directly is deprecated.
 *
 * @returns {InitOutput}
 */
export function initSync(module: { module: SyncInitInput } | SyncInitInput): InitOutput;

/**
 * If `module_or_path` is {RequestInfo} or {URL}, makes a request and
 * for everything else, calls `WebAssembly.instantiate` directly.
 *
 * @param {{ module_or_path: InitInput | Promise<InitInput> }} module_or_path - Passing `InitInput` directly is deprecated.
 *
 * @returns {Promise<InitOutput>}
 */
export default function __wbg_init (module_or_path?: { module_or_path: InitInput | Promise<InitInput> } | InitInput | Promise<InitInput>): Promise<InitOutput>;
