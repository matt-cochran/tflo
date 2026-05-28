//! Behavioural tests for the stateless math operators in
//! [`tflo_ops::ops::math`].
//!
//! Each test drives a known input through an operator and asserts the output,
//! including typed [`Absent`] reasons for domain violations. The expected
//! values are ported from the legacy `tflo-core` oracle:
//! `compile/eval/eval.rs` (`NodeOp::Sqrt`, `NodeOp::Ln`, etc.).
//!
//! Most tests drive the operator directly through [`Operator::eval`] so the
//! typed [`Absent::DomainError`] reason is observable. Builder tests drive
//! the operator end-to-end via the [`MathOps`] extension trait.
//!
//! # UFCS
//!
//! The legacy `tflo-core` inherent methods on `Comp<R, f64>` shadow the
//! `MathOps` trait methods under plain call syntax. Builder tests use UFCS
//! (`MathOps::sqrt(&v)`) to reach the `tflo-ops` extension-trait methods.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use tflo_core::compile::Absent;
use tflo_core::operator::Operator;
use tflo_core::prelude::*;
use tflo_ops::ops::math::{Ln, Log2, Log10, MathOps, Sqrt};

// ============================================================================
// Helpers
// ============================================================================

const EPS: f64 = 1e-10;

/// Feed a single value into an operator at timestamp 0.
fn eval_one(op: &mut dyn Operator, v: f64) -> tflo_core::compile::Computed {
    op.eval(&[Ok(v)], 0).as_computed().unwrap()
}

/// Feed an absent input (`WarmingUp`) into an operator at timestamp 0.
fn eval_absent(op: &mut dyn Operator) -> tflo_core::compile::Computed {
    op.eval(&[Err(Absent::WarmingUp)], 0).as_computed().unwrap()
}

/// Assert a [`Computed`] is `Ok` and within `EPS` of `want`.
fn ok_close(got: tflo_core::compile::Computed, want: f64) {
    match got {
        Ok(v) => assert!(
            (v - want).abs() < EPS,
            "expected Ok({want}), got Ok({v}) (delta {})",
            (v - want).abs()
        ),
        Err(e) => panic!("expected Ok({want}), got Err({e:?})"),
    }
}

/// Run a `MathOps` builder method end-to-end, collecting per-record `f64`
/// outputs (absent results become `NaN`).
#[derive(Clone)]
struct Rec {
    ts: i64,
    v: f64,
}

fn run<F>(rows: &[(i64, f64)], build: F) -> Vec<f64>
where
    F: FnOnce(&Comp<Rec, f64>) -> Comp<Rec, f64>,
{
    rows.iter()
        .map(|&(ts, v)| Rec { ts, v })
        .collect::<Vec<_>>()
        .into_iter()
        .tflo(|t| {
            let _ = t.timestamp(|x| x.ts);
            let v = t.prop(|x| x.v);
            build(&v)
        })
        .collect()
}

// ============================================================================
// abs — closure, no domain error
// ============================================================================

#[test]
fn abs_positive_value() {
    let out = run(&[(1, 3.0)], MathOps::abs);
    assert!((out[0] - 3.0).abs() < EPS);
}

#[test]
fn abs_negative_value() {
    let out = run(&[(1, -4.5)], MathOps::abs);
    assert!((out[0] - 4.5).abs() < EPS);
}

#[test]
fn abs_zero() {
    let out = run(&[(1, 0.0)], MathOps::abs);
    assert!((out[0]).abs() < EPS);
}

// ============================================================================
// sqrt — hand-written Operator, domain error for x < 0
// ============================================================================

#[test]
fn sqrt_valid_input() {
    let mut op = Sqrt;
    ok_close(eval_one(&mut op, 4.0), 2.0);
    ok_close(eval_one(&mut op, 9.0), 3.0);
    ok_close(eval_one(&mut op, 2.0), std::f64::consts::SQRT_2);
}

#[test]
fn sqrt_zero_returns_zero() {
    let mut op = Sqrt;
    ok_close(eval_one(&mut op, 0.0), 0.0);
}

#[test]
fn sqrt_negative_is_domain_error() {
    let mut op = Sqrt;
    assert_eq!(eval_one(&mut op, -1.0), Err(Absent::DomainError));
    assert_eq!(eval_one(&mut op, -0.001), Err(Absent::DomainError));
}

#[test]
fn sqrt_absent_input_propagates() {
    let mut op = Sqrt;
    assert_eq!(eval_absent(&mut op), Err(Absent::WarmingUp));
}

#[test]
fn sqrt_via_builder() {
    let out = run(&[(1, 4.0), (2, 9.0), (3, -1.0)], MathOps::sqrt);
    assert!((out[0] - 2.0).abs() < EPS);
    assert!((out[1] - 3.0).abs() < EPS);
    // domain error surfaces as NaN through the builder (absent => NaN)
    assert!(out[2].is_nan());
}

// ============================================================================
// ln — hand-written Operator, domain error for x <= 0
// ============================================================================

#[test]
fn ln_valid_input() {
    let mut op = Ln;
    ok_close(eval_one(&mut op, 1.0), 0.0);
    ok_close(eval_one(&mut op, std::f64::consts::E), 1.0);
}

#[test]
fn ln_zero_is_domain_error() {
    let mut op = Ln;
    assert_eq!(eval_one(&mut op, 0.0), Err(Absent::DomainError));
}

#[test]
fn ln_negative_is_domain_error() {
    let mut op = Ln;
    assert_eq!(eval_one(&mut op, -1.0), Err(Absent::DomainError));
}

#[test]
fn ln_absent_input_propagates() {
    let mut op = Ln;
    assert_eq!(eval_absent(&mut op), Err(Absent::WarmingUp));
}

#[test]
fn ln_via_builder() {
    let out = run(&[(1, std::f64::consts::E), (2, 1.0), (3, 0.0)], MathOps::ln);
    assert!((out[0] - 1.0).abs() < EPS);
    assert!((out[1] - 0.0).abs() < EPS);
    assert!(out[2].is_nan()); // domain error => NaN through builder
}

// ============================================================================
// log10 — hand-written Operator, domain error for x <= 0
// ============================================================================

#[test]
fn log10_valid_input() {
    let mut op = Log10;
    ok_close(eval_one(&mut op, 1.0), 0.0);
    ok_close(eval_one(&mut op, 10.0), 1.0);
    ok_close(eval_one(&mut op, 100.0), 2.0);
}

#[test]
fn log10_zero_is_domain_error() {
    let mut op = Log10;
    assert_eq!(eval_one(&mut op, 0.0), Err(Absent::DomainError));
}

#[test]
fn log10_negative_is_domain_error() {
    let mut op = Log10;
    assert_eq!(eval_one(&mut op, -5.0), Err(Absent::DomainError));
}

#[test]
fn log10_via_builder() {
    let out = run(&[(1, 100.0), (2, 0.0)], MathOps::log10);
    assert!((out[0] - 2.0).abs() < EPS);
    assert!(out[1].is_nan());
}

// ============================================================================
// log2 — hand-written Operator, domain error for x <= 0
// ============================================================================

#[test]
fn log2_valid_input() {
    let mut op = Log2;
    ok_close(eval_one(&mut op, 1.0), 0.0);
    ok_close(eval_one(&mut op, 2.0), 1.0);
    ok_close(eval_one(&mut op, 8.0), 3.0);
}

#[test]
fn log2_zero_is_domain_error() {
    let mut op = Log2;
    assert_eq!(eval_one(&mut op, 0.0), Err(Absent::DomainError));
}

#[test]
fn log2_negative_is_domain_error() {
    let mut op = Log2;
    assert_eq!(eval_one(&mut op, -1.0), Err(Absent::DomainError));
}

#[test]
fn log2_via_builder() {
    let out = run(&[(1, 8.0), (2, -1.0)], MathOps::log2);
    assert!((out[0] - 3.0).abs() < EPS);
    assert!(out[1].is_nan());
}

// ============================================================================
// exp — closure, no domain error
// ============================================================================

#[test]
fn exp_basic() {
    let out = run(&[(1, 0.0), (2, 1.0), (3, -1.0)], MathOps::exp);
    assert!((out[0] - 1.0).abs() < EPS);
    assert!((out[1] - std::f64::consts::E).abs() < EPS);
    assert!((out[2] - 1.0 / std::f64::consts::E).abs() < 1e-9);
}

// ============================================================================
// pow — closure, no domain error (matches oracle: x.powf(n), no check)
// ============================================================================

#[test]
fn pow_integer_exponent() {
    let out = run(&[(1, 2.0), (2, 3.0)], |v| MathOps::pow(v, 3.0));
    assert!((out[0] - 8.0).abs() < EPS);
    assert!((out[1] - 27.0).abs() < EPS);
}

#[test]
fn pow_fractional_exponent() {
    let out = run(&[(1, 4.0)], |v| MathOps::pow(v, 0.5));
    assert!((out[0] - 2.0).abs() < EPS);
}

#[test]
fn pow_zero_exponent_is_one() {
    let out = run(&[(1, 42.0)], |v| MathOps::pow(v, 0.0));
    assert!((out[0] - 1.0).abs() < EPS);
}

// ============================================================================
// clamp — closure, no domain error
// ============================================================================

#[test]
fn clamp_in_range_passes_through() {
    let out = run(&[(1, 5.0)], |v| MathOps::clamp(v, 0.0, 10.0));
    assert!((out[0] - 5.0).abs() < EPS);
}

#[test]
fn clamp_below_min_snaps_to_min() {
    let out = run(&[(1, -3.0)], |v| MathOps::clamp(v, 0.0, 10.0));
    assert!((out[0] - 0.0).abs() < EPS);
}

#[test]
fn clamp_above_max_snaps_to_max() {
    let out = run(&[(1, 15.0)], |v| MathOps::clamp(v, 0.0, 10.0));
    assert!((out[0] - 10.0).abs() < EPS);
}

// ============================================================================
// floor — closure, no domain error
// ============================================================================

#[test]
fn floor_positive() {
    let out = run(&[(1, 3.9), (2, 3.1), (3, 3.0)], MathOps::floor);
    assert!((out[0] - 3.0).abs() < EPS);
    assert!((out[1] - 3.0).abs() < EPS);
    assert!((out[2] - 3.0).abs() < EPS);
}

#[test]
fn floor_negative() {
    let out = run(&[(1, -3.1)], MathOps::floor);
    assert!((out[0] - (-4.0)).abs() < EPS);
}

// ============================================================================
// ceil — closure, no domain error
// ============================================================================

#[test]
fn ceil_positive() {
    let out = run(&[(1, 3.1), (2, 3.9), (3, 3.0)], MathOps::ceil);
    assert!((out[0] - 4.0).abs() < EPS);
    assert!((out[1] - 4.0).abs() < EPS);
    assert!((out[2] - 3.0).abs() < EPS);
}

#[test]
fn ceil_negative() {
    let out = run(&[(1, -3.9)], MathOps::ceil);
    assert!((out[0] - (-3.0)).abs() < EPS);
}

// ============================================================================
// round — closure, no domain error
// ============================================================================

#[test]
fn round_half_away_from_zero() {
    let out = run(&[(1, 2.5), (2, -2.5), (3, 2.4), (4, 2.6)], MathOps::round);
    assert!((out[0] - 3.0).abs() < EPS); // rounds away from zero
    assert!((out[1] - (-3.0)).abs() < EPS);
    assert!((out[2] - 2.0).abs() < EPS);
    assert!((out[3] - 3.0).abs() < EPS);
}
