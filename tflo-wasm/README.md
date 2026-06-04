# tflo-wasm &mdash; Streaming Technical Analysis in the Browser

**tflo-wasm** is the WebAssembly package for [tflo](https://tflo.dev), a Rust-native library for real-time, streaming technical analysis and signal processing. This package lets you compute SMA, RSI, Bollinger Bands, crossing signals, and CEL rule evaluations entirely in-browser &mdash; no server, no network.

```typescript
import init, { computeSma } from "tflo-wasm";

await init();

const ticks = [
  { ts: 0, value: 10 }, { ts: 1, value: 20 },
  { ts: 2, value: 30 }, { ts: 3, value: 25 },
];
const sma = computeSma(ticks, { period: 3 });
// → [null, null, 20, 25]  (null during warmup)
```

## Quick Start

### Install

```bash
npm install tflo-wasm
```

### Load and init

```typescript
import init, {
  computeSma,
  computeRsi,
  computeBollinger,
  detectCross,
  evaluateRules,
} from "tflo-wasm";

await init();
```

> **Important**: The `init()` function loads and instantiates the `.wasm` binary. All compute functions will throw until `init()` resolves.

## API

### Tick format

All indicator functions accept an array of tick objects:

```typescript
interface Tick {
  ts: number;   // timestamp (ms since epoch, or any ordering key)
  value: number; // the observed value
}
```

---

### `computeSma(ticks, config)`

Simple Moving Average over a count-based window.

```typescript
computeSma(ticks, { period: 20 })
// → (number | null)[]
```

- **`period`**: Window size (integer). Must be ≥ 2.
- **Returns**: Array of SMA values. Leading elements are `null` during warmup (before the window is full).

---

### `computeRsi(ticks, config)`

Relative Strength Index (14-period Wilder's smoothing by default).

```typescript
computeRsi(ticks, { period: 14 })
// → (number | null)[]
```

- **`period`**: Lookback period (integer). Must be ≥ 2.
- **Returns**: Array of RSI values in [0, 100]. `null` during warmup.

---

### `computeBollinger(ticks, config)`

Bollinger Bands with configurable multiplier.

```typescript
computeBollinger(ticks, { period: 20, multiplier: 2 })
// → ({ middle: number, upper: number, lower: number } | null)[]
```

- **`period`**: SMA/stddev window (integer).
- **`multiplier`**: Number of standard deviations for the bands (default: `2.0`).
- **Returns**: Array of `{ middle, upper, lower }` objects, or `null` during warmup.

---

### `detectCross(ticks, config)`

Detect when values cross a fixed threshold.

```typescript
detectCross(ticks, { threshold: 70, direction: "both" })
// → { value: number, direction: "above" | "below" }[]
```

- **`threshold`**: The reference value to cross (number).
- **`direction`** (optional): `"above"`, `"below"`, or `"both"` (default).
  - `"above"`: only emits events when crossing *upward* through the threshold.
  - `"below"`: only emits events when crossing *downward*.
  - `"both"`: emits both directions.
- **Returns**: Array of crossing events (may be empty).

---

### `computeIndicator(ticks, config)`

Generic dispatch — picks the right indicator based on `config.indicator`.

```typescript
computeIndicator(ticks, { indicator: "sma", period: 14 })
// same as computeSma(ticks, { period: 14 })
```

Supported indicators: `"sma"`, `"rsi"`, `"bollinger"`, `"cross"`.

---

### CEL Rule Evaluation

tflo-wasm also bundles the CEL (Common Expression Language) rule engine for filtering and classifying data points.

```typescript
evaluateRules(rules, items)
// → { item_id: string, matched_rules: string[] }[]
```

**`rules`**: An object with a `rules` array:

```typescript
const rules = {
  rules: [
    { name: "hot", condition: "value > 80", action: { type: "alert" } },
    { name: "cold", condition: "value < 20", action: { type: "log" } },
  ],
};
```

**`items`**: Array of items with typed fields:

```typescript
const items = [
  { id: "a", value: 85, level: "high" },
  { id: "b", value: 50, level: "medium" },
  { id: "c", value: 10, level: "low" },
];

const results = evaluateRules(rules, items);
// → [
//     { item_id: "a", matched_rules: ["hot"] },
//     { item_id: "b", matched_rules: [] },
//     { item_id: "c", matched_rules: ["cold"] },
//   ]
```

CEL supports boolean (`bool`), integer (`int`), float (`double`), and string (`string`) fields. Nested objects and arrays are partially supported.

---

### YAML rules

```typescript
evaluateRulesFromYaml(rulesYaml, items)
```

Takes rules as a YAML string instead of a JSON object.

---

## Architecture

```
┌──────────────┐     ┌──────────────────────┐     ┌─────────────────┐
│  TypeScript  │ ──▶ │  JSON-in/JSON-out    │ ──▶ │  tflo-core      │
│  Your code   │     │  tflo-wasm bridge    │     │  (CompiledGraph) │
│              │ ◀── │  (wasm-bindgen FFI)  │ ◀── │  (indicators)   │
└──────────────┘     └──────────────────────┘     └─────────────────┘
```

The design is intentionally simple:
- **No complex TypeScript types** cross the wasm boundary — just JSON strings.
- **No manual memory management**: `wasm-bindgen` handles serialization.
- **No server required**: Everything runs in the browser via WebAssembly.

## Performance

- SMA(20) on 10,000 ticks: ~2ms
- RSI(14) on 10,000 ticks: ~3ms
- Bollinger(20, 2) on 10,000 ticks: ~4ms

All indicators are computed via the same `CompiledGraph` engine used in production Rust deployments.

## Building from source

```bash
# Install wasm-pack
cargo install wasm-pack

# Build the wasm package
wasm-pack build tflo-wasm --target web

# Output goes to tflo-wasm/pkg/
```

## Publishing

```bash
cd tflo-wasm/pkg
npm publish
```

## License

MIT OR Apache-2.0