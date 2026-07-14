#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
//! Runtime-configurable filtering with CEL, Rhai, and Rego.
//!
//! Each scripting crate provides iterator adapters — they operate on
//! regular Rust iterators, not through the compiled graph step cycle.
//!
//! Scenario: an intrusion-detection system (IDS) screens authentication
//! events. The rules that decide which login attempts look suspicious are
//! expressed as scripts, so security analysts can tune them at runtime
//! without recompiling the detector.
//!
//! Run: cargo run --example docs-scripting

use std::sync::{Arc, Mutex};

use cel_interpreter::Context;
use rhai::Scope;
use serde::Serialize;

use tflo_cel::prelude::*;
use tflo_rego::prelude::*;
use tflo_rhai::prelude::*;

// ---- Shared domain type ------------------------------------------------

/// An authentication / login event observed by the IDS.
#[derive(Clone, Debug, Serialize)]
struct AuthEvent {
    /// Event timestamp (epoch millis).
    ts: i64,
    /// Number of failed login attempts in the recent window.
    fail_count: f64,
    /// Reputation score of the source IP (0 = trusted, 100 = known-bad).
    source_ip_score: f64,
}

// ---- CEL ---------------------------------------------------------------

/// CEL requires `IntoCelContext`.
impl IntoCelContext for AuthEvent {
    fn into_cel_context(&self) -> Context<'static> {
        let mut ctx = Context::default();
        ctx.add_variable("ts", self.ts).unwrap();
        ctx.add_variable("fail_count", self.fail_count).unwrap();
        ctx.add_variable("source_ip_score", self.source_ip_score)
            .unwrap();
        ctx
    }
}

fn demo_cel(events: &[AuthEvent]) {
    // Keep only events with many failed logins from a suspect IP.
    // `cel_filter_result` is the canonical API — it surfaces compile and
    // evaluation errors as `Result<T>` instead of panicking / silently
    // dropping records.
    let filtered: Vec<AuthEvent> = events
        .iter()
        .cloned()
        .cel_filter_result("fail_count > 3.0 && source_ip_score > 50.0")
        .expect("CEL expression compiles")
        .filter_map(Result::ok)
        .collect();

    println!("CEL: {}/{} events flagged", filtered.len(), events.len());
}

// ---- Rhai --------------------------------------------------------------

/// Rhai requires `IntoRhaiScope`.
impl IntoRhaiScope for AuthEvent {
    fn into_rhai_scope(&self) -> Scope<'static> {
        let mut scope = Scope::new();
        scope.push("ts", self.ts);
        scope.push("fail_count", self.fail_count);
        scope.push("source_ip_score", self.source_ip_score);
        scope
    }
}

fn demo_rhai(events: &[AuthEvent]) {
    // Rhai script: flag brute-force attempts from low-reputation IPs.
    // `rhai_filter_result` is the canonical API — its engine is built
    // from `RhaiOptions::default()` (conservative DoS-mitigation caps)
    // and evaluation errors propagate as `Result<T>`.
    let filtered: Vec<AuthEvent> = events
        .iter()
        .cloned()
        .rhai_filter_result("fail_count > 3.0 && source_ip_score > 50.0")
        .expect("Rhai expression compiles")
        .filter_map(Result::ok)
        .collect();

    println!("Rhai: {}/{} events flagged", filtered.len(), events.len());
}

// ---- Rego --------------------------------------------------------------

// `AuthEvent` derives `Serialize`, so it automatically implements
// `IntoRegoInput` via the blanket impl in `tflo-rego`.

fn demo_rego(events: &[AuthEvent]) {
    let mut engine = PolicyEngine::new();

    engine
        .add_policy(
            "ids",
            r#"
            package ids

            default allow := false

            allow if {
                input.fail_count > 3.0
                input.source_ip_score > 50.0
            }
        "#,
        )
        .expect("policy should parse");

    let engine = Arc::new(Mutex::new(engine));

    let filtered: Vec<AuthEvent> = events
        .iter()
        .cloned()
        .rego_filter(Arc::clone(&engine), "data.ids.allow")
        .collect();

    println!("Rego: {}/{} events flagged", filtered.len(), events.len());
}

// ---- Main --------------------------------------------------------------

fn main() {
    let events = vec![
        AuthEvent {
            ts: 1000,
            fail_count: 7.0,
            source_ip_score: 85.0,
        },
        AuthEvent {
            ts: 2000,
            fail_count: 1.0,
            source_ip_score: 85.0,
        },
        AuthEvent {
            ts: 3000,
            fail_count: 9.0,
            source_ip_score: 10.0,
        },
        AuthEvent {
            ts: 4000,
            fail_count: 5.0,
            source_ip_score: 70.0,
        },
    ];

    demo_cel(&events);
    demo_rhai(&events);
    demo_rego(&events);
}
