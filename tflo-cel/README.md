# tflow-cel

CEL (Common Expression Language) rule engine integration for tflow.

## Overview

This crate provides runtime-configurable filtering and routing using CEL expressions, allowing rules to be changed without recompilation.

## Features

- **CEL Filtering**: Filter streams using CEL expressions
- **Rule Engine**: Define complex rules in YAML/JSON with hot reload support
- **Routing**: Route items based on rule matches
- **Actions**: Built-in action types (Alert, Log, Record, Tag, Ignore)

## Quick Start

### Simple Filtering

```rust
use tflo_cel::prelude::*;

// Define your type and implement IntoCelContext
impl IntoCelContext for Detection {
    fn into_cel_context(&self) -> Context<'static> {
        let mut ctx = Context::default();
        ctx.add_variable("snr", self.snr_db).unwrap();
        ctx.add_variable("freq_mhz", self.freq_hz as f64 / 1e6).unwrap();
        ctx
    }
}

// Filter using CEL expressions
let strong_signals: Vec<Detection> = detections.into_iter()
    .cel_filter("snr > 10.0 && freq_mhz > 100.0")
    .collect();
```

### Rule Engine

```rust
use tflo_cel::prelude::*;

// Load rules from YAML
let engine = RuleEngine::from_yaml(r#"
rules:
  - name: strong_signal
    condition: "snr > 20.0"
    action:
      type: alert
      priority: high
      
  - name: protected_band
    condition: "freq_mhz >= 118.0 && freq_mhz <= 137.0"
    action:
      type: log
"#)?;

// Evaluate rules
for detection in &detections {
    for rule in engine.evaluate(detection) {
        match &rule.action {
            Action::Alert { priority, .. } => send_alert(detection, priority),
            Action::Log => log_detection(detection),
            _ => {}
        }
    }
}
```

### Routing

```rust
// Route with rule engine
for (detection, matched_rules) in detections.into_iter().cel_route(&engine) {
    for rule in matched_rules {
        handle_match(&rule, &detection);
    }
}

// Only yield items with at least one match
for (detection, rules) in detections.into_iter().cel_route_matched(&engine) {
    process_matched(detection, rules);
}
```

## Rule Configuration

### YAML Format

```yaml
variables:
  min_snr: 10.0
  
rules:
  - name: rule_name
    description: "Optional description"
    condition: "snr > variables.min_snr"
    action:
      type: alert
      priority: high
      notify:
        - ops@example.com
    priority: 1  # Lower = higher priority
    tags:
      - important
      - production
```

### Action Types

```yaml
# Log the match
action:
  type: log

# Send an alert
action:
  type: alert
  priority: high  # low, medium, high, critical
  notify:
    - email@example.com

# Record data
action:
  type: record
  duration_secs: 60

# Tag the item
action:
  type: tag
  tag: "anomaly"

# Suppress/ignore
action:
  type: ignore

# Custom action
action:
  type: custom
  name: "webhook"
  params:
    url: "https://..."
```

## Hot Reload

```rust
let mut engine = RuleEngine::from_file("rules.yaml")?;

// Watch for changes and reload
if rules_file_changed() {
    engine.reload("rules.yaml")?;
}
```

## License

MIT OR Apache-2.0

