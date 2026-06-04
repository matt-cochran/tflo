# Interop backlog

This document captures deferred integrations between `tflo` and
mature CEP / streaming engines (Flink, Apache Beam, Kafka Streams,
Esper). Each entry describes the integration shape and the
standalone value tflo brings to the composition.

**Build discipline:** entries here are **not committed work**. They
are promoted to the closure plan only when one of these is true:

1. A real user explicitly requests the integration.
2. The standalone story (Phases 0–3 of the closure plan) has
   landed and contributor bandwidth is available for the next
   bet.

Documenting the design here, before any build, has two purposes:

- Prevents the wheel from being re-invented when the time does
  come.
- Lets reviewers reject ad-hoc integration PRs by pointing to the
  intended shape ("if you want a Flink adapter, the design lives
  in this doc — extend it, don't replace it").

The framing across all entries is **composability**, not
substitution. `tflo` provides what the host engine cannot: typed
`Absent`, typed `Signal`, edge / WASM / embedded-Rust deployment,
Rust-native composition. The host provides what `tflo` does not:
distributed runtime, exactly-once via 2PC, savepoints, mature SQL,
broad connector catalogs.

---

## `tflo-flink` — Flink integration

**Status:** deferred. Build when a real Flink user wants
edge-emitted typed signals fed into a Flink job, or wants tflo's
detector composition inside a Flink `KeyedProcessFunction`.

**Design sketch**

`tflo-core` compiled to a shared library (`.so` / `.dylib` /
`.dll`); Flink calls via JNI. Companion in `tflo-flink/jvm/` is a
thin Java wrapper exposing `TfloKeyedProcessFunction<K, R, S>` to
Flink job authors. In jurisdictions where JNI is off-limits (some
hardened JVMs), the fallback is a sidecar process with a small
binary protocol.

Two integration directions, both supported:

- **Flink → tflo:** Flink hosts the distributed runtime; tflo runs
  per-key detector logic inside a `KeyedProcessFunction`. The host
  owns watermarks, exactly-once, scaling, savepoints. tflo owns
  signal logic + typed `Absent`.
- **tflo → Flink:** tflo runs at the edge (gateway, browser,
  embedded service), emits typed `Signal<Mode, Payload>` events
  that get serialized and shipped to a central Flink job for
  aggregation, joining with reference data, etc.

Demo: `tflo-examples/examples/flink-supplement/` (would be built
alongside the crate).

**Standalone value**

The "supplement" story made concrete:

- Flink users gain typed `Absent` (named reasons for missing
  values — `WarmingUp`, `DivideByZero`, `FilteredOut`) which Flink
  itself does not provide.
- Flink users gain edge-side signal pre-processing in deployment
  shapes the JVM cannot reach.
- Both engines stay in their best lane: Flink owns the cluster,
  tflo owns the signal logic.

**Why deferred:** integration work has a real cost (JNI surface,
Java companion code, build pipeline for the shared library, CI
against Flink-in-docker). Not justified speculatively; only when
a user pulls.

---

## `tflo-beam` — Apache Beam adapter

**Status:** deferred. Beam runs on Flink, Dataflow, Spark
Streaming, and Samza. One adapter, multiple host engines.

**Design sketch**

A Beam `DoFn` (Java) that wraps a tflo graph the same way
`tflo-flink` wraps it inside a Flink `KeyedProcessFunction`. The
JNI / shared-library layer is shared with `tflo-flink`; the Java
companion is Beam-specific.

**Standalone value**

Portability. A user who writes a tflo+Beam pipeline gets the
choice of execution engine (Dataflow for managed, Flink for
self-hosted, Spark for batch-heavy) without rewriting the signal
logic.

**Why deferred:** strictly after `tflo-flink`. The JNI pattern
must be validated in one place first; only then is it cheap to
extend to Beam.

---

## `tflo-kstreams` — Kafka Streams adapter

**Status:** deferred. Lighter than Flink; useful for Kafka-native
shops without a Flink cluster.

**Design sketch**

A Kafka Streams `Processor<K, V>` (Java) that wraps a tflo graph.
Same shared-library/JNI substrate as `tflo-flink`.

**Standalone value**

For Kafka shops, the operational surface is just Kafka — no
JobManager, no separate state cluster. Adding tflo for signal
emission slots in cleanly without forcing a Flink deployment.

**Why deferred:** Kafka Streams' state-store semantics differ
enough from Flink's that the wrapper is non-trivial. Postpone
until both `tflo-flink` and `tflo-beam` have validated the JNI
pattern.

---

## Esper integration

**Status:** explicitly not planned. Esper is in maintenance mode
upstream; the integration cost-benefit is poor. Listed here so
the question doesn't recur.

If a user does need this, the simplest route is to have tflo emit
typed `Signal` events to a topic that Esper consumes — a normal
producer/consumer pattern requiring no special integration crate.

---

## Promotion criteria

An entry above is promoted from backlog to a committed phase
when, in order:

1. A concrete user (internal or external) requests the
   integration with a clear use case.
2. The use case is consistent with the "supplement, not replace"
   framing (i.e., it does not require duplicating Flink/Beam
   functionality inside tflo-core).
3. The standalone story (Phases 0–3 of the closure plan) is
   complete and stable.

Promotion produces a new phase in the closure plan, a new
workspace member crate, and a tracked example under
`tflo-examples/examples/`.
