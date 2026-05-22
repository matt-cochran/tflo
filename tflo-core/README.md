# tflo-core

The temporal event processing engine at the heart of [tflo](../README.md).

`tflo-core` is the engine layer: it compiles declarative computation graphs
and drives them over `Iterator` or `Stream`. The operator catalog (SMA, EMA,
cross detectors, z-score, etc.) lives in the separate [`tflo-ops`](../tflo-ops)
crate — add `tflo-ops` to your `Cargo.toml` and `use tflo_ops::prelude::*`
to restore all catalog methods.

See the [workspace README](../README.md) for an overview and quick start.

## What tflo-core provides

- **Computation graph** — `TFlowBuilder` / `Comp<R, T>` declare the graph;
  `CompiledGraph` drives it over records.
- **Source nodes** — `prop(|r| r.field)` extracts `f64` fields from records;
  `constant(v)` injects a fixed value.
- **Closure transforms** — `map_f64`, `map2_f64`, `filter_f64`,
  `filter_map_f64`, `scan_f64`, `scan2_f64` for inline stateless and stateful
  logic without writing an `Operator` impl.
- **`Operator` plugin trait** — `fn eval(&mut self, inputs: &[Computed], ts: i64) -> NodeOutput`;
  external crates implement this to contribute runtime graph nodes. Previous
  `CustomNode` callers should rename to `Operator`.
- **Keyed execution** — isolated per-key state via `CompiledGraph::keyed`.
- **Checkpointing** — `snapshot()` / `restore()` round-trip full node state
  via `postcard`.
- **Validated pipelines** — `ValidationOptions` enforce NaN/Inf rejection,
  gap limits, warmup minimums, sort order.
- **Out-of-order buffering** — `OutOfOrderPolicy::Buffer` with a watermark.

## Features

- `async` — `Stream` adapters (off by default).
- `full` — enables `async`.

## License

Licensed under either of MIT or Apache-2.0 at your option.
