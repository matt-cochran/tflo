//! Behavioural tests for the windowed + statistical operators in
//! [`tflo_ops::ops::windows`].
//!
//! Each test drives a known series through the real `tflo` builder via the
//! [`WindowOps`] extension trait and asserts the operator's per-record output.
//! The expected values are the behavioural oracle ported from the legacy
//! `tflo-core` catalog: the `Windowed`/`BivariateWindowed` operators wrap the
//! exact same window primitives the old `NodeOp` arms dispatched to, so
//! results are bit-identical.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use tflo_core::prelude::*;
use tflo_ops::ops::windows::WindowOps;

/// A single-channel record: timestamp + one value.
#[derive(Clone)]
struct Rec {
    ts: i64,
    v: f64,
}

/// A two-channel record: timestamp + a pair of values.
#[derive(Clone)]
struct Pair {
    ts: i64,
    a: f64,
    b: f64,
}

/// Build a `Vec<Rec>` from `(ts, value)` literals.
fn recs(rows: &[(i64, f64)]) -> Vec<Rec> {
    rows.iter().map(|&(ts, v)| Rec { ts, v }).collect()
}

/// Run a single-input windowed operator over a value series and collect the
/// per-record `f64` outputs (absent results become `NaN`).
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

/// Absolute tolerance for floating-point comparisons.
const EPS: f64 = 1e-9;

/// Assert two `f64`s are equal within [`EPS`].
fn close(got: f64, want: f64) {
    assert!(
        (got - want).abs() < EPS,
        "expected {want}, got {got} (delta {})",
        (got - want).abs()
    );
}

// ============================================================================
// Basic aggregations
// ============================================================================

#[test]
fn sma_count_window() {
    // Count window of 3 over 1..=5: each output is the mean of the last <=3.
    let rows = [(1, 1.0), (2, 2.0), (3, 3.0), (4, 4.0), (5, 5.0)];
    let out = run(&rows, |v| WindowOps::sma(v, 3usize));
    let want = [1.0, 1.5, 2.0, 3.0, 4.0];
    assert_eq!(out.len(), want.len());
    for (g, w) in out.iter().zip(want) {
        close(*g, w);
    }
}

#[test]
fn sma_time_window() {
    // 10s window: by ts=6000 all six values are inside it.
    let rows = [
        (1000, 150.0),
        (2000, 2800.0),
        (3000, 151.0),
        (4000, 2810.0),
        (5000, 149.0),
        (6000, 2795.0),
    ];
    let out = run(&rows, |v| {
        WindowOps::sma(v, std::time::Duration::from_secs(10))
    });
    close(out[0], 150.0);
    let expected_last = (150.0 + 2800.0 + 151.0 + 2810.0 + 149.0 + 2795.0) / 6.0;
    close(out[5], expected_last);
}

#[test]
fn std_and_variance_count_window() {
    // Classic series with mean 5, population variance 4, std 2.
    let rows = [
        (1, 2.0),
        (2, 4.0),
        (3, 4.0),
        (4, 4.0),
        (5, 5.0),
        (6, 5.0),
        (7, 7.0),
        (8, 9.0),
    ];
    let std = run(&rows, |v| WindowOps::std(v, 8usize));
    let var = run(&rows, |v| WindowOps::variance(v, 8usize));
    close(std[7], 2.0);
    close(var[7], 4.0);
    // First value: <2 samples -> warming up -> NaN.
    assert!(std[0].is_nan());
    assert!(var[0].is_nan());
}

#[test]
fn max_min_sum_count_count_window() {
    let rows = [(1, 3.0), (2, 1.0), (3, 7.0), (4, 2.0), (5, 5.0)];
    let max = run(&rows, |v| WindowOps::max(v, 3usize));
    let min = run(&rows, |v| WindowOps::min(v, 3usize));
    let sum = run(&rows, |v| WindowOps::sum(v, 3usize));
    let cnt = run(&rows, |v| WindowOps::count(v, 3usize));
    // window contents per step: [3] [3,1] [3,1,7] [1,7,2] [7,2,5]
    assert_eq!(max, vec![3.0, 3.0, 7.0, 7.0, 7.0]);
    assert_eq!(min, vec![3.0, 1.0, 1.0, 1.0, 2.0]);
    assert_eq!(sum, vec![3.0, 4.0, 11.0, 10.0, 14.0]);
    assert_eq!(cnt, vec![1.0, 2.0, 3.0, 3.0, 3.0]);
}

// ============================================================================
// Moving averages: EMA, WMA
// ============================================================================

#[test]
fn ema_count_window_seeds_then_smooths() {
    // CountEma(period=3): warming for the first 2, seeds with the mean of the
    // first 3, then smooths with alpha = 2/(3+1) = 0.5.
    let rows = [(1, 10.0), (2, 20.0), (3, 30.0), (4, 40.0)];
    let out = run(&rows, |v| WindowOps::ema(v, 3usize));
    assert!(out[0].is_nan());
    assert!(out[1].is_nan());
    close(out[2], 20.0); // seed = mean(10,20,30)
    close(out[3], 0.5 * 40.0 + 0.5 * 20.0); // 30.0
}

#[test]
fn ema_constant_series_is_constant() {
    let rows = [(1, 42.0), (2, 42.0), (3, 42.0), (4, 42.0)];
    let out = run(&rows, |v| WindowOps::ema(v, 2usize));
    close(out[3], 42.0);
}

#[test]
fn wma_count_window() {
    // WMA(3) over [1,2,3]: weights 1,2,3 -> (1+4+9)/6 = 2.3333…
    let rows = [(1, 1.0), (2, 2.0), (3, 3.0)];
    let out = run(&rows, |v| WindowOps::wma(v, 3usize));
    close(out[2], (1.0 * 1.0 + 2.0 * 2.0 + 3.0 * 3.0) / 6.0);
}

#[test]
fn wma_constant_series_is_constant() {
    let rows = [(1, 42.0), (2, 42.0), (3, 42.0)];
    let out = run(&rows, |v| WindowOps::wma(v, 3usize));
    close(out[2], 42.0);
}

// ============================================================================
// RSI: window-based and Wilder-smoothed
// ============================================================================

#[test]
fn rsi_all_gains_is_100() {
    let rows = [(1, 10.0), (2, 11.0), (3, 12.0), (4, 13.0), (5, 14.0)];
    let out = run(&rows, |v| WindowOps::rsi(v, 4usize));
    close(*out.last().unwrap(), 100.0);
}

#[test]
fn rsi_all_losses_is_0() {
    let rows = [(1, 14.0), (2, 13.0), (3, 12.0), (4, 11.0), (5, 10.0)];
    let out = run(&rows, |v| WindowOps::rsi(v, 4usize));
    close(*out.last().unwrap(), 0.0);
}

#[test]
fn rsi_wilder_all_gains_is_100() {
    // Wilder RSI over a strictly increasing series: avg_loss == 0 -> 100.
    let rows = [
        (1, 10.0),
        (2, 11.0),
        (3, 12.0),
        (4, 13.0),
        (5, 14.0),
        (6, 15.0),
    ];
    let out = run(&rows, |v| WindowOps::rsi_wilder_n(v, 3));
    close(*out.last().unwrap(), 100.0);
}

#[test]
fn rsi_wilder_warms_up_for_period_changes() {
    // period=3 needs the seed value plus 3 changes before producing output.
    let rows = [(1, 10.0), (2, 11.0), (3, 12.0), (4, 11.0)];
    let out = run(&rows, |v| WindowOps::rsi_wilder_n(v, 3));
    assert!(out[0].is_nan()); // seed
    assert!(out[1].is_nan()); // change 1
    assert!(out[2].is_nan()); // change 2
    // change 3 completes the seed window: gains 1,1 losses 1 -> avg_gain 2/3,
    // avg_loss 1/3 -> RSI = 100 - 100/(1 + 2) = 66.666…
    close(out[3], 100.0 - 100.0 / (1.0 + 2.0));
}

// ============================================================================
// Distribution statistics: median, quantile, rank
// ============================================================================

#[test]
fn median_count_window() {
    // Window of 3, contents per step: [3] [3,1] [3,1,2] [1,2,5] [2,5,4]
    let rows = [(1, 3.0), (2, 1.0), (3, 2.0), (4, 5.0), (5, 4.0)];
    let out = run(&rows, |v| WindowOps::median(v, 3usize));
    assert_eq!(out, vec![3.0, 2.0, 2.0, 2.0, 4.0]);
}

#[test]
fn quantile_at_half_equals_median() {
    let rows = [(1, 3.0), (2, 1.0), (3, 2.0), (4, 5.0), (5, 4.0)];
    let med = run(&rows, |v| WindowOps::median(v, 3usize));
    let q50 = run(&rows, |v| WindowOps::quantile(v, 3usize, 0.5));
    assert_eq!(med, q50);
}

#[test]
fn quantile_extremes_are_min_and_max() {
    // sorted last 3: [1,2,3] -> q0 = 1, q1 = 3.
    let rows = [(1, 3.0), (2, 1.0), (3, 2.0)];
    let q0 = run(&rows, |v| WindowOps::quantile(v, 3usize, 0.0));
    let q1 = run(&rows, |v| WindowOps::quantile(v, 3usize, 1.0));
    close(q0[2], 1.0);
    close(q1[2], 3.0);
}

#[test]
fn rank_of_current_value() {
    // Window of 3. current_rank = (# values strictly less) / (n-1).
    // step 3: window [3,1,2], current=2 -> 1 value < 2 -> 1/2 = 0.5
    // step 4: window [1,2,5], current=5 -> 2 values < 5 -> 2/2 = 1.0
    // step 5: window [2,5,4], current=4 -> 1 value < 4 -> 1/2 = 0.5
    let rows = [(1, 3.0), (2, 1.0), (3, 2.0), (4, 5.0), (5, 4.0)];
    let out = run(&rows, |v| WindowOps::rank(v, 3usize));
    close(out[2], 0.5);
    close(out[3], 1.0);
    close(out[4], 0.5);
}

// ============================================================================
// Higher moments: skewness, kurtosis
// ============================================================================

#[test]
fn skewness_count_window() {
    // Population skewness of [1,2,3,10] (right-skewed) ~ 1.0182.
    let rows = [(1, 1.0), (2, 2.0), (3, 3.0), (4, 10.0)];
    let out = run(&rows, |v| WindowOps::skewness(v, 4usize));
    assert!(out[0].is_nan()); // <3 samples
    assert!(out[1].is_nan());
    assert!((out[3] - 1.018_233_764_908_628_4).abs() < 1e-6);
}

#[test]
fn kurtosis_count_window() {
    // Population excess kurtosis of [1,2,3,10] ~ -0.7696.
    let rows = [(1, 1.0), (2, 2.0), (3, 3.0), (4, 10.0)];
    let out = run(&rows, |v| WindowOps::kurtosis(v, 4usize));
    assert!(out[0].is_nan()); // <4 samples
    assert!(out[2].is_nan());
    assert!((out[3] - (-0.769_600_000_000_000_1)).abs() < 1e-6);
}

// ============================================================================
// Bivariate: correlation, covariance
// ============================================================================

/// Run a two-input bivariate operator over a `(ts, a, b)` series.
fn run_pair<F>(rows: &[(i64, f64, f64)], build: F) -> Vec<f64>
where
    F: FnOnce(&Comp<Pair, f64>, &Comp<Pair, f64>) -> Comp<Pair, f64>,
{
    let data: Vec<Pair> = rows.iter().map(|&(ts, a, b)| Pair { ts, a, b }).collect();
    data.into_iter()
        .tflo(|t| {
            let _ = t.timestamp(|x| x.ts);
            let a = t.prop(|x| x.a);
            let b = t.prop(|x| x.b);
            build(&a, &b)
        })
        .collect()
}

#[test]
fn covariance_count_window() {
    // A = [1,2,3,4], B = [2,4,6,8] -> population covariance:
    //   sum_xy/n - mean_x*mean_y = 60/4 - 2.5*5.0 = 2.5
    let rows = [(1, 1.0, 2.0), (2, 2.0, 4.0), (3, 3.0, 6.0), (4, 4.0, 8.0)];
    let out = run_pair(&rows, |a, b| WindowOps::covariance(a, b, 4usize));
    close(out[3], 2.5);
}

#[test]
fn correlation_perfect_linear_is_one() {
    // B is a strictly increasing linear function of A -> correlation 1.0.
    let rows = [(1, 1.0, 2.0), (2, 2.0, 4.0), (3, 3.0, 6.0), (4, 4.0, 8.0)];
    let out = run_pair(&rows, |a, b| WindowOps::correlation(a, b, 4usize));
    close(out[3], 1.0);
}

#[test]
fn correlation_perfect_inverse_is_negative_one() {
    let rows = [(1, 1.0, 8.0), (2, 2.0, 6.0), (3, 3.0, 4.0), (4, 4.0, 2.0)];
    let out = run_pair(&rows, |a, b| WindowOps::correlation(a, b, 4usize));
    close(out[3], -1.0);
}
