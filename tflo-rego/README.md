# tflow-rego

OPA/Rego policy engine integration for tflow.

## Overview

This crate provides policy-based filtering and decision-making using the [Rego](https://www.openpolicyagent.org/docs/latest/policy-language/) policy language from Open Policy Agent (OPA), powered by [regorus](https://github.com/microsoft/regorus).

## Features

- **Policy Evaluation**: Evaluate Rego policies against streaming data
- **Policy-Based Filtering**: Filter streams using policy decisions
- **Static Data**: Combine policies with reference data
- **Bundle Support**: Load policies from directories

## Quick Start

### Basic Policy Evaluation

```rust
use tflo_rego::prelude::*;
use std::sync::{Arc, Mutex};

// Create engine and add policy
let mut engine = PolicyEngine::new();

engine.add_policy("spectrum", r#"
    package spectrum
    
    default allow = false
    
    allow {
        input.snr > 10.0
        not protected_band
    }
    
    protected_band {
        input.freq_mhz >= 118.0
        input.freq_mhz <= 137.0
    }
"#)?;

// Evaluate
let detection = serde_json::json!({
    "snr": 15.0,
    "freq_mhz": 200.0
});

let allowed = engine.eval_allow(&detection, "data.spectrum.allow")?;
```

### Filtering Streams

```rust
use std::sync::{Arc, Mutex};

let engine = Arc::new(Mutex::new(PolicyEngine::new()));

// Add policies
{
    let mut e = engine.lock().unwrap();
    e.add_policy("filter", r#"
        package filter
        default allow = false
        allow { input.snr > 10.0 }
    "#)?;
}

// Filter using policy
let allowed: Vec<Detection> = detections.into_iter()
    .rego_filter(engine.clone(), "data.filter.allow")
    .collect();
```

### Using Static Data

```rust
let mut engine = PolicyEngine::new();

// Add reference data
engine.add_data(serde_json::json!({
    "protected_bands": [
        {"start": 118.0, "end": 137.0, "name": "Aviation"},
        {"start": 156.0, "end": 162.0, "name": "Maritime"}
    ],
    "min_snr": 10.0
}))?;

engine.add_policy("rules", r#"
    package rules
    
    default allow = false
    
    allow {
        input.snr >= data.min_snr
        not in_protected_band
    }
    
    in_protected_band {
        some band
        band := data.protected_bands[_]
        input.freq_mhz >= band.start
        input.freq_mhz <= band.end
    }
"#)?;
```

### Loading from Files

```rust
let mut engine = PolicyEngine::new();

// Load a single policy file
engine.add_policy_from_file("policies/spectrum.rego")?;

// Load all policies from a directory
let count = engine.add_policies_from_directory("policies/")?;

// Load reference data
engine.add_data_from_file("data/config.json")?;
```

## Policy Examples

### Access Control

```rego
package authz

default allow = false

allow {
    input.user.role == "admin"
}

allow {
    input.user.role == "operator"
    input.action == "read"
}
```

### Signal Classification

```rego
package signal

classification := "strong" {
    input.snr > 30.0
}

classification := "moderate" {
    input.snr > 10.0
    input.snr <= 30.0
}

classification := "weak" {
    input.snr <= 10.0
}
```

### Rate Limiting

```rego
package ratelimit

default allow = false

allow {
    count(input.recent_requests) < data.max_requests_per_minute
}

deny_reason := "rate limit exceeded" {
    not allow
}
```

## Configuration

```yaml
bundles:
  - path: policies/main
    name: main
    data:
      version: "1.0"
      
data:
  admin_role: superuser
  max_requests: 100
  
strict: true
```

## License

MIT OR Apache-2.0

