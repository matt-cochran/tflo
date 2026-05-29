/* @ts-self-types="./tflo.d.ts" */

/**
 * Streaming threshold-cross detector. `update(value, threshold)` returns
 * `"rising"`, `"falling"`, or `"none"`.
 */
export class WasmCrossDetector {
    __destroy_into_raw() {
        const ptr = this.__wbg_ptr;
        this.__wbg_ptr = 0;
        WasmCrossDetectorFinalization.unregister(this);
        return ptr;
    }
    free() {
        const ptr = this.__destroy_into_raw();
        wasm.__wbg_wasmcrossdetector_free(ptr, 0);
    }
    constructor() {
        const ret = wasm.wasmcrossdetector_new();
        this.__wbg_ptr = ret;
        WasmCrossDetectorFinalization.register(this, this.__wbg_ptr, this);
        return this;
    }
    reset() {
        wasm.wasmcrossdetector_reset(this.__wbg_ptr);
    }
    /**
     * @param {number} value
     * @param {number} threshold
     * @returns {string}
     */
    update(value, threshold) {
        let deferred1_0;
        let deferred1_1;
        try {
            const ret = wasm.wasmcrossdetector_update(this.__wbg_ptr, value, threshold);
            deferred1_0 = ret[0];
            deferred1_1 = ret[1];
            return getStringFromWasm0(ret[0], ret[1]);
        } finally {
            wasm.__wbindgen_free(deferred1_0, deferred1_1, 1);
        }
    }
}
if (Symbol.dispose) WasmCrossDetector.prototype[Symbol.dispose] = WasmCrossDetector.prototype.free;

/**
 * Streaming glitch filter. `update(value, ts_ms)` returns `"valid"`,
 * `"glitch"`, or `"none"`.
 */
export class WasmGlitchFilter {
    __destroy_into_raw() {
        const ptr = this.__wbg_ptr;
        this.__wbg_ptr = 0;
        WasmGlitchFilterFinalization.unregister(this);
        return ptr;
    }
    free() {
        const ptr = this.__destroy_into_raw();
        wasm.__wbg_wasmglitchfilter_free(ptr, 0);
    }
    /**
     * @param {number} threshold
     * @param {number} min_duration_ms
     */
    constructor(threshold, min_duration_ms) {
        const ret = wasm.wasmglitchfilter_new(threshold, min_duration_ms);
        this.__wbg_ptr = ret;
        WasmGlitchFilterFinalization.register(this, this.__wbg_ptr, this);
        return this;
    }
    reset() {
        wasm.wasmglitchfilter_reset(this.__wbg_ptr);
    }
    /**
     * @param {number} value
     * @param {number} ts_ms
     * @returns {string}
     */
    update(value, ts_ms) {
        let deferred1_0;
        let deferred1_1;
        try {
            const ret = wasm.wasmglitchfilter_update(this.__wbg_ptr, value, ts_ms);
            deferred1_0 = ret[0];
            deferred1_1 = ret[1];
            return getStringFromWasm0(ret[0], ret[1]);
        } finally {
            wasm.__wbindgen_free(deferred1_0, deferred1_1, 1);
        }
    }
}
if (Symbol.dispose) WasmGlitchFilter.prototype[Symbol.dispose] = WasmGlitchFilter.prototype.free;

/**
 * Streaming hysteresis-cross detector. `update(value, threshold)` returns
 * `"rising"`, `"falling"`, or `"none"`.
 */
export class WasmHysteresisCrossDetector {
    __destroy_into_raw() {
        const ptr = this.__wbg_ptr;
        this.__wbg_ptr = 0;
        WasmHysteresisCrossDetectorFinalization.unregister(this);
        return ptr;
    }
    free() {
        const ptr = this.__destroy_into_raw();
        wasm.__wbg_wasmhysteresiscrossdetector_free(ptr, 0);
    }
    /**
     * @param {number} hysteresis
     */
    constructor(hysteresis) {
        const ret = wasm.wasmhysteresiscrossdetector_new(hysteresis);
        this.__wbg_ptr = ret;
        WasmHysteresisCrossDetectorFinalization.register(this, this.__wbg_ptr, this);
        return this;
    }
    reset() {
        wasm.wasmhysteresiscrossdetector_reset(this.__wbg_ptr);
    }
    /**
     * @param {number} value
     * @param {number} threshold
     * @returns {string}
     */
    update(value, threshold) {
        let deferred1_0;
        let deferred1_1;
        try {
            const ret = wasm.wasmhysteresiscrossdetector_update(this.__wbg_ptr, value, threshold);
            deferred1_0 = ret[0];
            deferred1_1 = ret[1];
            return getStringFromWasm0(ret[0], ret[1]);
        } finally {
            wasm.__wbindgen_free(deferred1_0, deferred1_1, 1);
        }
    }
}
if (Symbol.dispose) WasmHysteresisCrossDetector.prototype[Symbol.dispose] = WasmHysteresisCrossDetector.prototype.free;

/**
 * Streaming pulse-width detector. `update(value, ts_ms)` returns
 * `"short"`, `"valid"`, `"long"`, or `"none"`.
 */
export class WasmPulseWidthDetector {
    __destroy_into_raw() {
        const ptr = this.__wbg_ptr;
        this.__wbg_ptr = 0;
        WasmPulseWidthDetectorFinalization.unregister(this);
        return ptr;
    }
    free() {
        const ptr = this.__destroy_into_raw();
        wasm.__wbg_wasmpulsewidthdetector_free(ptr, 0);
    }
    /**
     * @param {number} threshold
     * @param {number} min_width_ms
     * @param {number} max_width_ms
     */
    constructor(threshold, min_width_ms, max_width_ms) {
        const ret = wasm.wasmpulsewidthdetector_new(threshold, min_width_ms, max_width_ms);
        this.__wbg_ptr = ret;
        WasmPulseWidthDetectorFinalization.register(this, this.__wbg_ptr, this);
        return this;
    }
    reset() {
        wasm.wasmpulsewidthdetector_reset(this.__wbg_ptr);
    }
    /**
     * @param {number} value
     * @param {number} ts_ms
     * @returns {string}
     */
    update(value, ts_ms) {
        let deferred1_0;
        let deferred1_1;
        try {
            const ret = wasm.wasmpulsewidthdetector_update(this.__wbg_ptr, value, ts_ms);
            deferred1_0 = ret[0];
            deferred1_1 = ret[1];
            return getStringFromWasm0(ret[0], ret[1]);
        } finally {
            wasm.__wbindgen_free(deferred1_0, deferred1_1, 1);
        }
    }
}
if (Symbol.dispose) WasmPulseWidthDetector.prototype[Symbol.dispose] = WasmPulseWidthDetector.prototype.free;

/**
 * Streaming runt detector. `update(value)` returns `"valid"`, `"runt"`,
 * or `"none"`.
 */
export class WasmRuntDetector {
    __destroy_into_raw() {
        const ptr = this.__wbg_ptr;
        this.__wbg_ptr = 0;
        WasmRuntDetectorFinalization.unregister(this);
        return ptr;
    }
    free() {
        const ptr = this.__destroy_into_raw();
        wasm.__wbg_wasmruntdetector_free(ptr, 0);
    }
    /**
     * @param {number} low
     * @param {number} high
     */
    constructor(low, high) {
        const ret = wasm.wasmruntdetector_new(low, high);
        this.__wbg_ptr = ret;
        WasmRuntDetectorFinalization.register(this, this.__wbg_ptr, this);
        return this;
    }
    reset() {
        wasm.wasmruntdetector_reset(this.__wbg_ptr);
    }
    /**
     * @param {number} value
     * @returns {string}
     */
    update(value) {
        let deferred1_0;
        let deferred1_1;
        try {
            const ret = wasm.wasmruntdetector_update(this.__wbg_ptr, value);
            deferred1_0 = ret[0];
            deferred1_1 = ret[1];
            return getStringFromWasm0(ret[0], ret[1]);
        } finally {
            wasm.__wbindgen_free(deferred1_0, deferred1_1, 1);
        }
    }
}
if (Symbol.dispose) WasmRuntDetector.prototype[Symbol.dispose] = WasmRuntDetector.prototype.free;

/**
 * Streaming window detector. `update(value)` returns `"entered"`,
 * `"exitedLow"`, `"exitedHigh"`, or `"none"`.
 */
export class WasmWindowDetector {
    __destroy_into_raw() {
        const ptr = this.__wbg_ptr;
        this.__wbg_ptr = 0;
        WasmWindowDetectorFinalization.unregister(this);
        return ptr;
    }
    free() {
        const ptr = this.__destroy_into_raw();
        wasm.__wbg_wasmwindowdetector_free(ptr, 0);
    }
    /**
     * @param {number} low
     * @param {number} high
     */
    constructor(low, high) {
        const ret = wasm.wasmwindowdetector_new(low, high);
        this.__wbg_ptr = ret;
        WasmWindowDetectorFinalization.register(this, this.__wbg_ptr, this);
        return this;
    }
    reset() {
        wasm.wasmwindowdetector_reset(this.__wbg_ptr);
    }
    /**
     * @param {number} value
     * @returns {string}
     */
    update(value) {
        let deferred1_0;
        let deferred1_1;
        try {
            const ret = wasm.wasmwindowdetector_update(this.__wbg_ptr, value);
            deferred1_0 = ret[0];
            deferred1_1 = ret[1];
            return getStringFromWasm0(ret[0], ret[1]);
        } finally {
            wasm.__wbindgen_free(deferred1_0, deferred1_1, 1);
        }
    }
}
if (Symbol.dispose) WasmWindowDetector.prototype[Symbol.dispose] = WasmWindowDetector.prototype.free;

/**
 * Compute Bollinger Bands.
 *
 * # Arguments
 * * `input_json` — JSON array of `{"ts": i64, "value": f64}`.
 * * `config_json` — JSON object with `"period"` (usize) and optional `"multiplier"` (f64).
 *
 * # Returns
 * JSON array of `{"middle": f64, "upper": f64, "lower": f64} | null`.
 * @param {string} input_json
 * @param {string} config_json
 * @returns {string}
 */
export function compute_bollinger(input_json, config_json) {
    let deferred3_0;
    let deferred3_1;
    try {
        const ptr0 = passStringToWasm0(input_json, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ptr1 = passStringToWasm0(config_json, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len1 = WASM_VECTOR_LEN;
        const ret = wasm.compute_bollinger(ptr0, len0, ptr1, len1);
        deferred3_0 = ret[0];
        deferred3_1 = ret[1];
        return getStringFromWasm0(ret[0], ret[1]);
    } finally {
        wasm.__wbindgen_free(deferred3_0, deferred3_1, 1);
    }
}

/**
 * Compute an Exponential Moving Average.
 *
 * # Arguments
 * * `input_json` — JSON array of `{"ts": i64, "value": f64}`.
 * * `config_json` — JSON object with `"period"` (usize).
 *
 * # Returns
 * JSON array of EMA values.
 * @param {string} input_json
 * @param {string} config_json
 * @returns {string}
 */
export function compute_ema(input_json, config_json) {
    let deferred3_0;
    let deferred3_1;
    try {
        const ptr0 = passStringToWasm0(input_json, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ptr1 = passStringToWasm0(config_json, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len1 = WASM_VECTOR_LEN;
        const ret = wasm.compute_ema(ptr0, len0, ptr1, len1);
        deferred3_0 = ret[0];
        deferred3_1 = ret[1];
        return getStringFromWasm0(ret[0], ret[1]);
    } finally {
        wasm.__wbindgen_free(deferred3_0, deferred3_1, 1);
    }
}

/**
 * Generic indicator computation entry point.
 *
 * Dispatches to the correct indicator based on `config.indicator`.
 * Supported: `"sma"`, `"rsi"`, `"bollinger"`, `"cross"`, `"ema"`, `"macd"`.
 * @param {string} input_json
 * @param {string} config_json
 * @returns {string}
 */
export function compute_indicator(input_json, config_json) {
    let deferred3_0;
    let deferred3_1;
    try {
        const ptr0 = passStringToWasm0(input_json, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ptr1 = passStringToWasm0(config_json, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len1 = WASM_VECTOR_LEN;
        const ret = wasm.compute_indicator(ptr0, len0, ptr1, len1);
        deferred3_0 = ret[0];
        deferred3_1 = ret[1];
        return getStringFromWasm0(ret[0], ret[1]);
    } finally {
        wasm.__wbindgen_free(deferred3_0, deferred3_1, 1);
    }
}

/**
 * Compute MACD (line, signal, histogram).
 *
 * # Arguments
 * * `input_json` — JSON array of `{"ts": i64, "value": f64}`.
 * * `config_json` — JSON object with `"fast"`, `"slow"`, `"signal"` (usize).
 *
 * # Returns
 * JSON array of `{"macd": f64, "signal": f64, "histogram": f64}`.
 * @param {string} input_json
 * @param {string} config_json
 * @returns {string}
 */
export function compute_macd(input_json, config_json) {
    let deferred3_0;
    let deferred3_1;
    try {
        const ptr0 = passStringToWasm0(input_json, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ptr1 = passStringToWasm0(config_json, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len1 = WASM_VECTOR_LEN;
        const ret = wasm.compute_macd(ptr0, len0, ptr1, len1);
        deferred3_0 = ret[0];
        deferred3_1 = ret[1];
        return getStringFromWasm0(ret[0], ret[1]);
    } finally {
        wasm.__wbindgen_free(deferred3_0, deferred3_1, 1);
    }
}

/**
 * Compute a Relative Strength Index.
 *
 * # Arguments
 * * `input_json` — JSON array of `{"ts": i64, "value": f64}`.
 * * `config_json` — JSON object with `"period"` (usize).
 *
 * # Returns
 * JSON array of RSI values (0–100).
 * @param {string} input_json
 * @param {string} config_json
 * @returns {string}
 */
export function compute_rsi(input_json, config_json) {
    let deferred3_0;
    let deferred3_1;
    try {
        const ptr0 = passStringToWasm0(input_json, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ptr1 = passStringToWasm0(config_json, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len1 = WASM_VECTOR_LEN;
        const ret = wasm.compute_rsi(ptr0, len0, ptr1, len1);
        deferred3_0 = ret[0];
        deferred3_1 = ret[1];
        return getStringFromWasm0(ret[0], ret[1]);
    } finally {
        wasm.__wbindgen_free(deferred3_0, deferred3_1, 1);
    }
}

/**
 * Compute a Simple Moving Average.
 *
 * # Arguments
 * * `input_json` — JSON array of `{"ts": i64, "value": f64}`.
 * * `config_json` — JSON object with `"period"` (usize).
 *
 * # Returns
 * JSON array of SMA values (null during warmup).
 * @param {string} input_json
 * @param {string} config_json
 * @returns {string}
 */
export function compute_sma(input_json, config_json) {
    let deferred3_0;
    let deferred3_1;
    try {
        const ptr0 = passStringToWasm0(input_json, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ptr1 = passStringToWasm0(config_json, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len1 = WASM_VECTOR_LEN;
        const ret = wasm.compute_sma(ptr0, len0, ptr1, len1);
        deferred3_0 = ret[0];
        deferred3_1 = ret[1];
        return getStringFromWasm0(ret[0], ret[1]);
    } finally {
        wasm.__wbindgen_free(deferred3_0, deferred3_1, 1);
    }
}

/**
 * Detect threshold crossings.
 *
 * # Arguments
 * * `input_json` — JSON array of `{"ts": i64, "value": f64}`.
 * * `config_json` — JSON object with `"threshold"` (f64) and optional `"direction"`.
 *
 * # Returns
 * JSON array of `{"ts": i64, "value": f64, "direction": string}`.
 * @param {string} input_json
 * @param {string} config_json
 * @returns {string}
 */
export function detect_cross(input_json, config_json) {
    let deferred3_0;
    let deferred3_1;
    try {
        const ptr0 = passStringToWasm0(input_json, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ptr1 = passStringToWasm0(config_json, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len1 = WASM_VECTOR_LEN;
        const ret = wasm.detect_cross(ptr0, len0, ptr1, len1);
        deferred3_0 = ret[0];
        deferred3_1 = ret[1];
        return getStringFromWasm0(ret[0], ret[1]);
    } finally {
        wasm.__wbindgen_free(deferred3_0, deferred3_1, 1);
    }
}

/**
 * Evaluate CEL rules (JSON format) against items.
 *
 * # Arguments
 * * `rules_json` — JSON string of rule definitions.
 * * `items_json` — JSON array of items with flattened fields.
 *
 * # Returns
 * JSON array of `{"item_id": string, "matched_rules": string[]}`.
 * @param {string} rules_json
 * @param {string} items_json
 * @returns {string}
 */
export function evaluate_rules(rules_json, items_json) {
    let deferred3_0;
    let deferred3_1;
    try {
        const ptr0 = passStringToWasm0(rules_json, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ptr1 = passStringToWasm0(items_json, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len1 = WASM_VECTOR_LEN;
        const ret = wasm.evaluate_rules(ptr0, len0, ptr1, len1);
        deferred3_0 = ret[0];
        deferred3_1 = ret[1];
        return getStringFromWasm0(ret[0], ret[1]);
    } finally {
        wasm.__wbindgen_free(deferred3_0, deferred3_1, 1);
    }
}

/**
 * Evaluate CEL rules (YAML format) against items.
 *
 * # Arguments
 * * `rules_yaml` — YAML string of rule definitions.
 * * `items_json` — JSON array of items.
 *
 * # Returns
 * JSON array of `{"item_id": string, "matched_rules": string[]}`.
 * @param {string} rules_yaml
 * @param {string} items_json
 * @returns {string}
 */
export function evaluate_rules_from_yaml(rules_yaml, items_json) {
    let deferred3_0;
    let deferred3_1;
    try {
        const ptr0 = passStringToWasm0(rules_yaml, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ptr1 = passStringToWasm0(items_json, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len1 = WASM_VECTOR_LEN;
        const ret = wasm.evaluate_rules_from_yaml(ptr0, len0, ptr1, len1);
        deferred3_0 = ret[0];
        deferred3_1 = ret[1];
        return getStringFromWasm0(ret[0], ret[1]);
    } finally {
        wasm.__wbindgen_free(deferred3_0, deferred3_1, 1);
    }
}

/**
 * Initialize panic hook for better error messages in the browser.
 */
export function init() {
    wasm.init();
}
function __wbg_get_imports() {
    const import0 = {
        __proto__: null,
        __wbg___wbindgen_throw_1506f2235d1bdba0: function(arg0, arg1) {
            throw new Error(getStringFromWasm0(arg0, arg1));
        },
        __wbg_error_a6fa202b58aa1cd3: function(arg0, arg1) {
            let deferred0_0;
            let deferred0_1;
            try {
                deferred0_0 = arg0;
                deferred0_1 = arg1;
                console.error(getStringFromWasm0(arg0, arg1));
            } finally {
                wasm.__wbindgen_free(deferred0_0, deferred0_1, 1);
            }
        },
        __wbg_new_227d7c05414eb861: function() {
            const ret = new Error();
            return ret;
        },
        __wbg_stack_3b0d974bbf31e44f: function(arg0, arg1) {
            const ret = arg1.stack;
            const ptr1 = passStringToWasm0(ret, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
            const len1 = WASM_VECTOR_LEN;
            getDataViewMemory0().setInt32(arg0 + 4 * 1, len1, true);
            getDataViewMemory0().setInt32(arg0 + 4 * 0, ptr1, true);
        },
        __wbindgen_init_externref_table: function() {
            const table = wasm.__wbindgen_externrefs;
            const offset = table.grow(4);
            table.set(0, undefined);
            table.set(offset + 0, undefined);
            table.set(offset + 1, null);
            table.set(offset + 2, true);
            table.set(offset + 3, false);
        },
    };
    return {
        __proto__: null,
        "./tflo_bg.js": import0,
    };
}

const WasmCrossDetectorFinalization = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(ptr => wasm.__wbg_wasmcrossdetector_free(ptr, 1));
const WasmGlitchFilterFinalization = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(ptr => wasm.__wbg_wasmglitchfilter_free(ptr, 1));
const WasmHysteresisCrossDetectorFinalization = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(ptr => wasm.__wbg_wasmhysteresiscrossdetector_free(ptr, 1));
const WasmPulseWidthDetectorFinalization = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(ptr => wasm.__wbg_wasmpulsewidthdetector_free(ptr, 1));
const WasmRuntDetectorFinalization = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(ptr => wasm.__wbg_wasmruntdetector_free(ptr, 1));
const WasmWindowDetectorFinalization = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(ptr => wasm.__wbg_wasmwindowdetector_free(ptr, 1));

let cachedDataViewMemory0 = null;
function getDataViewMemory0() {
    if (cachedDataViewMemory0 === null || cachedDataViewMemory0.buffer.detached === true || (cachedDataViewMemory0.buffer.detached === undefined && cachedDataViewMemory0.buffer !== wasm.memory.buffer)) {
        cachedDataViewMemory0 = new DataView(wasm.memory.buffer);
    }
    return cachedDataViewMemory0;
}

function getStringFromWasm0(ptr, len) {
    return decodeText(ptr >>> 0, len);
}

let cachedUint8ArrayMemory0 = null;
function getUint8ArrayMemory0() {
    if (cachedUint8ArrayMemory0 === null || cachedUint8ArrayMemory0.byteLength === 0) {
        cachedUint8ArrayMemory0 = new Uint8Array(wasm.memory.buffer);
    }
    return cachedUint8ArrayMemory0;
}

function passStringToWasm0(arg, malloc, realloc) {
    if (realloc === undefined) {
        const buf = cachedTextEncoder.encode(arg);
        const ptr = malloc(buf.length, 1) >>> 0;
        getUint8ArrayMemory0().subarray(ptr, ptr + buf.length).set(buf);
        WASM_VECTOR_LEN = buf.length;
        return ptr;
    }

    let len = arg.length;
    let ptr = malloc(len, 1) >>> 0;

    const mem = getUint8ArrayMemory0();

    let offset = 0;

    for (; offset < len; offset++) {
        const code = arg.charCodeAt(offset);
        if (code > 0x7F) break;
        mem[ptr + offset] = code;
    }
    if (offset !== len) {
        if (offset !== 0) {
            arg = arg.slice(offset);
        }
        ptr = realloc(ptr, len, len = offset + arg.length * 3, 1) >>> 0;
        const view = getUint8ArrayMemory0().subarray(ptr + offset, ptr + len);
        const ret = cachedTextEncoder.encodeInto(arg, view);

        offset += ret.written;
        ptr = realloc(ptr, len, offset, 1) >>> 0;
    }

    WASM_VECTOR_LEN = offset;
    return ptr;
}

let cachedTextDecoder = new TextDecoder('utf-8', { ignoreBOM: true, fatal: true });
cachedTextDecoder.decode();
const MAX_SAFARI_DECODE_BYTES = 2146435072;
let numBytesDecoded = 0;
function decodeText(ptr, len) {
    numBytesDecoded += len;
    if (numBytesDecoded >= MAX_SAFARI_DECODE_BYTES) {
        cachedTextDecoder = new TextDecoder('utf-8', { ignoreBOM: true, fatal: true });
        cachedTextDecoder.decode();
        numBytesDecoded = len;
    }
    return cachedTextDecoder.decode(getUint8ArrayMemory0().subarray(ptr, ptr + len));
}

const cachedTextEncoder = new TextEncoder();

if (!('encodeInto' in cachedTextEncoder)) {
    cachedTextEncoder.encodeInto = function (arg, view) {
        const buf = cachedTextEncoder.encode(arg);
        view.set(buf);
        return {
            read: arg.length,
            written: buf.length
        };
    };
}

let WASM_VECTOR_LEN = 0;

let wasmModule, wasmInstance, wasm;
function __wbg_finalize_init(instance, module) {
    wasmInstance = instance;
    wasm = instance.exports;
    wasmModule = module;
    cachedDataViewMemory0 = null;
    cachedUint8ArrayMemory0 = null;
    wasm.__wbindgen_start();
    return wasm;
}

async function __wbg_load(module, imports) {
    if (typeof Response === 'function' && module instanceof Response) {
        if (typeof WebAssembly.instantiateStreaming === 'function') {
            try {
                return await WebAssembly.instantiateStreaming(module, imports);
            } catch (e) {
                const validResponse = module.ok && expectedResponseType(module.type);

                if (validResponse && module.headers.get('Content-Type') !== 'application/wasm') {
                    console.warn("`WebAssembly.instantiateStreaming` failed because your server does not serve Wasm with `application/wasm` MIME type. Falling back to `WebAssembly.instantiate` which is slower. Original error:\n", e);

                } else { throw e; }
            }
        }

        const bytes = await module.arrayBuffer();
        return await WebAssembly.instantiate(bytes, imports);
    } else {
        const instance = await WebAssembly.instantiate(module, imports);

        if (instance instanceof WebAssembly.Instance) {
            return { instance, module };
        } else {
            return instance;
        }
    }

    function expectedResponseType(type) {
        switch (type) {
            case 'basic': case 'cors': case 'default': return true;
        }
        return false;
    }
}

function initSync(module) {
    if (wasm !== undefined) return wasm;


    if (module !== undefined) {
        if (Object.getPrototypeOf(module) === Object.prototype) {
            ({module} = module)
        } else {
            console.warn('using deprecated parameters for `initSync()`; pass a single object instead')
        }
    }

    const imports = __wbg_get_imports();
    if (!(module instanceof WebAssembly.Module)) {
        module = new WebAssembly.Module(module);
    }
    const instance = new WebAssembly.Instance(module, imports);
    return __wbg_finalize_init(instance, module);
}

async function __wbg_init(module_or_path) {
    if (wasm !== undefined) return wasm;


    if (module_or_path !== undefined) {
        if (Object.getPrototypeOf(module_or_path) === Object.prototype) {
            ({module_or_path} = module_or_path)
        } else {
            console.warn('using deprecated parameters for the initialization function; pass a single object instead')
        }
    }

    if (module_or_path === undefined) {
        module_or_path = new URL('tflo_bg.wasm', import.meta.url);
    }
    const imports = __wbg_get_imports();

    if (typeof module_or_path === 'string' || (typeof Request === 'function' && module_or_path instanceof Request) || (typeof URL === 'function' && module_or_path instanceof URL)) {
        module_or_path = fetch(module_or_path);
    }

    const { instance, module } = await __wbg_load(await module_or_path, imports);

    return __wbg_finalize_init(instance, module);
}

export { initSync, __wbg_init as default };
