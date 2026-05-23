# Deployment shapes

`tflo` is one engine that's intended to cover six concrete deployment
shapes. This doc names each, points at the example or crate that
demonstrates it, and lists the caveats that matter in production.

## 1. Embedded in a Rust service (single-process)

The original use case. `tflo-core` runs inline in the user's service;
records flow through `Iterator::tflo(...)` or `Stream::tflo(...)`; no
external runtime is involved. Keyed execution still works
single-process via `LocalShard` (the default `ShardRouter` impl in
`tflo_core::shard`).

**Try it:** every example under `tflo-examples/examples/` *except*
`iot-portal` runs in this mode.

**Caveats:** none specific to this shape.

## 2. Edge gateway (Linux / ARM, tokio + std)

A single-process Rust binary running on a sensor gateway. Consumes
sensor input (MQTT, OPC-UA, Modbus, vendor protocol), conditions it
(SMA, hysteresis, debounce), and republishes lifecycle events upstream
(Kafka, NATS, or directly to a TSDB sink).

- Engine: `tflo-core` (default features, async)
- Source: `tflo-connect-mqtt` (`rumqttc-backend` feature) — pure Rust,
  cross-compiles cleanly to `armv7-unknown-linux-gnueabihf`
- Sink: typically the same MQTT crate's producer, or `tflo-sink-influx`
  for direct write
- State: `tflo-state-files` (`async` feature) for crash-safe restart

**Try it:** the *edge gateway* leg of `tflo-examples/examples/iot-portal/`.

**Caveats:** MQTT QoS-2 dedup needs a bounded window (the
[`tflo_connect_mqtt::MqttCursor`] enforces this) — without it, the
in-flight set grows unbounded.

## 3. Central worker cluster (Kafka-sharded)

Multiple worker processes each owning a slice of the keyspace driven by
Kafka consumer-group rebalancing. State per partition is durable in an
`AsyncStateStore` (S3, Postgres, file, …) so a worker that loses a
partition during rebalance can have it restored elsewhere.

- Engine: `tflo-core` (async feature)
- Source: `tflo-connect-kafka` (`rdkafka-backend` feature for production)
- Shard router: `tflo_connect_kafka::KafkaShardRouter` — the
  `AsyncStateStore` argument is **required** at construction (a
  compile-time poka-yoke against running sharded execution with no
  durable state)
- State: `tflo-state-files` for dev / `tflo-state-s3` for production
- Sink: `tflo-sink-influx`, `tflo-arrow` for Parquet archive, or
  back-to-Kafka for further processing
- Orchestration: `tflo_core::state::Checkpointer` — writes snapshot
  first, cursor last (the crash-safe order), per-stage deadline, and a
  consecutive-failure circuit breaker.

**Try it:** the *central worker* leg of `tflo-examples/examples/iot-portal/`.

**Caveats:**
- At-least-once delivery semantics; sinks should be idempotent. End-to-
  end exactly-once via Kafka transactions is documented as future work.
- The router's `AssignmentEpoch` must be re-read on every event intake
  for the rebalance-race fence to work — see
  [`tflo_core::shard::AssignmentEpoch`].

## 4. WASM (browser / edge runtime)

`tflo-core` and `tflo-ops` already build clean for
`wasm32-unknown-unknown`; `tflo-wasm` exposes the streaming detectors
to JS. For larger graphs in this shape:

- Mind the wasm32 4 GiB per-instance memory cap. Per-key state grows
  with key cardinality — set a `KeyedConfig::max_active_keys` (LRU
  eviction to a snapshot store) when key count is unbounded.
- Time source: there's no `Runtime` trait; the engine only needs a
  wall-clock for `SystemTime::now()` in snapshot metadata, which is
  already guarded for wasm32.

**Try it:** `tflo-wasm` builds with `wasm-pack build --target web`.

## 5. no_std microcontroller

Not fully supported today — `tflo-core` depends on `std`. The Phase 1
roadmap notes this as a deferred goal (the original `Runtime` trait
removal narrowed the surface, but `std` collections in keyed execution
remain). Status: aspirational; track the roadmap.

## 6. Batch / replay over historical data

Run the same temporal-graph code over a bounded historical dataset.
The Iterator path supports this natively — feed any
`impl Iterator<Item = Record>` into `.tflo(...)`.

- Format: Parquet via `tflo-arrow` (`parquet` feature). Each
  `RecordBatch` row becomes a tflo record.
- Schema safety: stamp `tflo_arrow::schema_fingerprint(&schema)` into
  the resulting snapshot's `SnapshotMetadata.topology_fingerprint`; the
  Phase 1 fingerprint poka-yoke refuses a backfill against a
  structurally-different schema.

**Try it:** the `tflo-arrow` `parquet_io::{write_batches, read_batches}`
helpers; the `iot-portal` example builds a `RecordBatch` to show the
schema-fingerprint flow.

---

## Picking a shape

| Question | Shape |
|---|---|
| Single Rust process, no external deps? | **1. Embedded** |
| Linux/ARM gateway with sensor protocols? | **2. Edge gateway** |
| Need horizontal scale + restartable durable state? | **3. Cluster** |
| Browser / WASM-host runtime? | **4. WASM** |
| Bare-metal MCU? | **5. no_std** (aspirational) |
| Backfill or simulation over recorded data? | **6. Batch/replay** |

Shapes 1, 2, and 6 are production-ready today. Shape 3 ships the
contracts + KafkaShardRouter; the real-rdkafka backend is feature-gated
and exercises a redpanda-in-docker integration when the user opts in.
Shape 4 builds; the memory caveat is real. Shape 5 is roadmap.
