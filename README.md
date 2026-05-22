# tflo

**tflo** (*temporal flow*) is a temporal event processing engine for
domain-driven applications. Bring your domain events; layer streaming temporal
analysis — windowing, statistics, signal detection, lifecycle events — on top.

Use it from **Rust** (as a crate) or **TypeScript / Node** (a port of the same
engine).

> **Status:** experimental, pre-1.0. The API will change. Not yet published to
> crates.io.

## What it is

Many systems produce a stream of *domain events* — RF signature matches,
intrusion-detection alerts, sensor readings, transactions. The raw stream is
noisy. tflo turns it into clean, higher-level **lifecycle events**: *appeared*,
*persisted*, *changed*, *dropped out*, *crossed a threshold*.

You describe the analysis as a declarative computation graph over an
`Iterator` or `Stream`, and tflo compiles it into a stateful executor with
first-class support for:

- **Windowing** — count- and time-based windows over irregularly-timed events.
- **Streaming statistics** — moving averages, variance (Welford), correlation,
  rank, median.
- **Signal detection** — threshold crossing, hysteresis, glitch/debounce,
  runt, pulse-width, and zone detectors.
- **Keyed execution** — isolated per-key state (per emitter, per host, per
  sensor) with one builder.
- **Outlier & trend ops** — deviation bands, z-score, peak decline,
  rate-of-change.
- **Extensibility** — drop in your own runtime nodes via the `CustomNode`
  trait, with no fork of `tflo-core`.

## Quick start

```rust
use tflo_core::prelude::*;

#[derive(Clone)]
struct Detection {
    ts: i64,
    confidence: f64,
}

let detections: Vec<Detection> = /* ... */;

// Smooth a noisy confidence stream, then flag threshold crossings.
let events: Vec<ThresholdCrossEventMode> = detections
    .into_iter()
    .tflo(|t| {
        t.timestamp(|d| d.ts);
        let confidence = t.prop(|d| d.confidence);
        let smoothed = confidence.sma(5_u64.secs());
        let threshold = t.constant(0.8);
        smoothed.cross(&threshold)
    })
    .collect();
```

## Custom nodes

When the built-in operations aren't enough, implement [`CustomNode`] and attach
it with `Comp::custom_node` / `custom_node1` — no changes to `tflo-core`:

```rust
use tflo_core::custom_node::CustomNode;

struct RunningPeak { peak: f64 }

impl CustomNode for RunningPeak {
    fn eval(&mut self, inputs: &[f64]) -> f64 {
        let v = inputs.first().copied().unwrap_or(f64::NAN);
        self.peak = self.peak.max(v);
        self.peak
    }
}
```

## Workspace

| Crate | What it is |
|-------|------------|
| `tflo-core` | The temporal event processing engine — computation graph, windowing, signal detectors, keyed execution |
| `tflo-fintech` | Financial technical-analysis indicators (MACD, ADX, ATR, KAMA, …) as a `tflo-core` plugin |
| `tflo-cel` | CEL expression filtering |
| `tflo-rhai` | Rhai scripting |
| `tflo-rego` | OPA/Rego policy filtering |
| `tflo-state-files` / `tflo-state-s3` | Checkpoint stores |
| `tflo-connect-kafka` | Kafka adapter (reference implementation) |

Financial indicators are intentionally a *separate* crate: tflo-core is a
generic temporal event processing engine, and finance is one domain plugin
among many.

## Use cases

The "noisy detections in → clean lifecycle events out" pattern applies to:

- **RF spectrum monitoring** — signal lifecycle, Doppler range-rate, direction change
- **Intrusion detection** — collapsing noisy alerts into incidents
- **Observability** — de-flapping alerts (a hysteresis problem)
- **Fraud & anomaly** — streaming outlier detection
- **IoT** — sensor conditioning and event detection

## License

Licensed under either of [MIT](LICENSE-MIT) or
[Apache-2.0](LICENSE-APACHE) at your option.
