# tflo

**tflo** (*temporal flow*) is an embeddable temporal event-processing
engine for domain-driven applications. You describe streaming
temporal analysis — windowing, statistics, signal detection,
lifecycle events — as a Rust computation graph; tflo runs it
in-process, at the edge, in a browser, or alongside the cluster
engine you already operate.

Use it from **Rust** (as a crate) or **TypeScript / Node** (a port
of the same engine).

> **Status:** experimental, pre-1.0. The API will change. Not yet
> published to crates.io.

---

## What tflo is good at, standalone

tflo's primary story is the one no JVM CEP engine can tell. If
your need matches one of these, tflo runs alone:

- **Edge gateways under 1 GB RAM.** A 5–10 MB Rust binary in tens
  of MB of resident memory. ARM, x86_64, musl, glibc — all
  first-class. No JVM warmup, no GC pauses.
- **Browser / WASM.** `tflo-core` and `tflo-ops` compile clean for
  `wasm32-unknown-unknown`. Run signal detection live inside a
  dashboard, an extension, or a service worker.
- **Embedded in a Rust service.** If your service is already
  tonic, axum, or actix, tflo composes via `Iterator::tflo(...)`
  inside the process you have. No sidecar, no IPC, no separate
  cluster.
- **Same code for batch and streaming.** `Iterator::tflo(...)`
  runs identically on `Vec<Event>` for historical backfill and on
  `Stream<Event>` for live. The detectors don't know or care
  which it is.
- **Typed missing values.** tflo replaces NaN-as-sentinel with
  `Computed = Result<f64, Absent>` — explicit reasons like
  `WarmingUp`, `DivideByZero`, `FilteredOut`, `DomainError`.
  Downstream code branches on the *reason*, not on a single
  poisoned float.
- **Typed signals.** Detectors emit `Signal<Mode, Payload>`,
  cleanly factored: the *mode* says what happened (Rising,
  Entered, Runt, TooLong), the *payload* carries the data. Pattern
  matching, filtering, and routing are first-class on the mode.
- **No deployment infrastructure.** For single-process and
  Kafka-sharded shapes, you operate nothing beyond your existing
  service. No JobManager, no checkpoint cluster, no separate state
  backend.

### Quick start

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

The same code runs on a finite `Vec<Detection>` (backfill) and on
a tokio `Stream<Detection>` (live). The detectors, windowing, and
typed-absence semantics are identical in both.

### What tflo does in this shape

- **Windowing** — count- and time-based windows over
  irregularly-timed events.
- **Streaming statistics** — moving averages, variance (Welford),
  correlation, rank, median.
- **Signal detection** — threshold crossing, hysteresis,
  glitch/debounce, runt, pulse-width, and zone detectors.
- **Keyed execution** — isolated per-key state (per emitter, per
  host, per sensor) with one builder.
- **Outlier & trend ops** — deviation bands, z-score, peak
  decline, rate-of-change.
- **Extensibility** — drop in your own runtime nodes via the
  `CustomNode` trait, with no fork of `tflo-core`.

---

## How tflo composes with Flink, Esper, and Kafka Streams

tflo's secondary story is composability. Where users already
operate a mature CEP or streaming engine, tflo fills the layers
the host can't reach. The framing across these patterns is
**win-win composition**, not substitution.

### tflo at the edge, mature engine in the center

A common shape: tflo runs on sensor gateways and browsers,
emitting typed `Signal` events with already-conditioned data.
Those signals flow into Flink / Esper / Kafka Streams for
cross-source aggregation, enrichment, and storage.

```
[sensor gateways]    [browser dashboards]    [embedded services]
       │                     │                       │
       └── tflo signal emission (typed Signal events) ──┐
                                                        │
                                            [Kafka / NATS / MQTT]
                                                        │
                                                 [Flink / Esper /
                                                  Kafka Streams]
                                                        │
                                                 [warehouse, alerts,
                                                  user-facing apps]
```

What tflo contributes that the host engine cannot: typed
`Absent`, typed `Signal`, deployment in shapes the host can't
reach (browsers, ARM gateways, embedded services).

### tflo as the per-key detector inside a host engine

A second shape: the host engine owns watermarks, exactly-once,
scaling, savepoints. tflo runs *inside* a per-key processing slot
(Flink `KeyedProcessFunction`, Beam `DoFn`, Kafka Streams
`Processor`) and provides the signal logic + typed-absence
semantics that the host doesn't model natively.

```
                    [Flink job]
                        │
                ┌───────┴────────┐
                │  KeyedProcess  │
                │  Function      │  ← Flink owns watermarks,
                │                │     exactly-once, scaling
                │   ┌────────┐   │
                │   │ tflo   │   │  ← tflo owns signal logic,
                │   │ graph  │   │     typed Absent, detector
                │   └────────┘   │     composition
                └────────────────┘
```

The integration crates (`tflo-flink`, `tflo-beam`, `tflo-kstreams`)
are deferred until concrete user demand surfaces; designs are
captured in [`docs/interop-backlog.md`](docs/interop-backlog.md).

### tflo as backfill / replay over historical data

A third shape: the same detectors you run in production also run
against archived events. tflo's `Iterator::tflo(...)` works
identically on `Vec<Event>` and `Stream<Event>`, so a regression
test, a what-if simulation, or a historical re-emit uses the
same code as live.

```rust
// Live
let signals: impl Stream<Item = Signal<_, _>> = live_stream.tflo(|t| { ... });

// Backfill (same closure, same detectors, deterministic output)
let signals: Vec<Signal<_, _>> = historical_events.into_iter().tflo(|t| { ... }).collect();
```

---

## Picking tflo

**tflo is the right tool when:**

- Your deployment shape includes browsers, edge gateways, or
  embedded Rust services where a JVM is not an option.
- You want detector composition and signal emission in-process,
  not behind a network call.
- You need typed-`Absent` correctness for the difference between
  "warming up" and "math domain error" — distinct cases that NaN
  cannot represent.
- You want batch + streaming with literally the same code.
- Your throughput doesn't justify the operational cost of a
  Flink / Beam / Esper cluster.

**It probably isn't the right standalone tool when:**

- You need streaming SQL for analyst self-service.
- Your problem requires cross-region distributed semantics or
  multi-shard event-time joins. Host tflo inside Flink / Beam
  instead.
- You need a mature catalog of pre-built connectors (Debezium
  CDC, dozens of cloud sources). tflo provides Kafka, MQTT,
  Arrow / Parquet today; rich connector catalogs live in the host
  engines.
- Your team is Java-shop and standardizing on JVM tooling is
  more valuable than the deployment-shape benefits.

In both cases, the composition patterns above usually apply:
tflo can still emit typed signals into your existing stack, or
run inside it.

---

## Custom nodes

When the built-in operations aren't enough, implement
[`CustomNode`] and attach it with `Comp::custom_node` /
`custom_node1` — no changes to `tflo-core`:

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

For the recommended composite-algorithm path (no new runtime
nodes — just extension traits over `Comp`), see the crate-level
docs.

---

## Workspace

| Crate | What it is |
|-------|------------|
| `tflo-core` | The temporal event processing engine — computation graph, windowing, signal detectors, keyed execution |
| `tflo-ops` | Operator catalog — detector, statistics, and trend operators shared across domain plugins |
| `tflo-fintech` | Financial technical-analysis indicators (MACD, ADX, ATR, KAMA, …) as a `tflo-ops` plugin |
| `tflo-cel` | CEL expression filtering (stability: see crate docs) |
| `tflo-rhai` | Rhai scripting (stability: see crate docs) |
| `tflo-rego` | OPA / Rego policy filtering (stability: see crate docs) |
| `tflo-state-files` / `tflo-state-s3` | Checkpoint stores |
| `tflo-connect-kafka` | Kafka adapter (reference implementation) |
| `tflo-connect-mqtt` | MQTT adapter (reference implementation) |
| `tflo-sink-influx` | InfluxDB write sink |
| `tflo-arrow` | Arrow / Parquet I/O for batch and replay |
| `tflo-cep` | Closure-based event-pattern matching ("A then B within T", bounded sequences) |
| `tflo-cep-wasm` | WebAssembly bindings for `tflo-cep` — same engine, JS callbacks |
| `tflo-wasm` | WASM bindings for `tflo-ops` / `tflo-fintech` |

Financial indicators are intentionally a *separate* crate:
`tflo-core` is a generic temporal event-processing engine, and
finance is one domain plugin among many.

Companion TypeScript SDK
[`@tflo/events-browser`](https://github.com/matt-cochran/tflo-events-browser)
wraps `tflo-cep-wasm` with a DOM capture layer
(`IntersectionObserver`-backed viewport tracking, throttled
listeners), a pluggable `Sink` interface (Console, Edge, GA4, custom),
and end-of-session flush via `sendBeacon`. ~25 KB gzip total.

Deferred sibling crates with captured designs (built on user
demand) are listed in
[`docs/interop-backlog.md`](docs/interop-backlog.md).

---

## Documentation

- [`docs/deployment-shapes.md`](docs/deployment-shapes.md) —
  embedded, edge, sharded cluster, WASM, batch/replay.
- [`docs/contracts.md`](docs/contracts.md) — the four pluggable
  traits (state store, cursor, shard router, operator).
- [`docs/non-goals.md`](docs/non-goals.md) — what tflo
  deliberately does not do, and why.
- [`docs/interop-backlog.md`](docs/interop-backlog.md) — deferred
  integration designs for Flink, Beam, Kafka Streams, and
  pattern-matching.

## License

Licensed under either of [MIT](LICENSE-MIT) or
[Apache-2.0](LICENSE-APACHE) at your option.
