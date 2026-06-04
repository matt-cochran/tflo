# tflow-rhai

Rhai scripting integration for tflow.

## Overview

This crate provides embedded scripting capabilities using [Rhai](https://rhai.rs/), allowing runtime-configurable filtering, transformation, and custom logic.

## Features

- **Rhai Filtering**: Filter streams using Rhai expressions
- **Transformation**: Transform items using Rhai scripts
- **Script Engine**: Cache and manage multiple scripts
- **Custom Functions**: Extend with domain-specific functions

## Quick Start

### Filtering

```rust
use tflo_rhai::prelude::*;

impl IntoRhaiScope for Detection {
    fn into_rhai_scope(&self) -> Scope<'static> {
        let mut scope = Scope::new();
        scope.push("snr", self.snr_db);
        scope.push("freq_mhz", self.freq_hz as f64 / 1e6);
        scope
    }
}

let filtered: Vec<Detection> = detections.into_iter()
    .rhai_filter("snr > 10.0 && freq_mhz > 100.0")
    .collect();
```

### Transformation

```rust
// Transform to computed values
let sums: Vec<Dynamic> = items.into_iter()
    .rhai_map("x + y")
    .collect();

// Enrich: keep original with computed
let enriched: Vec<(Item, Dynamic)> = items.into_iter()
    .rhai_enrich("x * y * 2")
    .collect();
```

### Script Engine

```rust
use tflo_rhai::script::ScriptEngine;

let mut engine = ScriptEngine::new();

// Compile and cache scripts
engine.compile("classify", r#"
    if snr > 30.0 { "strong" }
    else if snr > 10.0 { "moderate" }
    else { "weak" }
"#)?;

engine.compile("risk_score", "snr * confidence * 0.01")?;

// Load from files
engine.load_file("custom", "scripts/custom.rhai")?;
engine.load_directory("scripts/")?;

// Evaluate
let classification: String = engine.eval("classify", &detection)?;
let score: f64 = engine.eval("risk_score", &detection)?;
```

### Custom Engine Functions

```rust
use rhai::Engine;

let mut rhai_engine = Engine::new();

// Register custom functions
rhai_engine.register_fn("db_to_linear", |db: f64| -> f64 {
    10.0_f64.powf(db / 10.0)
});

rhai_engine.register_fn("is_protected_band", |freq_mhz: f64| -> bool {
    (118.0..=137.0).contains(&freq_mhz) // Aviation
});

let engine = ScriptEngine::with_engine(rhai_engine);

// Now available in scripts
let filtered = detections.into_iter()
    .rhai_filter_with_engine(engine.engine().clone(), "is_protected_band(freq_mhz)")
    .collect();
```

## Error Handling

```rust
// Filter with error handling
let results: Vec<RhaiResult<Detection>> = detections.into_iter()
    .rhai_filter_result("snr > threshold")?
    .collect();

for result in results {
    match result {
        Ok(detection) => process(detection),
        Err(e) => log_error(e),
    }
}

// Transform with error handling
let results: Vec<RhaiResult<Dynamic>> = items.into_iter()
    .rhai_map_result("complex_calculation(x, y)")?
    .collect();
```

## License

MIT OR Apache-2.0

