# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased — Phases 2–6 connectors, lint cleanup, reference deployment] — 2026-05-23

Same day as the Phase 1 contracts cut; the rest of the production
roadmap landed end-to-end. Summary by phase:

### Phase 2 — Kafka connector (`tflo-connect-kafka` rewrite)

- `KafkaConsumer` / `KafkaProducer` async traits + `RebalanceEvent`,
  `TopicPartition`, `KafkaMessage` types.
- `KafkaShardRouter` — `ShardRouter` impl with a **required**
  `AsyncStateStore` constructor parameter (the compile-time poka-yoke
  against running sharded execution with no durable state).
  `apply_rebalance` bumps `AssignmentEpoch` monotonically;
  `events_dropped_stale_epoch` counter exposed.
- `InMemoryCursorStore` now implements both sync `CursorStore` and
  async `AsyncCursorStore` over the same backing data.
- Optional `rdkafka-backend` feature: thin wrapper around
  `rdkafka::StreamConsumer` / `FutureProducer`.

### Phase 3 — MQTT connector (new crate `tflo-connect-mqtt`)

- `BoundedSet` — fixed-capacity insertion-ordered set for QoS-2 dedup,
  the poka-yoke against unbounded edge-side memory growth.
- `MqttCursor` (implements `Cursor`) with bounded `qos2_inflight_window`;
  constructor refuses windows above `MAX_QOS2_WINDOW_SIZE` (64 KiB).
- `MqttConsumer` / `MqttProducer` async traits; `MqttMessage` /
  `MqttPublish` / `Qos` types.
- Optional `rumqttc-backend` feature; pure-Rust client, cross-compiles
  to ARM / WASM without C deps.

### Phase 4 — Influx sink + Arrow/Parquet/Polars (new crates)

- `tflo-sink-influx`: `LineProtocol` builder (tags, fields, escaping per
  spec), `Batcher` with soft flush threshold + hard `max_buffer_bytes`
  bound, pluggable `InfluxHttpClient` trait (no direct `reqwest`/`hyper`
  dep).
- `tflo-arrow`: `schema_fingerprint()` (columnar analog of
  `Builder::fingerprint`); `parquet` feature with `parquet_io::{write,
  read}_batches`; `polars` feature with `polars_interop` helpers.

### Phase 5 — Lint backlog batches 4 + 5 (annotated, not rewritten)

All remaining `pedantic` / `nursery` lints in `docs/lint-backlog.md` are
resolved as **permanent `allow`** entries with rationale comments in
`[workspace.lints.clippy]`. The `tflo-fintech` golden-fixture
bit-equality suite is the actual safety net against numeric drift;
trying to rewrite `mul_add` paths would break it.

### Phase 6 — Reference deployment + docs

- `tflo-examples/examples/iot-portal/main.rs`: end-to-end deployment
  exercising MQTT consume → conditioning → Kafka publish → central
  worker consume → Influx write → Parquet archive, with in-process mock
  client implementations so the example runs in CI without external
  services. Demonstrates `Checkpointer` ordering, `KafkaShardRouter`
  rebalance + epoch bump, and `schema_fingerprint`.
- `docs/deployment-shapes.md`: the six concrete deployment shapes
  `tflo` covers, with caveats per shape.
- `docs/contracts.md`: contributor-facing inventory of the four core
  contracts (`AsyncStateStore`, `Cursor`, `ShardRouter`, `Operator`)
  plus the connector-crate auxiliary traits.
- `docs/community-crates.md`: named slots for OPC-UA, Modbus, Redis,
  NATS, Timescale community crates.

### Ergonomic additions (made along the way)

- `Arc<T>` blanket impls for `AsyncStateStore`, `AsyncCursorStore<C>`,
  and `InfluxHttpClient` so dyn-erased trait objects pass into generic
  constructors without manual wrappers.

### Verification at end of roadmap

- `cargo clippy --workspace --all-targets -- -D warnings` clean.
- 42 test result groups pass (started Phase 0 at 36; +20 new tests
  added by Phases 1–4).
- Golden 55/55 intact.
- `cargo run --example iot-portal` completes the full edge → cluster →
  sink → archive flow.

---

## [Unreleased — Phase 1 contracts] — 2026-05-23

The first leg of the production roadmap (see plan file in
`/home/mc/.claude/plans/`). Adds the load-bearing contracts that downstream
phases (Kafka connector, MQTT connector, distributed state) depend on.
**Additive** — existing `Operator` impls and the sync `StateStore` trait
continue to work unchanged.

### Added — `tflo-core`

- **`tflo_core::state` module (feature `async`)** — new async-first state
  store and crash-safe checkpoint orchestrator.
  - `AsyncStateStore` trait — `async fn save / load / list_keys / save_batch /
    delete`. The recommended path for new backends (S3, Redis, network
    stores). `save_batch` has a sequential-loop default; cost-sensitive
    backends should override to use multi-object batched APIs.
  - `AsyncCursorStore<C: Cursor>` trait — cursor-side companion.
  - `Checkpointer<C, S, X>` — orchestrator that writes **snapshot first,
    cursor last** (the crash-safe order). Mandatory per-stage deadline,
    consecutive-failure circuit breaker. Atomic counters expose
    `commits_total` / `failures_total` / `timeouts_total` for scraping.
  - `CheckpointError` enum: `Timeout`, `StateStore`, `CursorStore`,
    `SnapshotCapture`, `CircuitOpen`.
- **`tflo_core::shard` module** — pluggable key→shard ownership.
  - `ShardRouter<K>` trait — `owns(&K) -> bool` + `assignment_epoch() -> u64`.
    Lifecycle hooks (`on_assign`/`on_revoke`) intentionally live in
    connector crates — they are runtime-coupled.
  - `LocalShard` — default impl that owns every key (preserves single-process
    behavior).
  - `AssignmentEpoch` — monotonic counter for fencing stale events through
    rebalance windows.
- **`Builder::fingerprint() -> [u8; 32]`** — topology hash of the
  computation graph (node count + per-node kind/name). Stamped into
  snapshot metadata when set; on restore a mismatch is rejected with
  `ComputeError::InvalidInput`. Poka-yoke for silent version skew across
  workers.
- **`Operator::type_id_version() -> u32`** — opt-in versioning for stateful
  operators. Default `0` keeps all ~30 existing impls compiling.
- **`StatelessOperator` marker trait** — declaration of intent for operators
  with no checkpointable state.
- **`CompiledGraph::with_topology_fingerprint(&self, [u8; 32]) -> Self`** —
  setter; `snapshot()` automatically embeds the fingerprint and `restore()`
  verifies it.
- **`SnapshotError::Unsupported { index, kind }`** — typed error instead of
  silent `None` for non-snapshottable nodes (`scan`/`scan2`/plugin without
  `save()`). Surfaced via `NodeState::to_snapshot(index)`.

### Added — `tflo-state-files`

- New feature flag `async` adds an `AsyncStateStore` impl that runs file I/O
  on `tokio::task::spawn_blocking`. Both sync `StateStore` and async paths
  coexist.

### Changed — `tflo-state-s3` (breaking, behind feature)

- **Rewritten** as a direct `AsyncStateStore` implementation. The
  `tokio::runtime::Handle::try_current().block_on(...)` hack is **gone**.
- `S3Client` trait gains a `delete_object` method (with a no-op default for
  back-compat).
- The synchronous `StateStore` impl is **removed** from `S3StateStore`.
  Callers that used the sync path must switch to the async API (see the
  `docs-s3-checkpoint` example) or pin `tflo-state-s3 = "0.1"`.

### Changed — `SnapshotMetadata`

- New optional field `topology_fingerprint: Option<[u8; 32]>`.
  `#[serde(default)]` makes existing on-disk snapshots load fine. Struct
  constructors must supply the field (or `..Default::default()`).
- `SnapshotMetadata` now derives `Default`.

### Deliberately deferred from the original Phase 1 plan

- **Time feature flags (`time-tokio`/`time-embassy`/`time-wasm`)** —
  deferred because `tflo-core` does not actually take a time source today.
- **`Runtime` trait** — `tflo-core` does not spawn or schedule; the cost of
  abstracting it is not yet earned.
- **`Source` / `Sink` traits in core** — `futures::Stream` and
  `futures::Sink` already cover this surface.
- **Removing default `Operator::save`/`load`** — would break ~30 existing
  impls. The fingerprint poka-yoke at the builder level provides the actual
  safety net.

## [Unreleased — 2026-05-22] — tflo-ops split

tflo is pre-1.0 and has not been published to crates.io; the API is unstable.

### Added — `tflo-ops` crate (operator catalog)

- **New `tflo-ops` crate** extracts the full operator catalog from `tflo-core`.
  `tflo-ops` now owns all windowed aggregations (`sma`, `ema`, `std`, `var`,
  `wma`, `median`, `quantile`, `skewness`, `kurtosis`, `correlation`,
  `covariance`), statistical operators, stateful trackers (`prev`, `lag`,
  `cumulative_*`, `pct_change`, `log_return`, `zscore`, `rate_of_change`,
  `momentum`, `peak_decline`), event detectors (`cross`, `cross_above`,
  `cross_below`, `glitch`, `runt`, `pulse_width`, `window_detector`, zone
  ops), math and composite operators. `tflo-core` is now engine-only: record
  sources, closure transforms, the `Operator` plugin trait, keyed execution,
  and checkpointing.

- **Unified `Operator` plugin trait** supersedes `CustomNode`. New signature:
  `fn eval(&mut self, inputs: &[Computed], ts: i64) -> NodeOutput`. Type
  renames: `BoxedCustomNode → BoxedOperator`, `CustomNodeFactory →
  OperatorFactory`, `CustomNodeLoadError → OperatorLoadError`. All catalog
  operators in `tflo-ops` are implemented via this trait.

- **`NodeOutput`** is the public, renamed-from-`Value` engine output type.
  Supports both the `f64`-or-`Absent` hot path (`NodeOutput::Computed`) and
  typed (non-`f64`) outputs via `NodeOutput::Other(Box<dyn Any>)`.

### Changed — migration guide

- **Restore all catalog methods** with `use tflo_ops::prelude::*;` — this
  brings `price.sma(20)`, `price.cross_above(&threshold)`, etc. back into
  scope.
- **Primitives moved**: `tflo_core::primitives::X` →
  `tflo_ops::primitives::X` (e.g. `WelfordWindow`, `CrossDetector`,
  `GlitchFilter`, `RuntDetector`, `LagBuffer`, …).
- **Event types moved**: `tflo_core::event::{GlitchResult, RuntResult,
  PulseWidthResult, WindowEvent}` → `tflo_ops::events::*`. The
  `ThresholdCrossEventMode` in `tflo_ops::events` is a distinct type from
  the `tflo_core::event::ThresholdCrossEventMode` (which backs `Signal<TMode>`
  and `EdgeSignal` in the engine).
- **Div / DivConst zero-divisor behaviour**: arithmetic `Div` and `DivConst`
  operators now produce `f64::INFINITY` or `f64::NAN` on a zero divisor; the
  downstream `finite_or_warming` helper turns those into `Absent::WarmingUp`.
  The older typed `Absent::DivideByZero` path is no longer emitted by these
  operators.

- **`tflo-core/src/wasm.rs` removed.** The JSON-in/JSON-out indicator bridge
  (`compute_sma`, `compute_rsi`, `compute_ema`, `compute_bollinger`,
  `compute_macd`, `detect_cross`, `compute_indicator`) moved from
  `tflo-core/src/wasm.rs` into `tflo-wasm/src/lib.rs`, which now imports
  `tflo-ops` and `tflo-fintech` directly. This eliminates the wasm32 build
  failure caused by the wasm module calling catalog methods (`.sma()`,
  `.rsi()`, etc.) that moved to `tflo-ops` during the split. The unused
  `wasm` feature flag was also removed from `tflo-core/Cargo.toml`.

---

## [Unreleased]

tflo is pre-1.0 and has not been published to crates.io; the API is unstable.

### Changed — reoriented as a temporal event processing engine

- `tflo-core` is now a generic temporal event processing engine. Financial
  technical-analysis indicators moved out into a new **`tflo-fintech`** crate.
- Dual-use operations were renamed to domain-neutral names in `tflo-core`:
  `bollinger_bands` → `deviation_band`, `drawdown` → `peak_decline`,
  `roc_n` → `rate_of_change`, `mom_n` → `momentum`. `tflo-fintech` re-exports
  the finance-named aliases via the `FintechAliases` trait.
- **Breaking:** finance indicators (`macd_n`, `adx_n`, `stochastic_n`, …) now
  require `use tflo_fintech::prelude::*`.

### Hardened — pre-open-source quality pass

- **Typed absence model.** A node's per-record output is now a `Computed`
  (`Result<f64, Absent>`) — a finite value, or a typed reason it is absent
  (`WarmingUp`, `DivideByZero`, `DomainError`, `FilteredOut`, …) — replacing
  the opaque `f64::NAN` sentinel. **Breaking:** `CustomNode::eval` takes
  `&[Computed]` and returns `Computed`, and `StepResult::WarmingUp` carries a
  `reason`. `O = f64` callers are unaffected — absence still flattens to
  `NaN`; use `O = Computed` to observe the reason.
- **Panic-freedom.** Production `unwrap`/`expect`/`panic!`/`unreachable!`
  sites were removed and locked out by `deny`-level clippy lints
  (`unwrap_used`, `expect_used`, `panic`, `unreachable`, `todo`); test code is
  exempt. Calibration constructors gained a total, clamping `new` plus a
  fallible `try_new`. **Breaking:** `Comp::custom_node` now takes
  `(first, rest, factory)` so the "at least one input" rule is enforced by the
  type system instead of an `assert!`. The `release` profile now enables
  `overflow-checks`.
- **Working `snapshot()` / `restore()`.** Checkpointing now serializes full
  per-node state (window buffers, accumulators, detector state machines) with
  `postcard`, not just metadata. `snapshot()` returns a `Result` and rejects
  any graph it cannot fully capture (a `scan`/`fold` node, or a `CustomNode`
  that does not implement the new optional `save`/`load`).
- **`OutOfOrderPolicy::Buffer` implemented.** Previously a no-op that
  processed records in arrival order; it now buffers within the lateness
  window, releases records on an advancing watermark, and flushes any
  remainder at end-of-stream.
- **`validated()` enforces every option.** All eight `ValidationOptions`
  fields — `reject_nan`/`reject_inf`, `error_on_nan`/`error_on_inf`/
  `error_on_negative`, `min_warmup`, `max_gap_ms`, and `assert_sorted` — are
  now checked; previously only `assert_sorted` was. New error variant
  `TFloError::TimestampGapExceeded`.
- **CI & lints.** Added a `rustfmt --check` step and a `-D warnings` clippy
  gate; declared an MSRV (`rust-version = "1.85"`). `clippy::pedantic` and
  `clippy::nursery` are temporarily suppressed with a documented
  re-enablement backlog (`docs/lint-backlog.md`).

### Added

- `CustomNode` trait plus `Comp::custom_node` / `custom_node1`: external crates
  can contribute runtime graph nodes without modifying `tflo-core`.
- `tflo-fintech` crate: technical-analysis indicators as a plugin, validated
  bit-exact against the TA-Lib C library via a golden-vector suite.

### Removed

- The unwired `NodeBehavior` trait (superseded by `CustomNode`).

### Performance

- Node outputs are stored in a typed `Value` (`f64` held inline), eliminating
  a heap allocation per node per record on the f64 hot path.

### Workspace crates

`tflo-core`, `tflo-fintech`, `tflo-cel`, `tflo-rhai`, `tflo-rego`,
`tflo-state-files`, `tflo-state-s3`, `tflo-connect-kafka`, `tflo-wasm`,
`tflo-examples`.
