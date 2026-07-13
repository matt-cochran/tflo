#![cfg(feature = "cel")]
#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    clippy::missing_const_for_fn,
    reason = "integration tests"
)]
//! **Cross-tier parity keystone.** Runs the shared golden vectors
//! (`tests/parity/vectors.json`) through the NATIVE engine. The browser test
//! `tflo-react/tests/parity.test.ts` runs the SAME vectors through the wasm
//! engine. Both assert against the same `expected`, so green-on-both proves the
//! native and browser engines produce byte-identical signals for serialized
//! CEL pattern specs — the foundation of "author once, run identically on
//! client and server."
//!
//! Events are generic `serde_json::Value`s (the wire form a server would push);
//! each fired signal is reduced to the last matched event's `ts`.

use serde::Deserialize;
use serde_json::Value;
use tflo_cep::CelPatternSpec;
use tflo_cep::prelude::*;

#[derive(Deserialize)]
struct Case {
    name: String,
    spec: CelPatternSpec,
    events: Vec<Value>,
    expected: Vec<i64>,
}

#[derive(Deserialize)]
struct Vectors {
    cases: Vec<Case>,
}

fn ts_of(e: &Value) -> i64 {
    e.get("ts").and_then(Value::as_i64).unwrap_or(0)
}

#[test]
fn parity_vectors_native() {
    let raw = include_str!("parity/vectors.json");
    let vectors: Vectors = serde_json::from_str(raw).expect("vectors.json parses");
    assert!(!vectors.cases.is_empty(), "no parity cases loaded");

    for case in vectors.cases {
        let name = case.name.clone();
        let pattern = case
            .spec
            .compile(ts_of, |m: &Match<Value>| ts_of(m.last()))
            .unwrap_or_else(|e| panic!("case `{name}` spec failed to compile: {e}"));

        let out: Vec<i64> = case.events.into_iter().match_pattern(pattern).collect();
        assert_eq!(
            out, case.expected,
            "case `{name}`: native signals != expected"
        );
    }
}
