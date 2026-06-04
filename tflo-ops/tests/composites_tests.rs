//! Behavioural tests for the composite operators in
//! [`tflo_ops::ops::composites`].
//!
//! Each test drives a known series through the real `tflo` builder via the
//! [`Composites`] extension trait and asserts per-record output with concrete
//! expected values. UFCS (`Composites::zscore(&v, w)`) is used throughout to
//! avoid the legacy `tflo-core` inherent methods shadowing the trait methods
//! under plain call syntax.
//!
//! Expected values are computed analytically from the method definitions in
//! `tflo-core/src/comp/dual_use.rs` (the behavioural oracle).
//!
//! Note on `std` warm-up: the underlying `CountWindow::variance()` returns
//! `NaN` when the window has fewer than 2 values, so `std` and any output
//! that depends on it (zscore, `deviation_band` upper/lower) is NaN for the
//! first record.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use tflo_core::prelude::*;
use tflo_ops::ops::composites::Composites;

/// Absolute tolerance for floating-point comparisons.
const EPS: f64 = 1e-9;

/// A single-channel record: timestamp + one value.
#[derive(Clone)]
struct Rec {
    ts: i64,
    v: f64,
}

/// Build a `Vec<Rec>` from `(ts, value)` literals.
fn recs(rows: &[(i64, f64)]) -> Vec<Rec> {
    rows.iter().map(|&(ts, v)| Rec { ts, v }).collect()
}

/// Run a single-output composite over a value series and collect per-record
/// `f64` outputs (absent results become `NaN`).
fn run<F>(rows: &[(i64, f64)], build: F) -> Vec<f64>
where
    F: FnOnce(&Comp<Rec, f64>) -> Comp<Rec, f64>,
{
    recs(rows)
        .into_iter()
        .tflo(|t| {
            let _ = t.timestamp(|x| x.ts);
            let v = t.prop(|x| x.v);
            build(&v)
        })
        .collect()
}

/// Run a three-output composite over a value series and collect per-record
/// tuple outputs. Used for `deviation_band`.
fn run_three<F>(rows: &[(i64, f64)], build: F) -> Vec<(f64, f64, f64)>
where
    F: FnOnce(&Comp<Rec, f64>) -> (Comp<Rec, f64>, Comp<Rec, f64>, Comp<Rec, f64>),
{
    recs(rows)
        .into_iter()
        .tflo(|t| {
            let _ = t.timestamp(|x| x.ts);
            let v = t.prop(|x| x.v);
            build(&v)
        })
        .collect()
}

/// Assert two `f64`s are close (or both `NaN`).
fn close(got: f64, want: f64) {
    if want.is_nan() {
        assert!(got.is_nan(), "expected NaN, got {got}");
    } else {
        assert!(
            (got - want).abs() < EPS,
            "expected {want}, got {got} (delta {})",
            (got - want).abs()
        );
    }
}

// ============================================================================
// calibrate — no warmup, output = input * gain + offset
// ============================================================================

#[test]
fn calibrate_basic() {
    // gain=2.0, offset=1.0 → output = v*2 + 1
    let rows = [(1, 0.0), (2, 1.0), (3, 5.0), (4, -3.0)];
    let out = run(&rows, |v| Composites::calibrate(v, 2.0, 1.0));
    close(out[0], 1.0);
    close(out[1], 3.0);
    close(out[2], 11.0);
    close(out[3], -5.0);
}

#[test]
fn calibrate_identity() {
    // gain=1.0, offset=0.0 → passthrough
    let rows = [(1, 3.5), (2, -7.2), (3, 0.0)];
    let out = run(&rows, |v| Composites::calibrate(v, 1.0, 0.0));
    close(out[0], 3.5);
    close(out[1], -7.2);
    close(out[2], 0.0);
}

// ============================================================================
// dc_remove — signal - SMA(signal, window)
// ============================================================================

#[test]
fn dc_remove_count_window() {
    // Window 3, series [1, 2, 3, 4, 5].
    // SMA(3): 1.0, 1.5, 2.0, 3.0, 4.0
    // dc_remove = v - sma
    let rows = [(1, 1.0), (2, 2.0), (3, 3.0), (4, 4.0), (5, 5.0)];
    let out = run(&rows, |v| Composites::dc_remove(v, 3usize));
    close(out[0], 0.0); // 1 - 1.0 = 0
    close(out[1], 0.5); // 2 - 1.5 = 0.5
    close(out[2], 1.0); // 3 - 2.0 = 1.0
    close(out[3], 1.0); // 4 - 3.0 = 1.0
    close(out[4], 1.0); // 5 - 4.0 = 1.0
}

// ============================================================================
// normalize_range — (v - min) / (max - min)
// ============================================================================

#[test]
fn normalize_range_count_window() {
    // Window 2, series [1, 3, 2, 5, 4].
    // min(2), max(2):
    //   step 0: window=[1]       min=1, max=1 → (1-1)/(1-1) = NaN (div by zero, range=0)
    //   step 1: window=[1,3]     min=1, max=3 → (3-1)/(3-1) = 1.0
    //   step 2: window=[3,2]     min=2, max=3 → (2-2)/(3-2) = 0.0
    //   step 3: window=[2,5]     min=2, max=5 → (5-2)/(5-2) = 1.0
    //   step 4: window=[5,4]     min=4, max=5 → (4-4)/(5-4) = 0.0
    let rows = [(1, 1.0), (2, 3.0), (3, 2.0), (4, 5.0), (5, 4.0)];
    let out = run(&rows, |v| Composites::normalize_range(v, 2usize));
    assert!(out[0].is_nan()); // div by zero (range=0 for single-element window)
    close(out[1], 1.0);
    close(out[2], 0.0);
    close(out[3], 1.0);
    close(out[4], 0.0);
}

// ============================================================================
// baseline_correct — signal - quantile(signal, window, percentile)
// ============================================================================

#[test]
fn baseline_correct_p0_equals_min() {
    // 0th percentile = min; so baseline_correct(win, 0.0) = signal - rolling_min.
    // Window 2, series [3, 1, 4, 1, 5].
    // min(2): 3, 1, 1, 1, 1
    // output: 0, 0, 3, 0, 4
    let rows = [(1, 3.0), (2, 1.0), (3, 4.0), (4, 1.0), (5, 5.0)];
    let out = run(&rows, |v| Composites::baseline_correct(v, 2usize, 0.0));
    close(out[0], 0.0); // 3 - 3 = 0
    close(out[1], 0.0); // 1 - 1 = 0
    close(out[2], 3.0); // 4 - 1 = 3
    close(out[3], 0.0); // 1 - 1 = 0
    close(out[4], 4.0); // 5 - 1 = 4
}

// ============================================================================
// zscore — (value - mean) / std
//
// std requires ≥2 values; step 0 is always NaN.
// ============================================================================

#[test]
fn zscore_arithmetic_window_3() {
    // Series [2, 4, 6] window 3:
    //   step 0: mean=2, std=NaN (< 2 values in window) → NaN
    //   step 1: mean=3, std=1 (pop_std of [2,4]) → (4-3)/1 = 1.0
    //   step 2: mean=4, std=sqrt(8/3) (pop_std of [2,4,6]) → (6-4)/sqrt(8/3)
    let rows = [(1, 2.0), (2, 4.0), (3, 6.0)];
    let out = run(&rows, |v| Composites::zscore(v, 3usize));
    assert!(out[0].is_nan()); // std=NaN (only 1 value)
    close(out[1], 1.0); // (4-3)/1 = 1.0
    let mean3 = 4.0_f64;
    let std3 =
        (((2.0_f64 - mean3).powi(2) + (4.0 - mean3).powi(2) + (6.0 - mean3).powi(2)) / 3.0).sqrt();
    close(out[2], (6.0 - mean3) / std3);
}

#[test]
fn zscore_constant_series_is_nan() {
    // A constant series has std=0 → zscore=NaN (0/0) for all steps.
    let rows = [(1, 10.0), (2, 10.0), (3, 10.0), (4, 10.0)];
    let out = run(&rows, |v| Composites::zscore(v, 3usize));
    for v in &out {
        assert!(v.is_nan(), "expected NaN, got {v}");
    }
}

// ============================================================================
// deviation_band — (middle, upper, lower)
//
// upper/lower depend on std; they are NaN for the first record.
// ============================================================================

#[test]
fn deviation_band_count_window() {
    // Series [2, 4, 6] window 3, k=1.
    //   step 0: middle=2, std=NaN (< 2 vals) → upper=NaN, lower=NaN
    //   step 1: middle=3, std=1              → upper=4, lower=2
    //   step 2: middle=4, std=sqrt(8/3)      → upper=4+sqrt(8/3), lower=4-sqrt(8/3)
    let rows = [(1, 2.0), (2, 4.0), (3, 6.0)];
    let out = run_three(&rows, |v| Composites::deviation_band(v, 3usize, 1.0));
    // step 0: middle is valid, upper/lower are NaN
    close(out[0].0, 2.0);
    assert!(out[0].1.is_nan());
    assert!(out[0].2.is_nan());
    // step 1
    close(out[1].0, 3.0);
    close(out[1].1, 4.0);
    close(out[1].2, 2.0);
    // step 2
    let mean3 = 4.0_f64;
    let std3 =
        (((2.0_f64 - mean3).powi(2) + (4.0 - mean3).powi(2) + (6.0 - mean3).powi(2)) / 3.0).sqrt();
    close(out[2].0, mean3);
    close(out[2].1, mean3 + std3);
    close(out[2].2, mean3 - std3);
}

#[test]
fn deviation_band_k2_doubles_width() {
    // Series [0, 2] window 2, k=1 vs k=2: at step 1 mean=1, std=1.
    // k=1: upper=2, lower=0
    // k=2: upper=3, lower=-1
    let rows = [(1, 0.0), (2, 2.0)];
    let out1 = run_three(&rows, |v| Composites::deviation_band(v, 2usize, 1.0));
    let out2 = run_three(&rows, |v| Composites::deviation_band(v, 2usize, 2.0));
    // step 0: upper/lower are NaN for both (only 1 value, std=NaN)
    assert!(out1[0].1.is_nan());
    assert!(out2[0].1.is_nan());
    // step 1: mean=1, std=1
    close(out1[1].0, 1.0);
    close(out1[1].1, 2.0);
    close(out1[1].2, 0.0);
    close(out2[1].1, 3.0);
    close(out2[1].2, -1.0);
}

// ============================================================================
// peak_decline — (current - cummax) / cummax
// ============================================================================

#[test]
fn peak_decline_monotone_increasing_is_zero() {
    // A monotonically increasing series: cummax = current → decline = 0.
    let rows = [(1, 1.0), (2, 2.0), (3, 3.0), (4, 4.0)];
    let out = run(&rows, Composites::peak_decline);
    for v in &out {
        close(*v, 0.0);
    }
}

#[test]
fn peak_decline_after_drop() {
    // Series [4, 4, 2, 4, 1].
    // cummax: 4, 4, 4, 4, 4
    // decline: 0, 0, -0.5, 0, -0.75
    let rows = [(1, 4.0), (2, 4.0), (3, 2.0), (4, 4.0), (5, 1.0)];
    let out = run(&rows, Composites::peak_decline);
    close(out[0], 0.0);
    close(out[1], 0.0);
    close(out[2], -0.5); // (2-4)/4 = -0.5
    close(out[3], 0.0);
    close(out[4], -0.75); // (1-4)/4 = -0.75
}

// ============================================================================
// momentum — current - value period records ago
// ============================================================================

#[test]
fn momentum_period_1() {
    // Momentum(1) = current - prev.
    // Series [10, 12, 9, 14]:
    //   step 0: NaN (no prev yet)
    //   step 1: 12-10 = 2
    //   step 2: 9-12 = -3
    //   step 3: 14-9 = 5
    let rows = [(1, 10.0), (2, 12.0), (3, 9.0), (4, 14.0)];
    let out = run(&rows, |v| Composites::momentum(v, 1));
    assert!(out[0].is_nan());
    close(out[1], 2.0);
    close(out[2], -3.0);
    close(out[3], 5.0);
}

#[test]
fn momentum_period_2() {
    // Momentum(2) = current - value 2 records ago.
    // Series [1, 2, 3, 4, 5]:
    //   step 0: NaN
    //   step 1: NaN
    //   step 2: 3-1 = 2
    //   step 3: 4-2 = 2
    //   step 4: 5-3 = 2
    let rows = [(1, 1.0), (2, 2.0), (3, 3.0), (4, 4.0), (5, 5.0)];
    let out = run(&rows, |v| Composites::momentum(v, 2));
    assert!(out[0].is_nan());
    assert!(out[1].is_nan());
    close(out[2], 2.0);
    close(out[3], 2.0);
    close(out[4], 2.0);
}

// ============================================================================
// rate_of_change — ((current - prev) / prev) * 100
// ============================================================================

#[test]
fn rate_of_change_period_1() {
    // ROC(1) = ((current - prev) / prev) * 100.
    // Series [10, 11, 9]:
    //   step 0: NaN (no prev yet)
    //   step 1: ((11-10)/10)*100 = 10.0
    //   step 2: ((9-11)/11)*100
    let rows = [(1, 10.0), (2, 11.0), (3, 9.0)];
    let out = run(&rows, |v| Composites::rate_of_change(v, 1));
    assert!(out[0].is_nan());
    close(out[1], 10.0);
    close(out[2], ((9.0 - 11.0) / 11.0) * 100.0);
}

#[test]
fn rate_of_change_period_2() {
    // ROC(2) = ((current - lag2) / lag2) * 100.
    // Series [10, 12, 15]:
    //   step 0: NaN
    //   step 1: NaN
    //   step 2: ((15-10)/10)*100 = 50.0
    let rows = [(1, 10.0), (2, 12.0), (3, 15.0)];
    let out = run(&rows, |v| Composites::rate_of_change(v, 2));
    assert!(out[0].is_nan());
    assert!(out[1].is_nan());
    close(out[2], 50.0);
}

// ============================================================================
// Typed divide-by-zero — OPS-002 regression suite.
//
// Each composite division was previously a closure-`Div` node whose `f64 / 0.0`
// drifted to `±inf` or `NaN` and was then flattened to `Absent::WarmingUp` by
// the downstream `finite_or_warming` mapping. With the `divide_safe` helper a
// zero denominator now surfaces the typed `Absent::DivideByZero`. The tests
// below `collect::<Vec<Computed>>` so the `Absent` variant is preserved (the
// `f64` extraction would still flatten it to `NaN`).
// ============================================================================

use tflo_core::compile::{Absent, Computed, NodeOutput};
use tflo_core::operator::{Operator, require};

/// Passthrough operator that re-emits its single input verbatim. It exists
/// only to retype `Comp<R, f64>` as `Comp<R, Computed>` so the typed `Absent`
/// reason survives extraction (the `f64` extractor would flatten every
/// `Err(_)` to `NaN`).
struct AsComputed;

impl Operator for AsComputed {
    fn eval(&mut self, inputs: &[Computed], _ts: i64) -> NodeOutput {
        NodeOutput::computed(require(inputs, 0))
    }

    fn name(&self) -> &str {
        "as_computed"
    }
}

/// Same shape as [`run`] above but collects per-record [`Computed`] so the
/// typed [`Absent`] reason survives. Routes the composite output through the
/// [`AsComputed`] passthrough operator to retype the marker without changing
/// the stored value.
fn run_computed<F>(rows: &[(i64, f64)], build: F) -> Vec<Computed>
where
    F: FnOnce(&Comp<Rec, f64>) -> Comp<Rec, f64>,
{
    recs(rows)
        .into_iter()
        .tflo(|t| {
            let _ = t.timestamp(|x| x.ts);
            let v = t.prop(|x| x.v);
            let out: Comp<Rec, f64> = build(&v);
            Comp::<Rec, f64>::custom_node1_dyn::<_, Computed>(&out, || Box::new(AsComputed))
        })
        .collect()
}

#[test]
fn zscore_emits_divide_by_zero_when_std_is_zero() {
    // A constant series → std=0 once the window has ≥2 samples, so every
    // post-warmup record divides 0 by 0. Pre-fix: each was flattened to
    // `Err(Absent::WarmingUp)`. Post-fix: typed `Err(Absent::DivideByZero)`.
    let rows = [(1, 10.0), (2, 10.0), (3, 10.0), (4, 10.0)];
    let out = run_computed(&rows, |v| Composites::zscore(v, 3usize));
    // Step 0: std is still NaN (only 1 sample in the window), so the
    // numerator/denominator are still warming up.
    assert_eq!(out[0], Err(Absent::WarmingUp));
    // Steps 1..=3: std=0 → typed DivideByZero.
    assert_eq!(out[1], Err(Absent::DivideByZero));
    assert_eq!(out[2], Err(Absent::DivideByZero));
    assert_eq!(out[3], Err(Absent::DivideByZero));
}

#[test]
fn peak_decline_emits_divide_by_zero_when_peak_is_zero() {
    // Starting at 0 → cummax is 0 → (current - 0) / 0 is DivideByZero.
    // After a positive sample lifts the peak, the typed reason clears.
    let rows = [(1, 0.0), (2, 0.0), (3, 5.0), (4, 3.0)];
    let out = run_computed(&rows, Composites::peak_decline);
    assert_eq!(out[0], Err(Absent::DivideByZero));
    assert_eq!(out[1], Err(Absent::DivideByZero));
    // peak=5, current=5 → 0/5 = 0.0
    assert_eq!(out[2], Ok(0.0));
    // peak=5, current=3 → (3-5)/5 = -0.4
    let v = out[3].unwrap();
    assert!((v - -0.4).abs() < EPS, "expected -0.4, got {v}");
}

#[test]
fn rate_of_change_emits_divide_by_zero_when_prev_is_zero() {
    // Period 1, series starts at 0 → on step 1, prev=0 → DivideByZero.
    let rows = [(1, 0.0), (2, 5.0), (3, 10.0)];
    let out = run_computed(&rows, |v| Composites::rate_of_change(v, 1));
    assert_eq!(out[0], Err(Absent::WarmingUp));
    assert_eq!(out[1], Err(Absent::DivideByZero));
    // prev=5, current=10 → ((10-5)/5)*100 = 100.0
    assert_eq!(out[2], Ok(100.0));
}

#[test]
fn normalize_range_emits_divide_by_zero_when_range_is_zero() {
    // Constant series → max == min → range=0 → DivideByZero on every record.
    let rows = [(1, 7.0), (2, 7.0), (3, 7.0)];
    let out = run_computed(&rows, |v| Composites::normalize_range(v, 2usize));
    for (i, &c) in out.iter().enumerate() {
        assert_eq!(
            c,
            Err(Absent::DivideByZero),
            "step {i}: expected DivideByZero, got {c:?}",
        );
    }
}
