# Design — extract the operator catalog into `tflo-ops`

- **Status:** approved (brainstorm), pending implementation plan
- **Date:** 2026-05-22
- **Depends on:** the pre-open-source hardening pass (typed `Computed`/`Absent`,
  the `CustomNode` plugin with `save`/`load`, `postcard` checkpointing)

## Context

`tflo-core` currently bundles two things: a streaming dataflow/CEP **engine**
and a ~60-operator **catalog** (SMA, EMA, std-dev, windows, lag, cross
detection, glitch/runt/pulse detectors, z-score, calibration, …). The domain-
specific layer already left in the CEP reorientation — finance indicators are
in `tflo-fintech`. What remains in core is domain-neutral, but it is still a
fixed, closed catalog baked into the engine's `Node`/`NodeOp`/`NodeState`
enums.

This design separates the two: `tflo-core` becomes a minimal, unopinionated
engine; the operator catalog moves to a new crate, **`tflo-ops`**.

## Goals

1. **Engine/catalog layering** — `tflo-core` reads as a pure dataflow+CEP
   engine: sources, closure transforms, keyed execution, and a plugin
   mechanism. Nothing else.
2. **True extensibility** — node kinds are not a closed enum. A third party
   (or `tflo-ops` itself, or `tflo-fintech`) ships operators as peers, with no
   privileged access to engine internals.
3. **Dependency hygiene** — an embedder who needs only the engine + closures +
   plugins does not compile the catalog.
4. **No behaviour change** — the golden suite stays bit-exact; the public
   *surface* of operators (`price.sma(20)`) is preserved via extension traits.

## Non-goals

- Not changing the runtime-built-DAG model (see "Dispatch" below). No move to
  compile-time pipelines or type-level graphs.
- No cargo-feature granularity inside `tflo-ops` — it is one cohesive crate
  ("the core algorithm library"); what changes together ships together.
- No proc-macro crate.

## Architecture

### Crate topology

```
tflo-core      the engine — graph compile/eval, ValueStore, builder, Comp,
               iterator adapters, keyed execution, Computed/Absent, the
               closure ops (map/scan/filter), the Operator plugin mechanism.
tflo-ops       the catalog — every operator as an Operator plugin, the
               primitives/ structs, generic op shapes, extension traits on
               Comp, the dual-use composites, the event/result types.
```

Dependency graph: `tflo-ops → tflo-core`; `tflo-fintech → tflo-core + tflo-ops`
(its composites build on `ema`/`sma`/…); `tflo-examples`, `tflo-site` examples →
all three; `tflo-cel`/`rhai`/`rego`/`state-*` → `tflo-core` only.

### Dispatch — decided: trait objects

A runtime-constructed heterogeneous DAG cannot statically dispatch its nodes —
the node set is decided at runtime by `.tflo(|t| …)` closures, `zip`, and
per-key keyed graphs. Indirect dispatch is therefore inherent, not a library
gap. Measured cost: a `Box<dyn>` call is ~1–3 ns; the *net* delta over today's
60-variant `match` (itself a jump-table indirect branch) is ~1–2 ns per node
per record. The engine already pays this for `Prop`, `map`/`scan`/`filter`, and
`CustomNode`. Making catalog ops plugins makes dispatch **uniform** — it is not
a new cost category. No fast-lane is built up front; if a benchmarked op
regresses visibly, that single op can keep an inlined path later.

### The `Operator` plugin trait

`CustomNode` is superseded by a single, unified plugin trait, `Operator`. It is
the *only* way a node kind enters the engine — catalog ops, `tflo-fintech`
indicators, and user plugins are all `Operator`s.

- `eval(&mut self, inputs: &[Computed]) -> NodeOutput` — `NodeOutput` is the
  engine's stored value type (today's internal `Value`, made public, likely
  renamed `NodeOutput`): either a `Computed` (`Result<f64, Absent>`) or a typed
  boxed value. f64 ops return `computed.into()`; typed/event ops (`cross`,
  `glitch_filter`, …) return `event.into()`. This is the "generalize the
  plugin" decision — one mechanism covers both f64 and typed-event operators.
- `reset(&mut self)` — default no-op.
- `name(&self) -> &str` — default `"operator"`.
- `save(&self) -> Option<Vec<u8>>` / `load(&mut self, &[u8]) -> Result<(),
  CustomNodeLoadError>` — checkpoint hooks, carried over from the hardening
  pass. Default = not checkpointable.

`Operator` must stay object-safe (`Box<dyn Operator>`).

### Engine changes in `tflo-core`

- `NodeOp` shrinks from ~60 variants to ~9: `Prop`, `Const`, `MapF64`,
  `Map2F64`, `FilterF64`, `FilterMapF64`, `ScanF64`, `Scan2F64`, and `Plugin`
  (holds a `Box<dyn Operator>` plus its input `NodeId`s). `NodeState` shrinks to
  `Stateless`, `ScanState`, `Scan2State`, `Plugin`.
- `eval_node` loses every catalog arm; the `Plugin` arm collects
  `Vec<Computed>` inputs and calls `Operator::eval`.
- `eval/helpers.rs` (the ~20 `eval_*` helpers) moves out — those helper shapes
  become the generic op shapes in `tflo-ops` (see below).
- `Value` → public `NodeOutput`. `ExtractOutput` already extracts both
  `f64`/`Computed` and the boxed typed values; it is unchanged.
- `compile/snapshot.rs` collapses: `NodeStateSnapshot` becomes essentially
  `Stateless` + `Plugin(Vec<u8>)`, since every stateful node is now a plugin
  checkpointed via `save`/`load`.
- `std::ops` arithmetic (`Comp + Comp`, etc.) **stays in `tflo-core`** — the
  orphan rule forbids `tflo-ops` from implementing `std::ops::Add` for
  `tflo-core::Comp`. It is reimplemented as thin sugar over the core closure
  ops (`a + b` ≡ `map2_f64(|x,y| x+y)`), so the engine carries arithmetic
  without a dedicated `Node` variant and without "knowing" any catalog op.
- `comp/custom.rs` (closure ops) and the `Operator` trait + builder entry
  points stay in core.

## `tflo-ops` design

### Generic op shapes (avoid repeated scaffolding — decision T1)

The ~37 stateful catalog ops fall into a few *shapes*. Each shape is one
generic struct that implements `Operator` (including `save`/`load`) **once**;
concrete ops are thin constructors/type aliases over it.

- `Windowed<W, R>` — a window primitive `W` plus a reduction `R: Fn(&W) -> f64`.
  Backs SMA, Std, Variance, Max, Min, Sum, Count, WMA, RSI, Median, Quantile,
  Rank, Skewness, Kurtosis. A small `WindowPrimitive` trait (`push`, `count`)
  lets time- and count-windows be used interchangeably; the `.sma(window)`
  builder picks the concrete `W` from the `Window::Time`/`Count` discriminant.
- `BivariateWindowed<W, R>` — two-input version; backs Correlation, Covariance.
- `StatefulTracker<S, Step>` — a small state `S` plus a step fn; backs Prev,
  Lag, Delta, Rate, Velocity, Acceleration, CumSum/Max/Min/Prod, PctChange,
  LogReturn.

These shapes are a formalization of the engine's existing `eval_windowed` /
`eval_moments` / `eval_bivariate` helpers — the abstraction already exists,
this turns it into types.

### `save`/`load` helper (decision T3)

The op shapes are `Serialize + Deserialize` (the primitives already are, from
the hardening pass). `tflo-ops` provides two free generic helpers — a `postcard`
round-trip of `self` — so any op's `save`/`load` is a one-line delegation. The
generic shapes implement `save`/`load` once via these; the handful of one-off
ops and detectors that do not fit a shape delegate in one line each.

### Extension traits

Catalog methods are extension traits on `Comp` (e.g. `WindowOps`, `StatefulOps`,
`CrossOps`, `MathOps`, `Composites`), implemented for `Comp<R, f64>`. A local
trait implemented for a foreign type is allowed by the orphan rule. A
`tflo_ops::prelude` re-exports them so `use tflo_ops::prelude::*` restores
`price.sma(20)` verbatim. Each method body wires an `Operator` into the graph
through the core builder entry point.

### Stateless math, composites, detectors

- **Stateless math** (`abs`, `sqrt`, `ln`, `exp`, `pow`, `clamp`, `floor`, …)
  are not plugins at all — they are extension methods that call core
  `map_f64`. No `Operator`, no `save`/`load`.
- **Composites** (`zscore`, `deviation_band`, `peak_decline`, `momentum`,
  `rate_of_change`, `dc_remove`, `baseline_correct`, `normalize_range`,
  `calibrate`) are pure graph-builders — extension methods that compose other
  catalog ops. No plugin.
- **Detectors** (`cross`/`cross_above`/`cross_under`/`cross_hysteresis`,
  `glitch_filter`, `runt_detect`, `pulse_width`, `window_detect`) are hand-
  written `Operator`s returning typed `NodeOutput`s. Their event/result enums
  (`ThresholdCrossEventMode`, `GlitchResult`, `RuntResult`, …) move to
  `tflo-ops`.

## Checkpointing

`snapshot()`/`restore()` stay in `tflo-core` and keep their public signatures.
Every stateful node is now a `Plugin`, checkpointed through `Operator::save`/
`load`; the generic shapes provide these for free. A graph is non-
checkpointable only if it contains a `scan`/`fold` node or a plugin whose
`save` returns `None` — the same rule as today. `compile/snapshot.rs` becomes
markedly smaller.

## Migration & breaking changes

This is a breaking change of the same scale and shape as the `tflo-fintech`
extraction:

- Every catalog call site (`.sma()`, `.cross()`, `.zscore()`, …) needs
  `use tflo_ops::prelude::*`. Affects `tflo-examples` (all examples),
  `tflo-site` blog/docs code, `tflo-fintech`, and the doc examples.
- `CustomNode` is renamed/reworked into `Operator`; `tflo-fintech`'s five
  indicator nodes and the `CustomNode` examples update once more.
- `Comp::custom_node` / `custom_node1` become `Operator`-based.
- The change is mechanical (add a `use`, adjust a trait name). It is a pre-1.0,
  pre-release change — acceptable cost.

The migration also re-touches the docs the hardening pass just fixed
(`custom-nodes.mdx`, the `CustomNode` posts) — they move to the `Operator`
vocabulary.

## Testing strategy

- The `tflo-fintech` golden suite is the regression oracle — it must stay
  bit-exact at 1e-6 through the whole migration. Run it after each move.
- `tflo-core`'s existing engine tests stay; new tests cover the `Operator`
  plugin path and typed-output plugins.
- `tflo-ops` gets unit tests per op shape and a snapshot/restore round-trip
  test for plugin checkpointing.
- Workspace gates unchanged: `cargo test --workspace --exclude tflo-wasm`,
  `cargo clippy … -D warnings`, `cargo fmt --check`, wasm build, `astro build`.
- Benchmark the golden suite before/after to confirm the dispatch change does
  not regress a hot op visibly.

## Risks

1. **`Windowed<W, R>` uniformity** — time vs count windows differ in `push`
   signature. Mitigation: a `WindowPrimitive` trait normalizes them; the few
   windows that genuinely do not fit (median, moments) get their own shape.
2. **Golden-suite drift** — any behavioural change surfaces as a 1e-6 mismatch.
   Mitigation: move ops in small batches, run the golden suite each batch.
3. **Migration ripple** — wide but mechanical; the compiler enumerates every
   missing `use`.
4. **`tflo-fintech` coupling** — fintech now depends on `tflo-ops`; its
   composites must be re-pointed at the extension traits.

## Out of scope

Compile-time graph models, feature-gated catalog, a proc-macro for op
boilerplate, and any new operators. This design only relocates and re-expresses
what already exists.
