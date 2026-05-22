# tflo-ops Split — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Extract the ~60-operator catalog out of `tflo-core` into a new cohesive crate `tflo-ops`, leaving `tflo-core` a minimal dataflow/CEP engine whose only node kinds are sources, closure transforms, and a generalized `Operator` plugin.

**Architecture:** `tflo-core` keeps the graph engine and gains one unified plugin trait, `Operator` (superseding `CustomNode`), whose `eval` returns a public `NodeOutput` so both `f64` ops and typed-event detectors are the same kind of plugin. `tflo-ops` re-expresses every catalog operator as an `Operator` — collapsing the uniform families into generic shapes (`Windowed<W,R>`, `BivariateWindowed<W,R>`, `StatefulTracker<S,Step>`) so the plugin scaffolding is written once — and exposes them as extension traits on `Comp` so `price.sma(20)` reads unchanged after a `use tflo_ops::prelude::*`.

**Tech Stack:** Rust 2024, workspace crates, `serde`/`postcard` (checkpointing), `proptest` (dev). Spec: `docs/superpowers/specs/2026-05-22-tflo-ops-split-design.md`.

---

## Conventions used throughout

- **Golden gate:** after every phase, run `cargo test -p tflo-fintech --test golden` — it must report `55 passed; 0 failed`. Any mismatch is a behavioural regression; stop and investigate.
- **Workspace gate:** `cargo test --workspace --exclude tflo-wasm`, `cargo clippy --workspace --exclude tflo-wasm --all-features --all-targets -- -D warnings`, `cargo fmt --all --check`.
- **Commit cadence:** one commit per task (the final step of each task). Hold no uncommitted work between tasks.
- **Op-migration pattern:** Phase 2 shows each family's pattern *in full once*; the remaining ops of that family are listed in a table and follow the identical shown pattern. That is intentional — the pattern is complete, the table enumerates its applications.

---

## File Structure

**`tflo-core` after the split** — engine only:
- `compile/` — graph compile/eval, `ValueStore`, `NodeOutput` (was `Value`), snapshot. `NodeOp`/`NodeState` shrink to sources + closures + `Plugin`.
- `comp/mod.rs` — `Comp`, `NodeId`, `std::ops` arithmetic (stays — orphan rule).
- `comp/custom.rs` — closure ops (`map_f64`/`scan_f64`/…). `comp/plugin.rs` — `Operator`-based `custom_node`.
- `custom_node.rs` → renamed `operator.rs` — the `Operator` trait + `require`.
- `builder.rs`, `iter_ext.rs`, `keyed.rs`, `pipeline.rs`, `validation/`, `error.rs`, `event.rs` (only generic `Signal`/`EventMode` helpers stay; the detector result enums move).
- **Deleted:** `comp/{windowed,stateful,cross,math,dual_use}.rs`, `compile/eval/helpers.rs`, the catalog `NodeOp` arms.
- **Moved out:** `primitives/`.

**`tflo-ops` (new crate):**
- `src/lib.rs`, `src/prelude.rs`
- `src/primitives/` — moved verbatim from `tflo-core`.
- `src/shapes/{windowed.rs,bivariate.rs,tracker.rs}` — the generic op shapes.
- `src/ops/{windows.rs,stats.rs,trackers.rs,detectors.rs,math.rs,composites.rs}` — concrete ops + extension traits.
- `src/checkpoint.rs` — the `save`/`load` helper.
- `src/events.rs` — `ThresholdCrossEventMode`, `GlitchResult`, `RuntResult`, `PulseWidthResult`, `WindowEvent` (moved from `tflo-core`).

---

## Phase 1 — Engine: the `Operator` plugin + `NodeOutput`

Phase 1 is purely additive on the engine side: it generalizes the plugin trait and exposes `NodeOutput`. The catalog `NodeOp` variants stay untouched and working. At the end of Phase 1 the whole workspace still builds and the golden suite is green.

### Task 1.1: Expose `Value` as the public `NodeOutput`

**Files:**
- Modify: `tflo-core/src/compile/value.rs`
- Modify: `tflo-core/src/compile/mod.rs` (re-export)
- Modify: `tflo-core/src/lib.rs` (prelude)

- [ ] **Step 1: Rename the type and make it public.** In `value.rs`, rename `pub(crate) enum Value` to `pub enum NodeOutput`, keeping both variants (`Computed(Computed)`, `Other(Box<dyn Any + Send + Sync>)`). Add public constructors and keep `as_any`:

```rust
impl NodeOutput {
    /// Wrap a computed `f64`-or-absent result.
    #[inline]
    #[must_use]
    pub fn computed(c: Computed) -> Self {
        NodeOutput::Computed(c)
    }

    /// Wrap any other typed value (an event enum, a `map`/`fold` output).
    ///
    /// This is the orphan-rule-safe way for a downstream crate's operator to
    /// emit a non-`f64` output — it cannot `impl From<…> for NodeOutput`.
    #[inline]
    #[must_use]
    pub fn other<T: std::any::Any + Send + Sync>(value: T) -> Self {
        NodeOutput::Other(Box::new(value))
    }
}
```

- [ ] **Step 2: Keep the `From` impls that are still legal.** Keep `impl From<f64>` and `impl From<Computed>` (both source types are in `tflo-core`). **Delete** the `impl_other_from!` macro invocation for the event types (`ThresholdCrossEventMode`, `GlitchResult`, …) — those types leave `tflo-core` in Phase 4, and downstream crates use `NodeOutput::other(…)` instead.

- [ ] **Step 3: Update every internal reference.** Rename `Value` → `NodeOutput` workspace-wide inside `tflo-core` (grep `Value` in `compile/`). `compile/mod.rs`: change `pub(crate) use value::Value;` to `pub use value::NodeOutput;`. `lib.rs` prelude: add `NodeOutput` to the `compile::{…}` re-export.

- [ ] **Step 4: Build.** Run `cargo build -p tflo-core`. Expected: PASS (the event `From` deletions cause errors only if something still constructs them — fix those call sites to `NodeOutput::other`).

- [ ] **Step 5: Commit.** `git add -A && git commit -m "refactor(core): expose Value as public NodeOutput"`

### Task 1.2: Define the `Operator` trait

**Files:**
- Rename: `tflo-core/src/custom_node.rs` → `tflo-core/src/operator.rs`
- Modify: `tflo-core/src/lib.rs` (`pub mod operator;`)
- Test: `tflo-core/src/operator.rs` (`#[cfg(test)]`)

- [ ] **Step 1: Write the failing test.** In `operator.rs` tests:

```rust
#[test]
fn operator_emits_typed_output() {
    struct Tagger;
    impl Operator for Tagger {
        fn eval(&mut self, inputs: &[Computed], _ts: i64) -> NodeOutput {
            NodeOutput::other(require(inputs, 0).is_ok())
        }
    }
    let mut op = Tagger;
    let out = op.eval(&[Ok(1.0)], 0);
    assert_eq!(out.as_any().downcast_ref::<bool>(), Some(&true));
}
```

- [ ] **Step 2: Run it — verify it fails.** `cargo test -p tflo-core operator_emits_typed_output` — Expected: FAIL (`Operator` not defined).

- [ ] **Step 3: Define the trait.** Replace the `CustomNode` trait with:

```rust
use crate::compile::{Absent, Computed, NodeOutput};

/// A node kind contributed to the engine — the single plugin mechanism.
///
/// `tflo-core` defines only sources and closure transforms natively; every
/// other node kind (the `tflo-ops` catalog, `tflo-fintech` indicators, user
/// plugins) reaches the engine as an `Operator`.
pub trait Operator: Send + Sync + 'static {
    /// Evaluate against this record's resolved inputs and timestamp.
    ///
    /// `inputs` holds one [`Computed`] per wired input, in declaration order.
    /// `ts` is the record timestamp (needed by time-windowed operators).
    fn eval(&mut self, inputs: &[Computed], ts: i64) -> NodeOutput;

    /// Reset to the freshly-constructed state. Default: no-op.
    fn reset(&mut self) {}

    /// Human-readable name for graph-plan/debug output. Default: `"operator"`.
    fn name(&self) -> &str {
        "operator"
    }

    /// Serialize state for checkpointing. Default `None` = not checkpointable.
    fn save(&self) -> Option<Vec<u8>> {
        None
    }

    /// Restore state from `save()` bytes. Default errors.
    fn load(&mut self, _bytes: &[u8]) -> Result<(), OperatorLoadError> {
        Err(OperatorLoadError::new("operator does not support checkpoint restore"))
    }
}

/// Boxed live operator instance held by a compiled graph.
pub type BoxedOperator = Box<dyn Operator>;

/// Factory producing fresh `Operator` instances (one per compiled graph).
pub type OperatorFactory = std::sync::Arc<dyn Fn() -> BoxedOperator + Send + Sync>;
```

Keep `require` unchanged. Rename `CustomNodeLoadError` → `OperatorLoadError` (keep its `new`/`Display`/`Error` impls). Rename `BoxedCustomNode`/`CustomNodeFactory` as above.

- [ ] **Step 4: Run the test — verify it passes.** `cargo test -p tflo-core operator_emits_typed_output` — Expected: PASS.

- [ ] **Step 5: Commit.** `git add -A && git commit -m "feat(core): add unified Operator plugin trait"`

### Task 1.3: Wire `Operator` into the engine

**Files:**
- Modify: `tflo-core/src/compile/mod.rs` (`NodeOp`/`NodeState`)
- Modify: `tflo-core/src/compile/eval/eval.rs` (the plugin arm)
- Modify: `tflo-core/src/comp/mod.rs` (`Node::Custom`), `comp/plugin.rs`

- [ ] **Step 1: Rename the plugin variants.** In `compile/mod.rs`, rename `NodeOp::Custom { inputs }` → `NodeOp::Plugin { inputs }`; `NodeState::Custom(BoxedCustomNode)` → `NodeState::Plugin(BoxedOperator)`. In `comp/mod.rs`, rename `Node::Custom { inputs, factory }` → `Node::Plugin { inputs, factory: OperatorFactory }`.

- [ ] **Step 2: Update `eval_node`'s plugin arm.** In `eval/eval.rs`, the arm becomes — note the `Operator` returns `NodeOutput` directly, no `Value::from`:

```rust
NodeOp::Plugin { inputs } => {
    let values: Vec<Computed> =
        inputs.iter().map(|id| Self::get_computed(store, id)).collect();
    match &mut node.state {
        NodeState::Plugin(op) => op.eval(&values, ts),
        _ => NodeOutput::computed(Err(Absent::WarmingUp)),
    }
}
```

- [ ] **Step 3: Update `ctx.rs` compile + `comp/plugin.rs`.** `compile_node` maps `Node::Plugin` → `NodeOp::Plugin` + `NodeState::Plugin(factory())`. `comp/plugin.rs`'s `custom_node`/`custom_node1` keep the `(first, rest, factory)` signature from the hardening pass but bound `N: Operator` and build `Node::Plugin`.

- [ ] **Step 4: Update snapshot.** In `compile/snapshot.rs`, `NodeStateSnapshot::Custom` → `Plugin`; `to_snapshot`/`apply_to` call `Operator::save`/`load`.

- [ ] **Step 5: Build + golden gate.** `cargo build -p tflo-core`; then migrate `tflo-fintech` in Task 1.4 before the golden suite can run.

- [ ] **Step 6: Commit.** `git add -A && git commit -m "refactor(core): rename Custom node kind to Plugin, eval via Operator"`

### Task 1.4: Migrate `tflo-fintech` + examples to `Operator`

**Files:**
- Modify: `tflo-fintech/src/nodes/mod.rs` (5 indicator structs)
- Modify: `tflo-examples/examples/custom-nodes/main.rs`, `three-extension-mechanisms/main.rs`

- [ ] **Step 1: Migrate the 5 fintech nodes.** For each of `AdxNode`, `PlusDiNode`, `MinusDiNode`, `AtrNode`, `KamaNode`: change `impl CustomNode` → `impl Operator`, and `fn eval(&mut self, inputs: &[Computed]) -> Computed` → `fn eval(&mut self, inputs: &[Computed], _ts: i64) -> NodeOutput`. The body is unchanged except the return: wrap the final `Computed` in `NodeOutput::computed(…)`. Example (`AdxNode`):

```rust
fn eval(&mut self, inputs: &[Computed], _ts: i64) -> NodeOutput {
    let close = match require(inputs, 0) { Ok(v) => v, Err(e) => return NodeOutput::computed(Err(e)) };
    let high  = match require(inputs, 1) { Ok(v) => v, Err(e) => return NodeOutput::computed(Err(e)) };
    let low   = match require(inputs, 2) { Ok(v) => v, Err(e) => return NodeOutput::computed(Err(e)) };
    self.close.push(close);
    self.high.push(high);
    self.low.push(low);
    NodeOutput::computed(finite_or_warming(ta_adx_last(&self.high, &self.low, &self.close, self.period)))
}
```

(`require(…)?` no longer works since the fn returns `NodeOutput`, not `Computed` — use the `match` form, or a tiny private helper `fn ok_or_out(c: Computed, …)`. Provide the helper if it reads better.)

- [ ] **Step 2: Migrate the example `CustomNode` impls** the same way (`RateOfChange`, `SnrGate`, `ScoreFilter`): `impl Operator`, `eval(…, _ts) -> NodeOutput`, wrap returns.

- [ ] **Step 3: Golden gate.** `cargo test -p tflo-fintech --test golden` — Expected: `55 passed; 0 failed`.

- [ ] **Step 4: Workspace gate.** `cargo test --workspace --exclude tflo-wasm` — Expected: all pass.

- [ ] **Step 5: Commit.** `git add -A && git commit -m "refactor: migrate CustomNode impls to Operator"`

---

## Phase 2 — `tflo-ops` crate + generic shapes

`tflo-core` still owns `primitives/` (public) and still has its catalog `NodeOp` arms — Phase 2 builds `tflo-ops` *alongside*, depending on `tflo_core::primitives`. Both can compute SMA at the end of Phase 2; that redundancy is removed in Phases 3–4.

### Task 2.1: Scaffold the `tflo-ops` crate

**Files:**
- Create: `tflo-ops/Cargo.toml`, `tflo-ops/src/lib.rs`, `tflo-ops/src/prelude.rs`
- Modify: workspace `Cargo.toml` (`members`, `[workspace.dependencies]`)

- [ ] **Step 1:** Create `tflo-ops/Cargo.toml`: package `tflo-ops`, `version`/`edition`/`rust-version` from `workspace`, `[lints] workspace = true`, deps `tflo-core = { workspace = true }`, `serde = { workspace = true }`, `postcard = { workspace = true }`, dev-deps `proptest`.

- [ ] **Step 2:** Add `"tflo-ops"` to workspace `members` and `tflo-ops = { path = "tflo-ops" }` to `[workspace.dependencies]`.

- [ ] **Step 3:** `lib.rs`: crate doc, `#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]`, `mod` declarations (empty modules for now), `pub mod prelude;`.

- [ ] **Step 4: Build.** `cargo build -p tflo-ops` — Expected: PASS (empty crate).

- [ ] **Step 5: Commit.** `git add -A && git commit -m "feat(ops): scaffold tflo-ops crate"`

### Task 2.2: Checkpoint helper + `WindowPrimitive` trait

**Files:**
- Create: `tflo-ops/src/checkpoint.rs`, `tflo-ops/src/shapes/mod.rs`

- [ ] **Step 1: The `save`/`load` helper.** In `checkpoint.rs`:

```rust
use serde::{Serialize, de::DeserializeOwned};
use tflo_core::operator::OperatorLoadError;

/// Generic `Operator::save` body: postcard-encode the whole operator.
#[must_use]
pub fn save<T: Serialize>(op: &T) -> Option<Vec<u8>> {
    postcard::to_stdvec(op).ok()
}

/// Generic `Operator::load` body: postcard-decode in place.
///
/// # Errors
/// Returns [`OperatorLoadError`] if the bytes are malformed.
pub fn load<T: DeserializeOwned>(op: &mut T, bytes: &[u8]) -> Result<(), OperatorLoadError> {
    *op = postcard::from_bytes(bytes).map_err(|e| OperatorLoadError::new(format!("decode: {e}")))?;
    Ok(())
}
```

- [ ] **Step 2: The `WindowPrimitive` trait.** In `shapes/mod.rs` — the common interface that lets time- and count-windows back one generic shape:

```rust
/// Common interface over the windowing primitives (`TimeWindow`, `CountWindow`, …).
pub trait WindowPrimitive {
    /// Admit a value; `ts` is ignored by count-based windows.
    fn push(&mut self, ts: i64, value: f64);
    /// Number of values currently retained.
    fn len(&self) -> usize;
    /// True when the window holds no values.
    fn is_empty(&self) -> bool {
        self.len() == 0
    }
}
```

- [ ] **Step 3: Build + commit.** `cargo build -p tflo-ops`; `git add -A && git commit -m "feat(ops): checkpoint helper + WindowPrimitive trait"`

### Task 2.3: `impl WindowPrimitive` for the moved primitives

**Files:**
- Modify: `tflo-core/src/primitives/{time_window,count_window,wma,rsi,median_window,higher_moments}.rs`

> The primitives still live in `tflo-core` during Phase 2; impl the trait there. It moves with them in Phase 4 (the `impl` moves too).

- [ ] **Step 1:** For each window primitive that a `Windowed` op uses, add `impl tflo_? ::WindowPrimitive`. The trait is in `tflo-ops`, the type is in `tflo-core` → orphan rule forbids this in `tflo-core`. **Resolution:** define `WindowPrimitive` in `tflo-core` instead (it is engine-neutral plumbing), re-exported by `tflo-ops`. Move the Task 2.2 Step 2 trait into `tflo-core/src/operator.rs` and have `tflo-ops` re-export it. Then `impl WindowPrimitive for TimeWindow` lives next to `TimeWindow`.

- [ ] **Step 2:** Add the impls. `TimeWindow`/`MedianTimeWindow`/`MomentsTimeWindow`/etc. use `push(ts, v)`; `CountWindow`/`MedianCountWindow`/etc. ignore `ts`. `len()` maps to the existing `count()`/buffer length.

- [ ] **Step 3: Build + commit.** `cargo build -p tflo-core`; `git add -A && git commit -m "feat(core): WindowPrimitive impls for window primitives"`

### Task 2.4: The `Windowed<W, R>` shape

**Files:**
- Create: `tflo-ops/src/shapes/windowed.rs`
- Test: same file (`#[cfg(test)]`)

- [ ] **Step 1: Write the failing test.**

```rust
#[test]
fn windowed_mean_over_count_window() {
    use tflo_core::primitives::CountWindow;
    let mut op = Windowed::new(CountWindow::new(3), |w: &CountWindow| w.mean());
    assert!(op.eval(&[Ok(10.0)], 0).as_computed().unwrap().is_ok()); // partial mean
    let _ = op.eval(&[Ok(20.0)], 0);
    let out = op.eval(&[Ok(30.0)], 0);
    assert_eq!(out.as_computed().unwrap(), Ok(20.0)); // mean(10,20,30)
}
```

(`as_computed()` is a small accessor on `NodeOutput` returning `Option<Computed>`; add it to `NodeOutput` in `tflo-core` if not present.)

- [ ] **Step 2: Run — verify fail.** `cargo test -p tflo-ops windowed_mean_over_count_window` — Expected: FAIL.

- [ ] **Step 3: Implement the shape.**

```rust
use serde::{Serialize, Deserialize};
use tflo_core::compile::{Computed, NodeOutput, finite_or_warming};
use tflo_core::operator::{Operator, require};
use tflo_core::WindowPrimitive;
use crate::checkpoint;

/// Generic windowed-reduction operator: push the input into a window
/// primitive `W`, then apply reduction `R` to the window.
#[derive(Serialize, Deserialize)]
pub struct Windowed<W, R> {
    window: W,
    #[serde(skip)]
    reduce: R,
}

impl<W, R> Windowed<W, R> {
    pub fn new(window: W, reduce: R) -> Self {
        Self { window, reduce }
    }
}

impl<W, R> Operator for Windowed<W, R>
where
    W: WindowPrimitive + Serialize + serde::de::DeserializeOwned + Send + Sync + 'static,
    R: Fn(&W) -> f64 + Default + Send + Sync + 'static,
{
    fn eval(&mut self, inputs: &[Computed], ts: i64) -> NodeOutput {
        let v = match require(inputs, 0) {
            Ok(v) => v,
            Err(e) => return NodeOutput::computed(Err(e)), // skip the push on absent input
        };
        self.window.push(ts, v);
        NodeOutput::computed(finite_or_warming((self.reduce)(&self.window)))
    }
    fn name(&self) -> &str { "windowed" }
    fn save(&self) -> Option<Vec<u8>> { checkpoint::save(self) }
    fn load(&mut self, bytes: &[u8]) -> Result<(), tflo_core::operator::OperatorLoadError> {
        checkpoint::load(self, bytes)
    }
}
```

> `reduce` is `#[serde(skip)]` — it is a stateless `fn`/zero-sized closure restored via `R: Default`. Use zero-sized named reduction types (e.g. `struct Mean;` with `impl Fn`-equivalent), *not* boxed closures, so `R: Default` holds and the struct stays `Serialize`. If a chosen `R` cannot be `Default`, that op uses a hand-written `Operator` instead (rare).

- [ ] **Step 4: Run — verify pass.** `cargo test -p tflo-ops windowed_mean_over_count_window` — Expected: PASS.

- [ ] **Step 5: Commit.** `git add -A && git commit -m "feat(ops): Windowed<W,R> generic op shape"`

### Task 2.5: The `BivariateWindowed` and `StatefulTracker` shapes

**Files:**
- Create: `tflo-ops/src/shapes/bivariate.rs`, `tflo-ops/src/shapes/tracker.rs`

- [ ] **Step 1: `BivariateWindowed<W, R>`** — same as `Windowed` but `eval` does `require(inputs,0)` and `require(inputs,1)`, propagating the first `Err`, and pushes the pair (`w.push(ts, a, b)`). Add a `BivariateWindow` trait analogous to `WindowPrimitive` for the two-input push, impl'd for `CorrelationTimeWindow`/`CorrelationCountWindow`. Write a failing test (covariance of a known series), implement, verify pass.

- [ ] **Step 2: `StatefulTracker<S, Step>`** — holds state `S: Serialize`, a `Step: Fn(&mut S, f64, i64) -> Computed + Default`. `eval` does `require(inputs,0)?`-style, then `(self.step)(&mut self.state, v, ts)`. Backs prev/lag/delta/rate/velocity/acceleration/cumsum/cummax/cummin/cumprod/pct_change/log_return. Write a failing test (cumsum), implement, verify pass.

- [ ] **Step 3: Commit.** `git add -A && git commit -m "feat(ops): BivariateWindowed + StatefulTracker shapes"`

### Task 2.6: Concrete windowed ops + the `WindowOps` extension trait

**Files:**
- Create: `tflo-ops/src/ops/windows.rs`, `tflo-ops/src/ops/stats.rs`
- Test: `tflo-ops/tests/windows_tests.rs`

- [ ] **Step 1: Reduction types.** For each windowed op define a zero-sized reduction. Pattern (shown once, in full):

```rust
/// Reduction: arithmetic mean of the window.
#[derive(Default, Serialize, Deserialize)]
pub struct Mean;
impl Mean { pub fn apply(w: &impl HasMean) -> f64 { w.mean() } }
```

In practice make `R` a plain `fn(&W) -> f64` pointer captured at construction (a `fn` item is `Copy + Default`-free) — **or** keep `R` as a named unit struct implementing a `Reduce<W>` trait. Choose the named-unit-struct form so `Windowed` stays `Serialize` with `R: Default`. Define one unit struct per reduction: `Mean, Std, Variance, Max, Min, Sum, Count, Wma, Rsi, Median(q?), Rank, Skewness, Kurtosis`.

- [ ] **Step 2: The extension trait.** In `windows.rs`:

```rust
use tflo_core::comp::Comp;
use tflo_core::window::Window;

/// Windowed-aggregation operators on a `Comp`.
pub trait WindowOps<R: 'static> {
    /// Simple moving average over `window`.
    fn sma<W: Into<Window>>(&self, window: W) -> Comp<R, f64>;
    // … std, variance, max, min, sum, count, wma, rsi, median, quantile,
    //    rank, skewness, kurtosis
}

impl<R: 'static> WindowOps<R> for Comp<R, f64> {
    fn sma<W: Into<Window>>(&self, window: W) -> Comp<R, f64> {
        let w: Window = window.into();
        self.custom_node1(move || match w {
            Window::Time(d)  => boxed(Windowed::new(TimeWindow::new(d),  Mean)),
            Window::Count(n) => boxed(Windowed::new(CountWindow::new(n), Mean)),
        })
    }
    // …
}
```

`custom_node1` takes a factory `|| -> impl Operator`; the time/count branch picks the concrete `Windowed<W, Mean>`. `boxed(…)` coerces to a common `Box<dyn Operator>` so both arms have one type — i.e. the factory returns `BoxedOperator`; have `custom_node1` accept a factory of `BoxedOperator` (it already boxes internally — expose a `custom_node1_boxed` variant, or make the factory return `impl Operator` and box per-arm via an enum). **Decision:** add `Comp::custom_node1_dyn(factory: impl Fn() -> BoxedOperator)` to `tflo-core` so the time/count branches unify cleanly.

- [ ] **Step 3: Tests.** `tflo-ops/tests/windows_tests.rs`: for `sma`/`std`/`max`/`min`/`sum`/`median`/`rsi`/`wma` etc., feed a known series via `.tflo(|t| { t.timestamp(…); t.prop(…).sma(3) })` and assert known outputs (port the existing `tflo-core` windowed unit tests verbatim — they are the behavioural oracle).

- [ ] **Step 4: Remaining windowed ops.** Each op below = "Step 2's `sma` pattern with the named reduction; Step 3 a ported test". Implement all:

| Op | Window primitive(s) | Reduction |
|---|---|---|
| `std`, `variance`, `max`, `min`, `sum`, `count` | `TimeWindow` / `CountWindow` | `Std`/`Variance`/`Max`/`Min`/`Sum`/`Count` |
| `median`, `quantile`, `rank` | `MedianTimeWindow` / `MedianCountWindow` | `Median`/`Quantile(q)`/`Rank` |
| `skewness`, `kurtosis` | `MomentsTimeWindow` / `MomentsCountWindow` | `Skewness`/`Kurtosis` |
| `wma` | `WmaTimeWindow` / `WmaCountWindow` | `Wma` |
| `rsi` | `RsiTimeWindow` / `RsiCountWindow` | `Rsi` |
| `correlation`, `covariance` | `BivariateWindowed` | `Correlation`/`Covariance` |
| `ema` | hand-written `Operator` (`TimeEma`/`CountEma` — not a window reduction) | — |
| `rsi_wilder_n` | hand-written `Operator` over `RsiWilderState` | — |

- [ ] **Step 5: Golden gate + commit.** `cargo test -p tflo-ops` passes; `git add -A && git commit -m "feat(ops): windowed + statistical operators"`

### Task 2.7: Stateful-tracker ops

**Files:** Create `tflo-ops/src/ops/trackers.rs`, test `tflo-ops/tests/trackers_tests.rs`.

- [ ] **Step 1:** Implement `prev`, `lag`, `delta`, `rate`, `velocity`, `acceleration`, `cumsum`, `cummax`, `cummin`, `cumprod`, `pct_change`, `log_return` as `StatefulTracker`-backed extension methods on a `StatefulOps` trait — each a step fn ported from `compile/eval/helpers.rs` (the typed-`Absent` logic from the hardening pass is the oracle: `dt<=0`→`ZeroTimeDelta`, `pct_change` prev-0→`DivideByZero`, etc.). `prev_by` is special: its key comes from the *record*, which `Operator::eval` does not receive — implement `prev_by(key_fn)` by wiring a hidden key-extraction `prop` node and passing the key as a second input (`StatefulTracker` reads `inputs[1]` as the partition key; document the 2⁵³ exact-key range).
- [ ] **Step 2:** Port the `tflo-core` stateful unit tests as `trackers_tests.rs`.
- [ ] **Step 3:** `cargo test -p tflo-ops` passes; commit `feat(ops): stateful tracker operators`.

### Task 2.8: Detector ops (typed output)

**Files:** Create `tflo-ops/src/events.rs`, `tflo-ops/src/ops/detectors.rs`, test `tflo-ops/tests/detectors_tests.rs`.

- [ ] **Step 1: Move the event types.** Copy `ThresholdCrossEventMode`, `GlitchResult`, `RuntResult`, `PulseWidthResult`, `WindowEvent` (and their helper methods) into `tflo-ops/src/events.rs`. `impl tflo_core::compile::ExtractOutput` for each (foreign-trait-for-local-type — allowed). They are *deleted from `tflo-core`* in Phase 4.
- [ ] **Step 2: Detector operators.** Implement `cross`, `cross_above`, `cross_under`, `cross_hysteresis`, `glitch_filter`, `runt_detect`, `pulse_width`, `window_detect` as hand-written `Operator`s wrapping the corresponding `primitives/` detector struct. `eval` reads inputs, updates the detector, returns `NodeOutput::other(event)`. `save`/`load` via `checkpoint::{save,load}`. Expose them on a `CrossOps` / `DetectorOps` extension trait. `gt`/`gte`/`lt`/`lte` are f64-output — `Windowed`-free; implement as `map2_f64`-based extension methods (Task 2.9 style).
- [ ] **Step 3:** Port the `tflo-core` detector tests.
- [ ] **Step 4:** `cargo test -p tflo-ops` passes; commit `feat(ops): event-detector operators`.

### Task 2.9: Stateless math ops

**Files:** Create `tflo-ops/src/ops/math.rs`.

- [ ] **Step 1:** `abs`, `sqrt`, `ln`, `exp`, `log10`, `log2`, `pow`, `clamp`, `floor`, `ceil`, `round` as a `MathOps` extension trait — each one line over the core closure op, e.g.:

```rust
fn sqrt(&self) -> Comp<R, f64> {
    self.map_f64(|x| if x < 0.0 { f64::NAN } else { x.sqrt() })
}
```

> These are not `Operator`s and need no `save`/`load` — they are pure closure nodes. Preserve the hardening pass's domain semantics (`sqrt`/`ln` of `<=0`): since `map_f64` closures are `f64 -> f64`, a domain error must surface as `NaN` here (the closure cannot emit `Absent`). If typed `DomainError` must be preserved for these, implement them instead as tiny hand-written `Operator`s — **decide during implementation by checking whether any test asserts the `Absent::DomainError` reason**; default to the closure form.

- [ ] **Step 2:** Commit `feat(ops): stateless math operators`.

### Task 2.10: Composites + prelude

**Files:** Create `tflo-ops/src/ops/composites.rs`, `tflo-ops/src/prelude.rs` (fill in).

- [ ] **Step 1:** Port `comp/dual_use.rs` — `deviation_band`, `zscore`, `peak_decline`, `momentum`, `rate_of_change`, `dc_remove`, `baseline_correct`, `normalize_range`, `calibrate` — as a `Composites` extension trait. These are pure graph-builders composing other `tflo-ops` methods; bodies move verbatim.
- [ ] **Step 2:** `prelude.rs` re-exports every extension trait (`WindowOps`, `StatefulOps`, `CrossOps`, `DetectorOps`, `MathOps`, `Composites`) and the event types.
- [ ] **Step 3:** `cargo test -p tflo-ops` + `cargo clippy -p tflo-ops -- -D warnings` pass; commit `feat(ops): composites + prelude`.

---

## Phase 3 — Switch the builder over

### Task 3.1: Re-point the builder, delete core catalog methods

**Files:**
- Delete: `tflo-core/src/comp/{windowed,stateful,cross,math,dual_use}.rs`
- Modify: `tflo-core/src/comp/mod.rs` (drop the `mod` lines; keep arithmetic `std::ops`)

- [ ] **Step 1:** Delete the five `comp/*.rs` catalog files and their `mod` declarations. The arithmetic `std::ops` impls in `comp/mod.rs` stay — verify they are implemented over `map2_f64`/`map_f64` (if any still build `Node::Add` etc., rewrite them as closure nodes now).
- [ ] **Step 2: Build the workspace.** `cargo build --workspace --exclude tflo-wasm` — Expected: FAIL, with errors at every `.sma()`/`.cross()`/… call site in `tflo-fintech`, `tflo-examples`, tests. That error list IS the Phase 5 migration checklist.
- [ ] **Step 3:** Do not fix call sites yet — that is Phase 5. Confirm `cargo build -p tflo-core` alone passes (core no longer references the catalog).
- [ ] **Step 4: Commit.** `git add -A && git commit -m "refactor(core): remove catalog builder methods (moved to tflo-ops)"`

---

## Phase 4 — Delete the dead engine catalog

### Task 4.1: Delete catalog `NodeOp`/`NodeState` variants + eval arms

**Files:**
- Modify: `compile/mod.rs` (`NodeOp`, `NodeState`), `compile/eval/eval.rs`, `compile/node.rs` (`offset_input_ids`), `compile/ctx.rs`, `compile/inspect.rs` (Debug)
- Delete: `tflo-core/src/compile/eval/helpers.rs`

- [ ] **Step 1:** Delete every catalog variant from `NodeOp` (Sma…Custom-era ops) and `NodeState`, leaving `NodeOp` = `Prop, Const, MapF64, Map2F64, FilterF64, FilterMapF64, ScanF64, Scan2F64, Plugin`; `NodeState` = `Stateless, ScanState, Scan2State, Plugin`.
- [ ] **Step 2:** Delete the corresponding `eval_node` arms and `eval/helpers.rs` (the `eval_*` helpers); delete the catalog arms from `offset_input_ids` and the `NodeState` `Debug` impl.
- [ ] **Step 3:** Delete the now-unused `RsiWilderState` from `compile/mod.rs` (it moved to a `tflo-ops` hand-written op in Task 2.6).
- [ ] **Step 4: Build.** `cargo build -p tflo-core` — Expected: PASS. `cargo build -p tflo-ops` — Expected: PASS (it used `tflo_core::primitives`, still present).
- [ ] **Step 5: Commit.** `git add -A && git commit -m "refactor(core): delete dead catalog node kinds"`

### Task 4.2: Move `primitives/` to `tflo-ops`

**Files:** `git mv tflo-core/src/primitives tflo-ops/src/primitives`

- [ ] **Step 1:** `git mv tflo-core/src/primitives tflo-ops/src/primitives`. Remove `pub mod primitives;` from `tflo-core/src/lib.rs`; add `mod primitives;` to `tflo-ops/src/lib.rs` (re-export the public types from the `tflo-ops` prelude).
- [ ] **Step 2:** Fix imports: in `tflo-ops`, `tflo_core::primitives::X` → `crate::primitives::X`. The `WindowPrimitive` impls move with the files; `WindowPrimitive` itself stays in `tflo-core` (Task 2.3 Step 1) — `tflo-ops` `use`s it.
- [ ] **Step 3:** `tflo-core`'s prelude drops the `primitives::{…}` re-export. Check no `tflo-core` file still references `primitives` (snapshot.rs must not).
- [ ] **Step 4: Build.** `cargo build -p tflo-core && cargo build -p tflo-ops` — Expected: PASS.
- [ ] **Step 5: Commit.** `git add -A && git commit -m "refactor: move primitives/ from tflo-core to tflo-ops"`

### Task 4.3: Collapse `compile/snapshot.rs`

**Files:** Modify `tflo-core/src/compile/snapshot.rs`.

- [ ] **Step 1:** `NodeStateSnapshot` now needs only `Stateless` and `Plugin(Vec<u8>)` variants (scan/scan2 stay non-checkpointable → `to_snapshot` returns `None`). Delete the ~30 catalog mirror variants and their `to_snapshot`/`apply_to` arms.
- [ ] **Step 2: Golden gate.** `cargo test -p tflo-fintech --test golden` — Expected: `55 passed`. `cargo test -p tflo-core --test snapshot_tests` — Expected: pass.
- [ ] **Step 3: Commit.** `git add -A && git commit -m "refactor(core): collapse snapshot to the Plugin node kind"`

---

## Phase 5 — Migration

### Task 5.1: Migrate `tflo-fintech`

**Files:** `tflo-fintech/Cargo.toml`, `tflo-fintech/src/composites.rs`, `tflo-fintech/src/lib.rs`.

- [ ] **Step 1:** Add `tflo-ops = { workspace = true }` to `tflo-fintech/Cargo.toml`.
- [ ] **Step 2:** In `composites.rs` and wherever fintech builds graphs, add `use tflo_ops::prelude::*;`. The fintech composites call `ema`/`sma`/`true_range`/`scan_f64`/… — `scan_f64` stays core, the rest now come from `tflo-ops` traits. Re-point `Comp::custom_node` calls (already `Operator`-based from Phase 1).
- [ ] **Step 3: Golden gate.** `cargo test -p tflo-fintech` — Expected: golden `55 passed`, contract tests pass.
- [ ] **Step 4: Commit.** `git add -A && git commit -m "refactor(fintech): depend on tflo-ops for the operator catalog"`

### Task 5.2: Migrate `tflo-examples`

**Files:** every `tflo-examples/examples/*/main.rs` flagged by the Phase 3 Step 2 build failure.

- [ ] **Step 1:** Add `tflo-ops = { workspace = true }` to `tflo-examples/Cargo.toml`.
- [ ] **Step 2:** For each failing example, add `use tflo_ops::prelude::*;` (next to the existing `use tflo_core::prelude::*;`). Adjust any `tflo_core::primitives::…` / `tflo_core::event::…` paths to `tflo_ops::…`.
- [ ] **Step 3: Build all examples.** `cargo build -p tflo-examples --examples` — Expected: PASS (all examples).
- [ ] **Step 4: Commit.** `git add -A && git commit -m "refactor(examples): use tflo-ops for catalog operators"`

### Task 5.3: Migrate the docs site

**Files:** `tflo-site/src/content/blog/*.mdx`, `tflo-site/src/pages/docs/*.astro`.

- [ ] **Step 1:** Grep `tflo-site/src` for code samples using catalog ops, `CustomNode`, `tflo_core::primitives`, `tflo_core::event`. Update import lines to `use tflo_ops::prelude::*;` and `CustomNode` → `Operator` (with the `eval(&[Computed], ts) -> NodeOutput` signature). The hardening pass already touched `custom-nodes.mdx` / the `CustomNode` posts — re-touch them for the `Operator` vocabulary.
- [ ] **Step 2: Build the site.** `cd tflo-site && npx astro build` — Expected: `Complete!`, 33 pages.
- [ ] **Step 3: Commit.** `git add -A && git commit -m "docs(site): update for the tflo-ops split"`

### Task 5.4: Final verification + CHANGELOG

**Files:** `CHANGELOG.md`, `tflo-core`/`tflo-ops` READMEs.

- [ ] **Step 1:** Add a CHANGELOG entry: the `tflo-ops` extraction, the `Operator` trait (supersedes `CustomNode`), `NodeOutput`, the `use tflo_ops::prelude::*` migration note. Add a short `tflo-ops` README; trim the `tflo-core` README's operator list.
- [ ] **Step 2: Full workspace gate.** Run all of: `cargo test --workspace --exclude tflo-wasm`; `cargo clippy --workspace --exclude tflo-wasm --all-features --all-targets -- -D warnings`; `cargo fmt --all --check`; `cargo build -p tflo-core --target wasm32-unknown-unknown`; `cd tflo-site && npx astro build`. All must pass.
- [ ] **Step 3:** Confirm `tflo-core`'s `NodeOp` has 9 variants and `tflo-core/src` no longer contains `primitives/`, `comp/windowed.rs`, etc.
- [ ] **Step 4: Commit.** `git add -A && git commit -m "docs: CHANGELOG + READMEs for the tflo-ops split"`

---

## Self-Review

**Spec coverage:** engine/catalog layering → Phases 1,3,4. True extensibility (`Operator`, no closed enum) → Task 1.2, 4.1. Dependency hygiene → Task 2.1 dep graph. Trait-object dispatch → Task 1.3. Typed-output plugin → Task 1.1 (`NodeOutput`), 1.2, 2.8. Generic shapes T1 → Tasks 2.4–2.5. `save`/`load` helper T3 → Task 2.2. One cohesive crate, no features → Task 2.1. Arithmetic stays in core (orphan rule) → Task 3.1. Checkpointing via `save`/`load` → Tasks 2.2, 4.3. Migration → Phase 5. All spec sections map to tasks.

**Open implementation decisions deliberately deferred to the engineer** (each flagged inline, not a placeholder): the `R: Default` vs `fn`-pointer form for `Windowed`'s reduction (Task 2.6 Step 1); whether stateless math keeps typed `DomainError` (Task 2.9 Step 1); `custom_node1_dyn` helper shape (Task 2.6 Step 2). Each has a stated default.

**Type consistency:** `Operator::eval(&[Computed], i64) -> NodeOutput` used consistently (Tasks 1.2, 1.3, 1.4, 2.4–2.9). `NodeOutput::{computed,other,as_any,as_computed}` consistent. `BoxedOperator`/`OperatorFactory`/`OperatorLoadError` consistent. `WindowPrimitive` lives in `tflo-core` (resolved in Task 2.3 Step 1, used in 2.4).

**Sequencing invariant:** `tflo-core` builds standalone after every task; `tflo-ops` builds from Task 2.1; the workspace is intentionally red only between Task 3.1 and Task 5.2 (the migration window) — called out explicitly.
