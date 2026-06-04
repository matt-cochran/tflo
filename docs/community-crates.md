# Community-crate slots (post-1.0)

The Phase-1 contracts in `tflo-core` are designed so the integrations
below can be added as separate crates without any change to core. They
were considered for the v1.0 first-party deliverables and demoted to
"named community slots" because the per-integration work is dominated
by the protocol/client layer, not by anything tflo-specific.

If you want to add one, [`docs/contracts.md`](contracts.md) is the
contract reference. The pattern matches `tflo-connect-mqtt` /
`tflo-connect-kafka`: a thin trait surface in `lib.rs`, optional
concrete backend behind a Cargo feature, in-process mock for tests.

## `tflo-connect-opcua`

Industrial-IoT source for OPC-UA SCADA systems.

- Client: the `opcua` crate (Rust port of Eclipse Milo).
- Cursor: subscription sequence numbers per
  `(server_url, subscription_id)`.
- Source: subscription change-notification stream → `Stream<Item =
  OpcuaTagValue>`.
- Sink: not typically needed (write-back to SCADA is rare).

## `tflo-connect-modbus`

Polling source for Modbus TCP / RTU devices.

- Client: `tokio-modbus`.
- Cursor: per-device poll timestamp (best-effort — Modbus has no
  intrinsic monotonic position).
- Source: a configurable polling loop yielding
  `Stream<Item = ModbusRegisterRead>`.

## `tflo-connect-redis`

Redis Streams source / sink.

- Client: `redis` crate with tokio feature.
- Cursor: Redis stream IDs (e.g., `1700000000000-0`).
- Source: `XREAD` driven `Stream`.
- Sink: `XADD`.

## `tflo-connect-nats`

NATS JetStream source / sink.

- Client: `async-nats`.
- Cursor: consumer sequence number.
- Source: durable-consumer pull subscription → `Stream`.
- Sink: JetStream publish.

## `tflo-sink-timescale`

TimescaleDB (PostgreSQL) sink.

- Demoted from first-party because the right answer is usually "use
  `sqlx::query` with `COPY` and call it a day" — about 30 lines of
  user code. A first-party crate would mostly be docs.
- If a crate is wanted: `sqlx` + `COPY ... FROM STDIN BINARY`, with a
  small `TimescaleBatcher` mirroring [`tflo_sink_influx::Batcher`]'s
  bounded-buffer + threshold-flush pattern.

## Out of scope (probably never)

- Cross-shard joins / fold across keys. Re-key through the message bus;
  documented pattern.
- Web admin UI / management plane — `tflo-site` is marketing-only.
- mTLS between workers — assume a trusted network / service mesh.
- WASM workers with sharded state — wasm32's 4 GiB cap kills the use
  case before tflo's contracts get in the way.

## Want to claim a slot?

Open an issue on the GitHub mirror with the crate name. The repo's
license is MIT/Apache-2.0 either way — third-party crates outside this
workspace need no permission, just use the contracts directly.
