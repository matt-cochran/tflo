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
