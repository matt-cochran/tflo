# Core contracts

`tflo-core` is contract-only above the engine: the integrations that
matter (state stores, cursors, shard routers, sources, sinks) are
implemented in separate crates so users can plug in their preferred
clients without taking direct deps on `aws-sdk-s3` / `rdkafka` /
`rumqttc` / `reqwest`. This page is the contributor-facing inventory.

## The four traits

### 1. `AsyncStateStore` — durable per-key state

`tflo_core::state::AsyncStateStore` (feature `async`)

```rust,ignore
#[async_trait]
pub trait AsyncStateStore: Send + Sync {
    async fn save(&self, key: &[u8], snapshot: &StateSnapshot) -> Result<(), String>;
    async fn load(&self, key: &[u8]) -> Result<Option<StateSnapshot>, String>;
    async fn list_keys(&self) -> Result<Vec<Vec<u8>>, String>;
    async fn save_batch(&self, items: &[(Vec<u8>, StateSnapshot)]) -> Result<(), String>;
    async fn delete(&self, key: &[u8]) -> Result<(), String>;
}
```

Reference impls in the workspace:

- `tflo-state-files` (`async` feature): file-backed, runs sync I/O on
  `tokio::task::spawn_blocking`.
- `tflo-state-s3`: direct async impl over the [`S3Client`] trait.

Notes for new impls:

- `save_batch` has a sequential-loop default. Cost-sensitive backends
  (S3, GCS) **must** override to use multi-object batched APIs — per-
  key `PUT` is the most common production cost amplification.
- The `Arc<dyn AsyncStateStore>` blanket impl already exists in core —
  callers can wrap once and pass to anything that accepts the trait.

### 2. `Cursor` — durable stream position

`tflo_core::adapter::Cursor`

```rust,ignore
pub trait Cursor: Clone + Send + Sync + Debug + 'static {
    fn to_bytes(&self) -> Vec<u8>;
    fn from_bytes(data: &[u8]) -> Result<Self, String>;
    fn display(&self) -> String;
}
```

Existing impls:

- `tflo_connect_kafka::KafkaOffset` (`(topic, partition, offset)`)
- `tflo_connect_mqtt::MqttCursor` (last packet id + bounded QoS-2
  dedup window + retained-topic sequence map)

Each cursor type pairs with an `AsyncCursorStore<C>` impl. The pairing
is what the `Checkpointer` orchestrates.

### 3. `ShardRouter<K>` — pluggable key→shard ownership

`tflo_core::shard::ShardRouter`

```rust,ignore
pub trait ShardRouter<K>: Send + Sync {
    fn owns(&self, key: &K) -> bool;
    fn assignment_epoch(&self) -> u64;
}
```

Two impls today:

- `tflo_core::shard::LocalShard` — own every key (single-process
  default; preserves pre-Phase-1 behavior).
- `tflo_connect_kafka::KafkaShardRouter` — driven by consumer-group
  rebalance callbacks.

Notes for new impls:

- The lifecycle hooks (`on_assign` / `on_revoke`) are intentionally
  outside the trait. They are runtime-coupled and live in connector
  crates.
- `assignment_epoch` must be **strictly monotonic** and bump after
  ownership changes. This is what fences out events that arrive in the
  rebalance window for keys you no longer own.

### 4. `Operator` — custom node plugin

`tflo_core::operator::Operator`

```rust,ignore
pub trait Operator: Send + Sync + 'static {
    fn eval(&mut self, inputs: &[Computed], ts: i64) -> NodeOutput;
    fn reset(&mut self) {}
    fn name(&self) -> &str { "operator" }
    fn save(&self) -> Option<Vec<u8>> { None }
    fn load(&mut self, _bytes: &[u8]) -> Result<(), OperatorLoadError> { /*…*/ }
    fn type_id_version(&self) -> u32 { 0 }   // Phase 1 — opt-in versioning
}

pub trait StatelessOperator: Operator {}      // marker for declared-intent
```

`tflo-ops` and `tflo-fintech` are both pure `Operator` plugins. They
are the canonical templates for writing your own catalog crate.

## Auxiliary contracts (in connector crates, not core)

| Trait | Crate | Purpose |
|---|---|---|
| `KafkaConsumer` / `KafkaProducer` | `tflo-connect-kafka` | Wrap any Kafka client — rdkafka backend optional |
| `MqttConsumer` / `MqttProducer` | `tflo-connect-mqtt` | Wrap any MQTT client — rumqttc backend optional |
| `S3Client` | `tflo-state-s3` | Wrap any S3 HTTP/SDK |
| `InfluxHttpClient` | `tflo-sink-influx` | Wrap any HTTP client for Influx writes |

## Writing a new integration crate

Follow the `tflo-fintech` / `tflo-connect-mqtt` layout:

```
tflo-<your-crate>/
  Cargo.toml
  src/
    lib.rs          // traits, types, glue
    prelude.rs      // re-exports
    <backend>.rs    // optional concrete backend, feature-gated
```

Cargo.toml conventions:

- `[features].default = []` — keep the crate dep-free by default.
- `async = ["tflo-core/async", "dep:tokio", ...]` if your contracts
  need an async runtime.
- Concrete backends behind their own feature (e.g.
  `rdkafka-backend`, `rumqttc-backend`) so the trait surface is
  always usable in tests without system deps.

Workspace updates:

1. Add the crate to `members` in the root `Cargo.toml`.
2. Add it to `workspace.dependencies` with a `path = "..."` entry.

Tests:

- The trait surface should be testable end-to-end via in-process mock
  impls, with no external services required.
- Integration tests against real backends go behind `#[ignore]` and a
  `docker-compose.yml` recipe.

## Crash-safe orchestration: `Checkpointer`

The Phase 1 `tflo_core::state::Checkpointer` is the single place that
sequences the snapshot → state-store-save → cursor-store-save writes
in the correct order. If you're building a new connector that needs
checkpointing, use it rather than re-implementing the ordering —
getting the snapshot/cursor write order wrong is the most common cause
of duplicate processing on restart.
