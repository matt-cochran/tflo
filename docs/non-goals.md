# Non-goals

This document records what `tflo` deliberately does **not** do, and
why. It complements `docs/deployment-shapes.md` (what `tflo` *does*
do, in what shapes).

A feature request that conflicts with one of these entries does not
necessarily mean "no" — it means "explain how this change is
compatible with the stated reasoning, or argue why the reasoning
should change." The PR template (`.github/PULL_REQUEST_TEMPLATE.md`)
asks contributors to address this explicitly.

## Streaming SQL

`tflo`'s public interface is a Rust DSL (`Iterator::tflo(...)`).
That choice is load-bearing for the project's positioning:

- A SQL parser is a large dependency that would inflate WASM
  binaries and the operational surface.
- Type-checking happens at Rust compile time today, not at deploy
  time. SQL would push validation back to runtime.
- The DSL composes naturally with other Rust async/iterator code;
  SQL would introduce a foreign-language boundary in every
  pipeline.

Engines that lead with SQL (Flink SQL, ksqlDB, RisingWave) are the
right tool when SQL is the requirement. `tflo` is not in that lane.

## Distributed runtime / job manager

`tflo` does not implement a JobManager / TaskManager / scheduler
layer of its own. The supported scale-out path is Kafka consumer
groups via `KafkaShardRouter` — i.e., the host's existing Kafka
infrastructure provides partitioning, ownership transfer, and
rebalance.

Building a tflo-specific cluster runtime would compete with Flink,
Spark Streaming, and Beam-on-Dataflow on their home turf. tflo's
value prop (embeddable, edge, WASM) does not depend on that
machinery and would be eroded by the operational complexity it
introduces.

For users whose distributed-runtime needs exceed
Kafka-consumer-group sharding, the supplement pattern applies:
host tflo inside Flink (or Beam, or Kafka Streams) and let the
host own scheduling, scaling, and savepoints. See
`docs/interop-backlog.md`.

## Exactly-once via two-phase commit

End-to-end exactly-once delivery requires either (a) transactional
sinks coordinated with checkpoint barriers (Flink's approach), or
(b) idempotent sinks with at-least-once delivery and deduplication.

`tflo` commits to (b). The `Deduplicator<K>` primitive (Phase 3e of
the closure plan) makes idempotent-sink behavior a first-class
helper rather than per-user prose. Two-phase commit lives in the
host engine when the user needs it; tflo does not re-implement it.

## Cross-shard / global watermark aggregator

`tflo`'s watermark is intentionally per-key. Cross-key alignment
("emit the joint signal only after every sensor has reported up
to T") is a host-framework concern. Implementing a global
watermark coordinator inside tflo would couple it to a
shared-coordination primitive that does not fit the embeddable +
WASM positioning.

When cross-key event-time alignment is required, the user hosts
tflo inside an engine that provides it (Flink, Beam).

## Savepoints distinct from checkpoints

Flink distinguishes *checkpoints* (periodic, machine-managed) from
*savepoints* (manual, human-named, used for deploys and disaster
recovery). `tflo` provides only checkpoints today.

A savepoint API can be added later if a real user need surfaces; it
would be a thin layer over `Checkpointer::commit` with a
named-key convention. Not a current goal.

## Processing-time mode

`tflo` is event-time-only. Watermarks advance based on input
timestamps, never wall-clock. This is a feature, not a gap: the
typed `Computed = Result<f64, Absent>` semantics depend on
deterministic, event-time-driven evaluation. Adding a
processing-time mode would create two parallel semantic universes
in one engine — a maintenance burden with no compensating user
benefit for the deployment shapes tflo targets.

## `no_std` / bare-metal MCU

Listed in `docs/deployment-shapes.md` as aspirational. The current
codebase depends on `std` collections (`HashMap`, `BinaryHeap`,
`Vec`) in keyed execution; lifting that requires a parallel
`no_std`-compatible storage layer. Not a current commitment.

Revisit only if a credible MCU use case surfaces with a contributor
prepared to do the storage-layer work.

---

## Reasoning meta-principle

These non-goals are intentional scope choices, not features waiting
to be built. Each one corresponds to a deployment shape or
architectural property that tflo deliberately optimizes *against*
in favor of its primary value prop (embeddable, edge, WASM,
typed signals).

If a proposed change conflicts with one of these, ask whether the
change makes the *primary* value prop stronger or weaker. If it
strengthens an adjacent value prop (e.g., distributed semantics)
at the cost of the primary one, the right place for that work is
in a sibling integration crate, not in `tflo-core`.
