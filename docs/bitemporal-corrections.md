# Bitemporal Corrections — API Shape

> **ROADMAP — not implemented.**
> This document specifies the *shape* of a future feature only. No bitemporal correction
> flow exists in the codebase today. See the shipped / latent / roadmap classification in
> `docs/superpowers/specs/2026-06-03-tflo-temporal-repositioning-design.md` §5 and §6-P4.

---

## 1. Problem: late-arriving corrections

tflo's reorder buffer already solves **out-of-order delivery**: an event whose `event_ts`
falls within the `allowed_lateness` window is accepted and placed into the correct
event-time bucket, even if it arrives after later-timestamped events. This is standard
event-time watermark semantics.

**Full bitemporal support is a strictly harder problem.** A correction (or retraction)
amends an event that has *already been processed and emitted against* — it arrives
**after** the allowed-lateness window has closed, or it semantically changes the
*content* (not just the order) of a record that was already counted.

Tracking both dimensions:

| Temporal axis | Meaning | Who controls it |
|---|---|---|
| `event_ts` | When the thing actually happened in the world | The event producer |
| `ingestion_ts` | When we first learned about it (processing time) | The engine / wall clock |

A system is **bitemporal** when it can answer: *"What did we know as of ingestion-time T₂
about events that occurred as of event-time T₁?"* Today tflo answers only the event-time
half cleanly.

---

## 2. Proposed correction event shape

A correction (or retraction) is a first-class event in the stream, not an out-of-band
mutation. Its payload carries enough information for deterministic re-derivation.

```rust
// Conceptual Rust shape — not yet implemented
pub enum CorrectionKind {
    /// Replace the `value` field of the target event.
    Amend { new_value: serde_json::Value },
    /// Remove the target event as if it never happened.
    Retract,
}

pub struct CorrectionEvent {
    /// Discriminator understood by the engine.
    pub kind: &'static str,           // "correction"
    /// Stable ID of the event being corrected (must be present on original).
    pub target_event_id: Uuid,
    /// The original event-time of the target event (used to locate its window bucket).
    pub event_ts: Timestamp,
    /// When this correction was issued (the new "learn time").
    pub ingestion_ts: Timestamp,
    /// What to do.
    pub correction: CorrectionKind,
}
```

Equivalent JSON wire form:

```json
{
  "kind": "correction",
  "target_event_id": "018f3b2a-...",
  "event_ts": 1717123456000,
  "ingestion_ts": 1717123999000,
  "new_value": { "amount": 42.50 },
  "retract": false
}
```

or, for a retraction:

```json
{
  "kind": "correction",
  "target_event_id": "018f3b2a-...",
  "event_ts": 1717123456000,
  "ingestion_ts": 1717123999000,
  "retract": true
}
```

**Design constraint:** original events must carry a stable `event_id` field for
corrections to reference. The engine does not generate these today; adding stable IDs to
the event schema is a prerequisite.

---

## 3. Re-derivation contract

When a correction lands, the engine must re-derive *only the affected outputs* —
not a full replay from the beginning.

### 3.1 What is "affected"

An output (window result, CEP match/no-match, projection partial) is affected if and
only if:

1. The corrected event's `event_ts` falls within the event-time bucket (or pattern
   window) that contributed to that output, **and**
2. The output's value depends on the corrected field(s) (predicate, aggregation input,
   sequence membership).

Outputs from buckets that do not overlap the corrected event's `event_ts` are untouched.

### 3.2 Re-derivation is pure replay on a sub-log

This is the critical property: **re-derivation reuses the pure-replay guarantee** that
already powers `useRecorder.replay()`, backtest (P1), and seek/roll-back (P2). The
engine:

1. Identifies the affected bucket(s) / partial matches by `event_ts`.
2. Constructs a patched sub-log: the original events for that bucket, with the corrected
   event replaced or removed.
3. Reruns the pure function `f(sub_log, rule, clock)` over that sub-log.
4. Emits a **derived correction event** carrying the delta: `{ prior_output, new_output,
   corrected_by: target_event_id }`.

Because the engine is 100% pure (`tflo-core/src/clock.rs`), this re-derivation is
deterministic: the same patched sub-log always produces the same revised output.

### 3.3 Downstream propagation

Re-derived outputs propagate forward through the graph exactly as original outputs do.
Downstream projections and React hooks that consume the output receive the revised signal;
the prior emission is superseded. The `useExplain` audit trail records the correction
lineage.

---

## 4. Relation to existing shipped features

| Feature | Status | Relation to bitemporal corrections |
|---|---|---|
| Reorder buffer (`tflo-core/src/reorder.rs`) | **Ships** | Handles *ordering* corrections within allowed-lateness; bitemporal corrections extend this to *content* corrections after the window closes |
| `useRecorder` / `replay()` | **Ships** | Re-derivation reuses the same pure-replay path |
| `useBacktest` (P1) | **Ships** | Sub-log replay is the same mechanism |
| `replayUntil(t)` / seek (P2) | **Ships** | State-as-of-T is a prerequisite for knowing what was known before a correction |
| `useShadowSpec` (P3) | **Ships** | Shadow mode can be used to test a correction policy before activating it |
| `GraphSnapshot` completeness for `Scan`/`Scan2` (P5) | **Ships** | Complete snapshots are required for efficient partial re-derivation without full replay |
| **Bitemporal corrections (P4)** | **ROADMAP** | This document |

---

## 5. Phase summary

This is a design sketch. The phasing of the broader primitives roadmap:

| Phase | Primitives | Status |
|---|---|---|
| P0 | README "a port" fix | **Shipped** |
| P1 | `useBacktest` | **Shipped** |
| P2 | `replayUntil(t)` / seek | **Shipped** |
| P3 | `useShadowSpec` | **Shipped** |
| P5 | `GraphSnapshot` `Scan`/`Scan2` completeness | **Shipped** |
| **P4** | **Bitemporal corrections** | **ROADMAP — this document** |

Implementation of P4 is deferred. It will not appear in docs or marketing until a
runnable proof artifact exists (strict-proof discipline, per the design spec §1).
