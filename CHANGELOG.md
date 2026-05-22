# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

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
  fallible `try_new`. The `release` profile now enables `overflow-checks`.
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
