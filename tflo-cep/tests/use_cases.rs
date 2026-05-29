#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    clippy::missing_const_for_fn,
    reason = "integration tests"
)]
//! Integration tests modeled on the browser-analytics use cases from the
//! design discussion: `abandoned_cart`, `engaged_with_product`, and a
//! rage-click shape expressed as repeated `then`-within steps.

use std::time::Duration;
use tflo_cep::prelude::*;

#[derive(Clone, Debug)]
struct Event {
    ts: i64,
    action: &'static str,
    target_id: &'static str,
}

fn ev(ts: i64, action: &'static str) -> Event {
    Event {
        ts,
        action,
        target_id: "",
    }
}

fn ev_target(ts: i64, action: &'static str, target_id: &'static str) -> Event {
    Event {
        ts,
        action,
        target_id,
    }
}

#[test]
fn abandoned_cart_fires_when_purchase_does_not_arrive_within_window() {
    let pattern = Pattern::<Event>::new("abandoned_cart")
        .timestamp(|e| e.ts)
        .when(|e| e.action == "add_to_cart")
        .not_then(|e| e.action == "purchase")
        .within(Duration::from_millis(5_000))
        .emit(|m| format!("abandoned at ts={}", m.first().ts))
        .expect("pattern is valid");

    let events = vec![
        ev(0, "add_to_cart"),
        ev(1_000, "view_page"),
        ev(2_000, "view_page"),
        ev(6_500, "view_page"), // past deadline — should trigger emit
    ];

    let signals: Vec<String> = events.into_iter().match_pattern(pattern).collect();
    assert_eq!(signals, vec!["abandoned at ts=0".to_string()]);
}

#[test]
fn abandoned_cart_does_not_fire_when_purchase_arrives_in_time() {
    let pattern = Pattern::<Event>::new("abandoned_cart")
        .timestamp(|e| e.ts)
        .when(|e| e.action == "add_to_cart")
        .not_then(|e| e.action == "purchase")
        .within(Duration::from_millis(5_000))
        .emit(|m| format!("abandoned at ts={}", m.first().ts))
        .expect("pattern is valid");

    let events = vec![
        ev(0, "add_to_cart"),
        ev(2_000, "purchase"),
        ev(10_000, "view_page"),
    ];

    let signals: Vec<String> = events.into_iter().match_pattern(pattern).collect();
    assert!(
        signals.is_empty(),
        "purchase within window cancels the pattern"
    );
}

#[test]
fn abandoned_cart_fires_at_end_of_stream_if_no_purchase_seen() {
    // A real cart-abandonment stream may end before the deadline arrives in
    // event-time. End-of-stream should resolve pending negative matches.
    let pattern = Pattern::<Event>::new("abandoned_cart")
        .timestamp(|e| e.ts)
        .when(|e| e.action == "add_to_cart")
        .not_then(|e| e.action == "purchase")
        .within(Duration::from_millis(5_000))
        .emit(|m| m.first().ts)
        .expect("pattern is valid");

    let events = vec![
        ev(0, "add_to_cart"),
        ev(1_000, "view_page"),
        // Stream ends well before the 5s deadline.
    ];

    let signals: Vec<i64> = events.into_iter().match_pattern(pattern).collect();
    assert_eq!(signals, vec![0]);
}

#[test]
fn engaged_with_product_fires_on_view_then_deep_scroll_within_30s() {
    let pattern = Pattern::<Event>::new("engaged_with_product")
        .timestamp(|e| e.ts)
        .when(|e| e.action == "product_view")
        .then(|e| e.action == "deep_scroll")
        .within(Duration::from_millis(30_000))
        .emit(|m| {
            assert_eq!(m.len(), 2);
            (m.first().ts, m.last().ts)
        })
        .expect("pattern is valid");

    let events = vec![
        ev(1_000, "product_view"),
        ev(5_000, "deep_scroll"), // within 30s — engagement confirmed
    ];

    let signals: Vec<(i64, i64)> = events.into_iter().match_pattern(pattern).collect();
    assert_eq!(signals, vec![(1_000, 5_000)]);
}

#[test]
fn engaged_with_product_does_not_fire_when_deep_scroll_is_late() {
    let pattern = Pattern::<Event>::new("engaged_with_product")
        .timestamp(|e| e.ts)
        .when(|e| e.action == "product_view")
        .then(|e| e.action == "deep_scroll")
        .within(Duration::from_millis(30_000))
        .emit(|_| "engaged")
        .expect("pattern is valid");

    let events = vec![
        ev(0, "product_view"),
        ev(40_000, "deep_scroll"), // 10s past deadline — partial match dropped
    ];

    let signals: Vec<&str> = events.into_iter().match_pattern(pattern).collect();
    assert!(signals.is_empty());
}

#[test]
fn rage_click_shape_three_clicks_in_one_second() {
    // The user's example used `repeated(3.., ...)` which is v0.2; for v0.1
    // we express three-clicks-in-one-second with a hand-chained pattern.
    // This is exactly what the macro-style sugar would compile to.
    let pattern = Pattern::<Event>::new("rage_click")
        .timestamp(|e| e.ts)
        .when(|e| e.action == "pointerdown" && e.target_id == "buy_button")
        .then(|e| e.action == "pointerdown" && e.target_id == "buy_button")
        .within(Duration::from_millis(1_000))
        .then(|e| e.action == "pointerdown" && e.target_id == "buy_button")
        .within(Duration::from_millis(1_000))
        .emit(|m| format!("rage_click on {} ({} taps)", m.first().target_id, m.len()))
        .expect("pattern is valid");

    let events = vec![
        ev_target(0, "pointerdown", "buy_button"),
        ev_target(200, "pointerdown", "buy_button"),
        ev_target(400, "pointerdown", "buy_button"), // third within 1s of second
    ];

    let signals: Vec<String> = events.into_iter().match_pattern(pattern).collect();
    assert_eq!(
        signals,
        vec!["rage_click on buy_button (3 taps)".to_string()]
    );
}

#[test]
fn match_at_name_lookup() {
    let pattern = Pattern::<Event>::new("login_then_checkout")
        .timestamp(|e| e.ts)
        .when(|e| e.action == "login")
        .then_named("checkout", |e| e.action == "checkout")
        .within(Duration::from_millis(60_000))
        .emit(|m| {
            let login = m.at("when_0").expect("login captured");
            let checkout = m.at("checkout").expect("checkout captured");
            (login.ts, checkout.ts)
        })
        .expect("pattern is valid");

    let events = vec![ev(0, "login"), ev(30_000, "checkout")];
    let signals: Vec<(i64, i64)> = events.into_iter().match_pattern(pattern).collect();
    assert_eq!(signals, vec![(0, 30_000)]);
}

#[test]
fn builder_rejects_missing_when() {
    let result = Pattern::<Event>::new("bad").emit(|_| "x");
    assert!(matches!(result, Err(tflo_cep::PatternError::NoWhenStep)));
}

#[test]
fn builder_rejects_not_then_without_within() {
    let result = Pattern::<Event>::new("bad")
        .when(|e| e.action == "a")
        .not_then(|e| e.action == "b")
        .emit(|_| "x");
    assert!(matches!(
        result,
        Err(tflo_cep::PatternError::NotThenMissingWithin { .. })
    ));
}

#[test]
fn builder_rejects_not_then_not_terminal() {
    let result = Pattern::<Event>::new("bad")
        .when(|e| e.action == "a")
        .not_then(|e| e.action == "b")
        .within(Duration::from_secs(1))
        .then(|e| e.action == "c")
        .emit(|_| "x");
    assert!(matches!(
        result,
        Err(tflo_cep::PatternError::NotThenNotTerminal { .. })
    ));
}
