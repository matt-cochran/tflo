# tflo-core overview

This document is a deeper tour of the `tflo-core` API: the syntax, the mental model, and the typical flow for using it.

At a high level, `tflo-core` is:

- A **declarative** computation graph builder (`TFlowBuilder`)
- A **compiler** that turns that graph into an executable (`CompiledGraph`)
- A set of **iterator adapters** that compile once and execute as you iterate (`.tflo(...)`, `.with(...)`, …)

## The basic flow: define → compile → execute

There are two common ways to run computations:

- **Ergonomic (recommended)**: use iterator adapters (`.tflo(...)`, `.with(...)`, `.validated(...)`)
- **Explicit**: use `TFlowBuilder` + `CompiledGraph` directly

Both approaches are the same model under the hood.

## 1) Define computations (inside a closure)

You define computations by constructing `Comp<R, T>` handles inside a closure. The closure receives a `TFlowBuilder<R>`.

```rust
use tflo_core::prelude::*;

#[derive(Clone)]
struct Tick {
    ts: i64,
    price: f64,
}

let ticks = vec![
    Tick { ts: 1000, price: 100.0 },
    Tick { ts: 2000, price: 101.0 },
    Tick { ts: 3000, price: 99.0 },
];

let smas: Vec<f64> = ticks.into_iter()
    .tflo(|t| {
        // Required for time-based windows.
        t.timestamp(|x| x.ts);

        let price = t.prop(|x| x.price);

        // Time-based window.
        price.sma(2_u64.secs())
    })
    .collect();
```

### Returning multiple outputs

Return a tuple of `Comp`s.

```rust
use tflo_core::prelude::*;

#[derive(Clone)]
struct Tick { ts: i64, price: f64 }

let ticks = vec![
    Tick { ts: 1000, price: 100.0 },
    Tick { ts: 2000, price: 101.0 },
];

let out: Vec<(f64, f64)> = ticks.into_iter()
    .tflo(|t| {
        t.timestamp(|x| x.ts);
        let price = t.prop(|x| x.price);
        (price.sma(2_u64.secs()), price.ema(2_u64.secs()))
    })
    .collect();
```

### Enrich records (keep the input record)

Use `.with(...)` to get `(record, output)` per element.

```rust
use tflo_core::prelude::*;

#[derive(Clone)]
struct Tick { ts: i64, price: f64 }

let ticks = vec![
    Tick { ts: 1000, price: 100.0 },
    Tick { ts: 2000, price: 101.0 },
];

let enriched: Vec<(Tick, f64)> = ticks.into_iter()
    .with(|t| {
        t.timestamp(|x| x.ts);
        let price = t.prop(|x| x.price);
        price.sma(2_u64.secs())
    })
    .collect();
```

## 2) Compile explicitly (optional)

If you want to compile once and then drive execution manually (for example, to integrate into a custom runtime), you can build and compile a graph directly.

```rust
use tflo_core::prelude::*;

#[derive(Clone)]
struct Tick { ts: i64, price: f64 }

let mut b = TFlowBuilder::<Tick>::new();
b.timestamp(|x| x.ts);

let price = b.prop(|x| x.price);
let sma = price.sma(2_u64.secs());

let output_ids = sma.output_ids();
let timestamp_fn = b.get_timestamp_fn().expect("timestamp() must be set for time windows");
let nodes = b.into_nodes();

let mut graph = CompiledGraph::compile(timestamp_fn, nodes, output_ids);

let ticks = vec![
    Tick { ts: 1000, price: 100.0 },
    Tick { ts: 2000, price: 101.0 },
    Tick { ts: 3000, price: 99.0 },
];

for tick in &ticks {
    if let Some(item) = graph.step(tick) {
        // item.ctx is the pipeline context (by default, the timestamp ordering key).
        let _ts = item.ctx;
        let _value = item.value;
    }
}
```

## 3) Execute with explicit warmup/error status (optional)

If you need to distinguish warmup from success and errors, use `.tflo_try(...)` or `CompiledGraph::step_with_status(...)`.

```rust
use tflo_core::prelude::*;
use tflo_core::compile::StepResult;

#[derive(Clone)]
struct Tick { ts: i64, price: f64 }

let ticks = vec![
    Tick { ts: 1000, price: 100.0 },
    Tick { ts: 2000, price: 101.0 },
];

let mut b = TFlowBuilder::<Tick>::new();
b.timestamp(|x| x.ts);
let price = b.prop(|x| x.price);
let sma = price.sma(2_u64.secs());

let output_ids = sma.output_ids();
let timestamp_fn = b.get_timestamp_fn().unwrap();
let nodes = b.into_nodes();
let mut graph = CompiledGraph::compile(timestamp_fn, nodes, output_ids);

for tick in &ticks {
    match graph.step_with_status(tick) {
        StepResult::Ready(_item) => { /* value available */ }
        StepResult::WarmingUp { remaining: _ } => { /* still warming up */ }
        StepResult::Error(_e) => { /* computation error */ }
    }
}
```

## WindowSpec (chainable window syntax)

You can use `WindowSpec` to build fluent window expressions:

```rust
use tflo_core::prelude::*;

#[derive(Clone)]
struct Tick { ts: i64, price: f64 }

let ticks = vec![
    Tick { ts: 1000, price: 100.0 },
    Tick { ts: 2000, price: 101.0 },
];

let smas: Vec<f64> = ticks.into_iter()
    .tflo(|t| {
        t.timestamp(|x| x.ts);
        let price = t.prop(|x| x.price);
        price.over(2_u64.secs()).sma()
    })
    .collect();
```

## Signals: “events as data”

Signal detection produces **domain signals as values** on the stream. Signal “modes” are represented as enums like `ThresholdCrossEventMode`.

`tflo-core` re-exports signal mode types in the prelude (feature `signals` is on by default).

```rust
use tflo_core::prelude::*;

#[derive(Clone)]
struct Tick { ts: i64, price: f64 }

let ticks = vec![
    Tick { ts: 1000, price: 99.0 },
    Tick { ts: 2000, price: 101.0 },
];

let out: Vec<ThresholdCrossEventMode> = ticks.into_iter()
    .tflo(|t| {
        t.timestamp(|x| x.ts);
        let price = t.prop(|x| x.price);
        let threshold = t.constant(100.0);
        price.cross_above(&threshold)
    })
    .collect();
```

## Where to look next

- `tflo-core/src/iter_ext.rs`: iterator adapters (`.tflo`, `.with`, `.validated`, `.tflo_try`, `.tflo_keyed`)
- `tflo-core/src/builder.rs`: `TFlowBuilder` definition
- `tflo-core/src/compile.rs`: `CompiledGraph` runtime + execution semantics
- `tflo-stats`, `tflo-ta`, `tflo-signals`, `tflo-async`: split-out crates with focused scope


