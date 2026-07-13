#![cfg(feature = "cel")]
#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    clippy::missing_const_for_fn,
    reason = "integration tests"
)]
//! Native CEL-string predicates (D1) with cross-step context parity (D4).
//! The CEL strings here are byte-identical to what `tflo-cep-wasm --features
//! cel` evaluates in the browser — this is the native half of cross-tier parity.

use serde::Serialize;
use std::time::Duration;
use tflo_cep::PatternError;
use tflo_cep::prelude::*;

#[derive(Clone, Debug, Serialize)]
struct Event {
    ts: i64,
    kind: String,
    amount: i64,
    card: String,
}

fn ev(ts: i64, kind: &str, amount: i64, card: &str) -> Event {
    Event {
        ts,
        kind: kind.to_string(),
        amount,
        card: card.to_string(),
    }
}

#[test]
fn cel_abandoned_cart_fires_on_deadline() {
    let pattern = Pattern::<Event>::new("abandoned_cart")
        .timestamp(|e| e.ts)
        .when_cel(r#"kind == "add_to_cart""#)
        .not_then_cel(r#"kind == "purchase""#)
        .within(Duration::from_secs(5))
        .emit(|m| format!("abandoned {}", m.first().card))
        .expect("pattern is valid");

    let events = vec![
        ev(0, "add_to_cart", 10, "c1"),
        ev(1_000, "view_page", 0, "c1"),
        ev(6_500, "view_page", 0, "c1"), // past the 5s deadline → fires
    ];
    let out: Vec<String> = events.into_iter().match_pattern(pattern).collect();
    assert_eq!(out, vec!["abandoned c1".to_string()]);
}

#[test]
fn cel_does_not_fire_when_purchase_arrives() {
    let pattern = Pattern::<Event>::new("abandoned_cart")
        .timestamp(|e| e.ts)
        .when_cel(r#"kind == "add_to_cart""#)
        .not_then_cel(r#"kind == "purchase""#)
        .within(Duration::from_secs(5))
        .emit(|_| "x".to_string())
        .expect("pattern is valid");

    let events = vec![
        ev(0, "add_to_cart", 10, "c1"),
        ev(2_000, "purchase", 10, "c1"), // cancels the negative
    ];
    let out: Vec<String> = events.into_iter().match_pattern(pattern).collect();
    assert!(out.is_empty());
}

#[test]
fn cel_cross_step_correlation_via_first() {
    // Card-testing shape: an auth_attempt, then a LATER auth_attempt with the
    // SAME card — correlated across steps with `first_card` (the MatchContext
    // parity variable). Mismatched cards in between must not advance the match.
    let pattern = Pattern::<Event>::new("same_card_retry")
        .timestamp(|e| e.ts)
        .when_cel(r#"kind == "auth_attempt""#)
        .then_cel(r#"kind == "auth_attempt" && card == first_card"#)
        .within(Duration::from_mins(1))
        .emit(|m| format!("retry {}", m.first().card))
        .expect("pattern is valid");

    let events = vec![
        ev(0, "auth_attempt", 1, "VISA-1"),
        ev(1_000, "auth_attempt", 1, "OTHER"), // different card → no advance
        ev(2_000, "auth_attempt", 1, "VISA-1"), // same as first → completes
    ];
    let out: Vec<String> = events.into_iter().match_pattern(pattern).collect();
    assert_eq!(out, vec!["retry VISA-1".to_string()]);
}

#[test]
fn cel_numeric_predicate() {
    let pattern = Pattern::<Event>::new("small_charge")
        .timestamp(|e| e.ts)
        .when_cel("kind == \"auth_attempt\" && amount < 200")
        .emit(|m| m.first().amount)
        .expect("pattern is valid");

    let events = vec![
        ev(0, "auth_attempt", 50, "c1"),  // < 200 → matches
        ev(1, "auth_attempt", 999, "c2"), // not < 200
    ];
    let out: Vec<i64> = events.into_iter().match_pattern(pattern).collect();
    assert_eq!(out, vec![50]);
}

#[test]
fn cel_spec_json_roundtrip_and_compile() {
    use tflo_cep::CelPatternSpec;

    // The JSON shape is identical to the TypeScript `CelPatternSpec` a server
    // would push to the browser.
    let json = r#"{"name":"abandoned_cart","when":"kind == \"add_to_cart\"","notThen":"kind == \"purchase\"","within":5000}"#;
    let spec: CelPatternSpec = serde_json::from_str(json).expect("parse spec");
    // Round-trips losslessly (same keys, same values).
    let reparsed: CelPatternSpec =
        serde_json::from_str(&serde_json::to_string(&spec).unwrap()).unwrap();
    assert_eq!(spec, reparsed);

    // Compiles to the same behavior as the hand-built D1 pattern.
    let pattern = spec
        .compile(
            |e: &Event| e.ts,
            |m| format!("abandoned {}", m.first().card),
        )
        .expect("spec compiles");
    let events = vec![
        ev(0, "add_to_cart", 10, "c1"),
        ev(6_500, "view_page", 0, "c1"), // past deadline → fires
    ];
    let out: Vec<String> = events.into_iter().match_pattern(pattern).collect();
    assert_eq!(out, vec!["abandoned c1".to_string()]);
}

#[test]
fn cel_spec_interior_negation_card_testing() {
    use tflo_cep::CelPatternSpec;

    // The card-testing fraud rule as pure data a server pushes to the browser:
    // two failed auths within 60s with NO successful auth between them. The
    // `notBetween` guard is positionally aligned with `then`.
    let json = r#"{"name":"card_testing","when":"kind == \"auth_fail\"","then":["kind == \"auth_fail\""],"notBetween":["kind == \"auth_success\""],"within":60000}"#;
    let spec: CelPatternSpec = serde_json::from_str(json).expect("parse spec");
    let reparsed: CelPatternSpec =
        serde_json::from_str(&serde_json::to_string(&spec).unwrap()).unwrap();
    assert_eq!(spec, reparsed, "notBetween round-trips losslessly");

    let build = || {
        spec.compile(
            |e: &Event| e.ts,
            |m| format!("card_testing {}", m.first().card),
        )
        .expect("spec compiles")
    };

    // fail -> fail, no success between -> fires.
    let fires: Vec<String> = vec![
        ev(0, "auth_fail", 1, "c1"),
        ev(20_000, "auth_fail", 1, "c1"),
    ]
    .into_iter()
    .match_pattern(build())
    .collect();
    assert_eq!(fires, vec!["card_testing c1".to_string()]);

    // fail -> success -> fail: the interior success suppresses the signal.
    let suppressed: Vec<String> = vec![
        ev(0, "auth_fail", 1, "c1"),
        ev(10_000, "auth_success", 1, "c1"),
        ev(20_000, "auth_fail", 1, "c1"),
    ]
    .into_iter()
    .match_pattern(build())
    .collect();
    assert!(
        suppressed.is_empty(),
        "a successful auth between the fails suppresses the card-testing signal"
    );
}

#[test]
fn cel_bad_expression_surfaces_at_emit() {
    let result = Pattern::<Event>::new("bad")
        .timestamp(|e| e.ts)
        .when_cel("kind ==== broken(")
        .emit(|_| ());
    assert!(matches!(result, Err(PatternError::CelCompile { .. })));
}
