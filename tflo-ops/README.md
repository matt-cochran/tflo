# tflo-ops

The operator catalog crate for the [tflo](../README.md) CEP engine.

`tflo-ops` sits atop [`tflo-core`](../tflo-core)'s engine and provides the
full library of ready-made streaming operators, exposed as extension traits on
`Comp<R, f64>` so call sites read naturally — e.g. `price.sma(20)`.

## Operator families

| Family | Trait | Examples |
|--------|-------|---------|
| **Windowed aggregations** | `WindowOps` | `sma`, `ema`, `std`, `var`, `wma`, `median`, `quantile`, `correlation`, `covariance`, `skewness`, `kurtosis` |
| **Stateful trackers** | `StatefulOps` | `prev`, `lag`, `cumulative_sum`, `pct_change`, `log_return`, `rate_of_change`, `momentum`, `peak_decline`, `zscore` |
| **Event detectors** | `CrossOps`, `DetectorOps` | `cross`, `cross_above`, `cross_below`, `glitch`, `runt`, `pulse_width`, `window_detector`, zone ops |
| **Math** | `MathOps` | `add`, `sub`, `mul`, `div`, `abs`, `clamp`, `ln`, `sqrt`, … |
| **Composites** | `Composites` | `deviation_band`, `macd` |

## Quick start

```rust
use tflo_core::comp::Comp;
use tflo_ops::prelude::*; // brings all extension traits + event types into scope

// Assume `price: Comp<MyRecord, f64>` built via TFlowBuilder
let sma20  = price.sma(20);
let above  = price.cross_above(&sma20);  // Comp<R, ThresholdCrossEventMode>
let signal = price.glitch(5, 10);        // Comp<R, GlitchResult>
```

## Primitives

Low-level stateful structs (window buffers, accumulators, detectors) live in
`tflo_ops::primitives`. They implement `tflo_core::operator::Operator` so they
can be plugged directly into custom graph nodes.

## Custom operators via the `Operator` trait

For advanced use cases you can implement `tflo_core::operator::Operator`
directly and contribute it as a runtime graph node:

```rust
use tflo_core::operator::Operator;
use tflo_core::compile::{Computed, NodeOutput, finite_or_warming};

struct MyOp { factor: f64 }

impl Operator for MyOp {
    fn eval(&mut self, inputs: &[Computed], _ts: i64) -> NodeOutput {
        NodeOutput::Computed(inputs[0].map(|v| v * self.factor))
    }
}
```

Pass it to the builder with `comp.custom_op(inputs, Box::new(MyOp { factor: 2.0 }))`.

## Event types

Detector operators return typed (non-`f64`) outputs. The relevant types live in
`tflo_ops::events` and are re-exported by `tflo_ops::prelude`:

- `ThresholdCrossEventMode` — `Rising`, `Falling`, `None`
- `GlitchResult`, `RuntResult`, `PulseWidthResult`
- `WindowEvent`

## Dependency

`tflo-ops` depends on `tflo-core`. To use `tflo-ops` without the full engine,
add `tflo-core` as a direct dependency too (it is a workspace crate).

## License

Licensed under either of MIT or Apache-2.0 at your option.
