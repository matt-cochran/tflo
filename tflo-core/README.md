# tflo-core

The Rust implementation of the temporal event processing engine at the heart
of [tflo](../README.md). (A TypeScript/Node port of the same engine also exists.)

`tflo-core` provides a declarative computation graph over `Iterator` and
`Stream`: windowing, streaming statistics, signal detection, and keyed
execution. It is domain-agnostic — financial technical-analysis indicators
live in the separate `tflo-fintech` crate.

See the [workspace README](../README.md) for an overview and quick start.

## Highlights

- **Computation graph** — declare once, stream forever; the same graph runs
  over a `Vec` or an async `Stream`.
- **Windowing** — count- and time-based windows over irregularly-timed events.
- **Signal detection** — cross, hysteresis, glitch/debounce, runt,
  pulse-width, and zone detectors.
- **Streaming statistics** — moving averages, Welford variance, correlation,
  higher moments, median, rank.
- **Outlier & trend ops** — deviation bands, z-score, peak decline,
  rate-of-change.
- **Keyed execution** — isolated per-key state via `tflo_keyed`.
- **Extensibility** — the `CustomNode` trait for runtime nodes;
  `map_f64`/`scan_f64` closures for inline logic.

## Features

- `async` — `Stream` adapters (off by default).
- `full` — enables `async`.

## License

Licensed under either of MIT or Apache-2.0 at your option.
