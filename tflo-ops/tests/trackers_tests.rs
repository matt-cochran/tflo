//! Behavioural tests for the stateful-tracker operators in
//! [`tflo_ops::ops::trackers`].
//!
//! Each test drives a known series through a tracker operator and asserts the
//! per-record output, including the typed [`Absent`] reasons. The expected
//! values are the behavioural oracle ported from the legacy `tflo-core`
//! catalog (`compile/eval/helpers.rs`): the `StatefulTracker` step functions
//! wrap the exact same state primitives the old `NodeOp` arms used, so results
//! are bit-identical.
//!
//! Most tests drive the operator directly through [`Operator::eval`] so the
//! typed [`Absent`] reason is observable. Several tests additionally drive the
//! operator end-to-end through the real `tflo` builder via the [`StatefulOps`]
//! extension trait.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::time::Duration;
use tflo_core::compile::Absent;
use tflo_core::operator::Operator;
use tflo_core::prelude::*;
use tflo_ops::ops::trackers::StatefulOps;
use tflo_ops::shapes::StatefulTracker;

// ============================================================================
// Helpers
// ============================================================================

/// Absolute tolerance for floating-point comparisons.
const EPS: f64 = 1e-9;

/// Feed `(ts, value)` rows through `op` via `eval`, collecting per-step
/// [`Computed`] results.
fn drive(op: &mut dyn Operator, rows: &[(i64, f64)]) -> Vec<Computed> {
    rows.iter()
        .map(|&(ts, v)| op.eval(&[Ok(v)], ts).as_computed().unwrap())
        .collect()
}

/// Assert a [`Computed`] is `Ok` and close to `want`.
fn ok_close(got: Computed, want: f64) {
    match got {
        Ok(v) => assert!(
            (v - want).abs() < EPS,
            "expected Ok({want}), got Ok({v}) (delta {})",
            (v - want).abs()
        ),
        Err(e) => panic!("expected Ok({want}), got Err({e:?})"),
    }
}

/// A single-channel record: timestamp + one value.
#[derive(Clone)]
struct Rec {
    ts: i64,
    v: f64,
}

/// Run a tracker end-to-end through the real builder, collecting per-record
/// `f64` outputs (absent results become `NaN`).
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
// Lookback
// ============================================================================

#[test]
fn prev_first_record_is_warming_up() {
    let mut op = StatefulTracker::new(
        tflo_core::primitives::PrevTracker::new(),
        tflo_ops::ops::trackers::PrevStep,
    );
    let out = drive(&mut op, &[(1, 10.0), (2, 20.0), (3, 30.0)]);
    assert_eq!(out[0], Err(Absent::WarmingUp));
    ok_close(out[1], 10.0);
    ok_close(out[2], 20.0);
}

#[test]
fn prev_via_builder() {
    let out = run(&[(1, 10.0), (2, 20.0), (3, 30.0)], StatefulOps::prev);
    assert!(out[0].is_nan());
    assert!((out[1] - 10.0).abs() < EPS);
    assert!((out[2] - 20.0).abs() < EPS);
}

#[test]
fn prev_by_tracks_previous_value_per_key() {
    use tflo_ops::ops::trackers::PrevByOp;

    // Interleaved two-key series: key 0 = {10, 11, 12}, key 1 = {20, 21}.
    // Each record's output is the previous value *for its own key*; the
    // first record of each key warms up.
    //   inputs[0] = value, inputs[1] = key.
    let rows: &[(f64, f64)] = &[
        (10.0, 0.0), // key 0, first  -> WarmingUp
        (20.0, 1.0), // key 1, first  -> WarmingUp
        (11.0, 0.0), // key 0, second -> prev 10.0
        (21.0, 1.0), // key 1, second -> prev 20.0
        (12.0, 0.0), // key 0, third  -> prev 11.0
    ];
    let mut op = PrevByOp::default();
    let out: Vec<Computed> = rows
        .iter()
        .map(|&(v, k)| op.eval(&[Ok(v), Ok(k)], 0).as_computed().unwrap())
        .collect();

    assert_eq!(out[0], Err(Absent::WarmingUp));
    assert_eq!(out[1], Err(Absent::WarmingUp));
    ok_close(out[2], 10.0);
    ok_close(out[3], 20.0);
    ok_close(out[4], 11.0);
}

#[test]
fn prev_by_absent_value_or_key_skips_the_step() {
    use tflo_ops::ops::trackers::PrevByOp;

    let mut op = PrevByOp::default();
    // Seed key 0 with 10.0.
    assert_eq!(
        op.eval(&[Ok(10.0), Ok(0.0)], 0).as_computed().unwrap(),
        Err(Absent::WarmingUp)
    );
    // Absent value: must not mutate key 0's stored previous.
    assert_eq!(
        op.eval(&[Err(Absent::WarmingUp), Ok(0.0)], 0)
            .as_computed()
            .unwrap(),
        Err(Absent::WarmingUp)
    );
    // Absent key: must not mutate any partition.
    assert_eq!(
        op.eval(&[Ok(99.0), Err(Absent::WarmingUp)], 0)
            .as_computed()
            .unwrap(),
        Err(Absent::WarmingUp)
    );
    // The next real key-0 record still sees the original 10.0 — the skipped
    // steps left the map untouched.
    ok_close(
        op.eval(&[Ok(11.0), Ok(0.0)], 0).as_computed().unwrap(),
        10.0,
    );
}

#[test]
fn prev_by_via_builder() {
    // A two-key record: `g` selects the partition, `v` the value.
    #[derive(Clone)]
    struct KeyedRec {
        ts: i64,
        g: f64,
        v: f64,
    }

    // Interleaved keys 0 and 1, mirroring the legacy `test_prev_by_key`
    // oracle: the first record of each key warms up (NaN), later records get
    // the previous value scoped to their own key.
    let rows = [
        (1, 0.0, 150.0),  // key 0, first  -> NaN
        (2, 1.0, 2800.0), // key 1, first  -> NaN
        (3, 0.0, 151.0),  // key 0, second -> 150.0
        (4, 1.0, 2810.0), // key 1, second -> 2800.0
    ];
    let out: Vec<f64> = rows
        .iter()
        .map(|&(ts, g, v)| KeyedRec { ts, g, v })
        .collect::<Vec<_>>()
        .into_iter()
        .tflo(|t| {
            let _ = t.timestamp(|x| x.ts);
            let v = t.prop(|x| x.v);
            // UFCS: the legacy `tflo-core` inherent `Comp::prev_by` shadows the
            // `StatefulOps` trait method under plain method-call syntax, so the
            // extension-trait op is reached explicitly — exactly as the other
            // builder tests reach `StatefulOps::prev` etc.
            StatefulOps::prev_by(&v, |x| x.g)
        })
        .collect();

    assert!(out[0].is_nan());
    assert!(out[1].is_nan());
    assert!((out[2] - 150.0).abs() < EPS);
    assert!((out[3] - 2800.0).abs() < EPS);
}

#[test]
fn prev_by_checkpoint_round_trip() {
    use tflo_ops::ops::trackers::PrevByOp;

    // Interleaved three-key series; the per-key map must survive save/load.
    let series: &[(f64, f64)] = &[
        (10.0, 0.0),
        (20.0, 1.0),
        (30.0, 2.0),
        (11.0, 0.0),
        (21.0, 1.0),
        (31.0, 2.0),
        (12.0, 0.0),
    ];

    let eval_one = |op: &mut PrevByOp, v: f64, k: f64| -> f64 {
        op.eval(&[Ok(v), Ok(k)], 0)
            .as_computed()
            .unwrap()
            .unwrap_or(f64::NAN)
    };

    let mut reference = PrevByOp::default();
    let reference_out: Vec<f64> = series
        .iter()
        .map(|&(v, k)| eval_one(&mut reference, v, k))
        .collect();

    let mut original = PrevByOp::default();
    let first_half: Vec<f64> = series[..4]
        .iter()
        .map(|&(v, k)| eval_one(&mut original, v, k))
        .collect();
    let bytes = original.save().expect("save should succeed");

    let mut restored = PrevByOp::default();
    restored.load(&bytes).expect("load should succeed");
    let second_half: Vec<f64> = series[4..]
        .iter()
        .map(|&(v, k)| eval_one(&mut restored, v, k))
        .collect();

    let resumed: Vec<f64> = first_half.into_iter().chain(second_half).collect();
    assert_series_eq(&resumed, &reference_out);
}

#[test]
fn lag_returns_value_from_the_past() {
    // 5s lag: at ts=6000 target ts=1000 -> 100.0; at ts=7000 -> 110.0.
    let out = run(
        &[
            (1000, 100.0),
            (2000, 110.0),
            (3000, 120.0),
            (6000, 130.0),
            (7000, 140.0),
        ],
        |v| StatefulOps::lag(v, Duration::from_secs(5)),
    );
    assert!(out[0].is_nan());
    assert!(out[1].is_nan());
    assert!(out[2].is_nan());
    assert!((out[3] - 100.0).abs() < EPS);
    assert!((out[4] - 110.0).abs() < EPS);
}

#[test]
fn delta_first_record_is_warming_up() {
    // 5s lag: delta at ts=6000 is 150 - 100 = 50.
    let mut op = StatefulTracker::new(
        tflo_core::primitives::LagBuffer::new(Duration::from_secs(5)),
        tflo_ops::ops::trackers::DeltaStep,
    );
    let out = drive(&mut op, &[(1000, 100.0), (6000, 150.0)]);
    assert_eq!(out[0], Err(Absent::WarmingUp));
    ok_close(out[1], 50.0);
}

// ============================================================================
// Cumulative
// ============================================================================

#[test]
fn cumsum_running_total() {
    let out = run(&[(1, 10.0), (2, 20.0), (3, -5.0)], StatefulOps::cumsum);
    assert!((out[0] - 10.0).abs() < EPS);
    assert!((out[1] - 30.0).abs() < EPS);
    assert!((out[2] - 25.0).abs() < EPS);
}

#[test]
fn cummax_high_water_mark() {
    let out = run(
        &[(1, 10.0), (2, 5.0), (3, 15.0), (4, 12.0)],
        StatefulOps::cummax,
    );
    assert_eq!(out, vec![10.0, 10.0, 15.0, 15.0]);
}

#[test]
fn cummin_running_minimum() {
    let out = run(
        &[(1, 10.0), (2, 15.0), (3, 5.0), (4, 8.0)],
        StatefulOps::cummin,
    );
    assert_eq!(out, vec![10.0, 10.0, 5.0, 5.0]);
}

#[test]
fn cumprod_running_product() {
    let out = run(&[(1, 2.0), (2, 3.0), (3, 0.5)], StatefulOps::cumprod);
    assert!((out[0] - 2.0).abs() < EPS);
    assert!((out[1] - 6.0).abs() < EPS);
    assert!((out[2] - 3.0).abs() < EPS);
}

// ============================================================================
// Returns
// ============================================================================

#[test]
fn pct_change_warming_up_then_percentage() {
    let mut op = StatefulTracker::new(
        tflo_ops::ops::trackers::PctChangeState::default(),
        tflo_ops::ops::trackers::PctChangeStep,
    );
    // (100 -> 110): +10%. (110 -> 99): -10%.
    let out = drive(&mut op, &[(1, 100.0), (2, 110.0), (3, 99.0)]);
    assert_eq!(out[0], Err(Absent::WarmingUp));
    ok_close(out[1], 10.0);
    ok_close(out[2], -10.0);
}

#[test]
fn pct_change_from_zero_is_divide_by_zero() {
    let mut op = StatefulTracker::new(
        tflo_ops::ops::trackers::PctChangeState::default(),
        tflo_ops::ops::trackers::PctChangeStep,
    );
    let out = drive(&mut op, &[(1, 0.0), (2, 5.0)]);
    assert_eq!(out[0], Err(Absent::WarmingUp));
    assert_eq!(out[1], Err(Absent::DivideByZero));
}

#[test]
fn log_return_warming_up_then_ln_ratio() {
    let mut op = StatefulTracker::new(
        tflo_ops::ops::trackers::LogReturnState::default(),
        tflo_ops::ops::trackers::LogReturnStep,
    );
    let out = drive(&mut op, &[(1, 100.0), (2, 110.0)]);
    assert_eq!(out[0], Err(Absent::WarmingUp));
    ok_close(out[1], (110.0_f64 / 100.0).ln());
}

#[test]
fn log_return_from_non_positive_is_domain_error() {
    let mut op = StatefulTracker::new(
        tflo_ops::ops::trackers::LogReturnState::default(),
        tflo_ops::ops::trackers::LogReturnStep,
    );
    // prev = -1.0 (non-positive) -> DomainError on the next step.
    let out = drive(&mut op, &[(1, -1.0), (2, 5.0)]);
    assert_eq!(out[0], Err(Absent::WarmingUp));
    assert_eq!(out[1], Err(Absent::DomainError));
}

#[test]
fn log_return_zero_current_is_domain_error() {
    let mut op = StatefulTracker::new(
        tflo_ops::ops::trackers::LogReturnState::default(),
        tflo_ops::ops::trackers::LogReturnStep,
    );
    // prev = 100.0 (positive), current = 0.0 -> oracle guard `value > 0.0`
    // fails -> DomainError.
    let out = drive(&mut op, &[(1, 100.0), (2, 0.0)]);
    assert_eq!(out[0], Err(Absent::WarmingUp));
    assert_eq!(out[1], Err(Absent::DomainError));
}

// ============================================================================
// Rate-based derivatives
// ============================================================================

#[test]
fn rate_warming_up_then_per_second_rate() {
    let mut op = StatefulTracker::new(
        tflo_ops::ops::trackers::DerivativeState::default(),
        tflo_ops::ops::trackers::RateStep,
    );
    // (100 -> 200) over 1000ms -> (200-100)/1000*1000 = 100 per second.
    let out = drive(&mut op, &[(1000, 100.0), (2000, 200.0)]);
    assert_eq!(out[0], Err(Absent::WarmingUp));
    ok_close(out[1], 100.0);
}

#[test]
fn rate_zero_time_delta_is_zero_time_delta() {
    let mut op = StatefulTracker::new(
        tflo_ops::ops::trackers::DerivativeState::default(),
        tflo_ops::ops::trackers::RateStep,
    );
    // Two records at the same timestamp -> dt == 0.
    let out = drive(&mut op, &[(1000, 100.0), (1000, 200.0)]);
    assert_eq!(out[0], Err(Absent::WarmingUp));
    assert_eq!(out[1], Err(Absent::ZeroTimeDelta));
}

#[test]
fn velocity_matches_rate() {
    let out = run(&[(1000, 100.0), (2000, 200.0), (3000, 250.0)], |v| {
        StatefulOps::velocity(v, Duration::from_secs(1))
    });
    assert!(out[0].is_nan());
    assert!((out[1] - 100.0).abs() < EPS);
    assert!((out[2] - 50.0).abs() < EPS);
}

#[test]
fn acceleration_second_derivative() {
    let mut op = StatefulTracker::new(
        tflo_ops::ops::trackers::AccelerationState::default(),
        tflo_ops::ops::trackers::AccelerationStep,
    );
    // ts(ms): 0, 1000, 2000. values: 0, 100, 300.
    // velocities: -, 100, 200. acceleration: -, -, (200-100)/1000*1000 = 100.
    let out = drive(&mut op, &[(0, 0.0), (1000, 100.0), (2000, 300.0)]);
    assert_eq!(out[0], Err(Absent::WarmingUp));
    assert_eq!(out[1], Err(Absent::WarmingUp));
    ok_close(out[2], 100.0);
}

// ============================================================================
// Checkpoint round-trip
// ============================================================================

/// Drive a tracker through `rows` and collect per-step results, treating an
/// absent result as `NaN` for comparison.
fn drive_nan(op: &mut dyn Operator, rows: &[(i64, f64)]) -> Vec<f64> {
    drive(op, rows)
        .into_iter()
        .map(|c| c.unwrap_or(f64::NAN))
        .collect()
}

/// Assert two output sequences match, treating `NaN`/`NaN` as equal.
fn assert_series_eq(got: &[f64], want: &[f64]) {
    assert_eq!(got.len(), want.len(), "length mismatch");
    for (i, (&g, &w)) in got.iter().zip(want).enumerate() {
        assert!(
            (g.is_nan() && w.is_nan()) || (g - w).abs() < EPS,
            "step {i}: got {g}, want {w}"
        );
    }
}

#[test]
fn cumsum_checkpoint_round_trip() {
    let series = [(1, 10.0), (2, 20.0), (3, 5.0), (4, 7.0), (5, 3.0)];

    let mut reference = StatefulTracker::new(
        tflo_core::primitives::CumulativeSum::new(),
        tflo_ops::ops::trackers::CumSumStep,
    );
    let reference_out = drive_nan(&mut reference, &series);

    let mut original = StatefulTracker::new(
        tflo_core::primitives::CumulativeSum::new(),
        tflo_ops::ops::trackers::CumSumStep,
    );
    let first_half = drive_nan(&mut original, &series[..2]);
    let bytes = original.save().expect("save should succeed");

    let mut restored = StatefulTracker::new(
        tflo_core::primitives::CumulativeSum::new(),
        tflo_ops::ops::trackers::CumSumStep,
    );
    restored.load(&bytes).expect("load should succeed");
    let second_half = drive_nan(&mut restored, &series[2..]);

    let resumed: Vec<f64> = first_half.into_iter().chain(second_half).collect();
    assert_series_eq(&resumed, &reference_out);
}

#[test]
fn lag_checkpoint_round_trip_preserves_duration() {
    // The lag duration lives in the serialized `LagBuffer`, so a checkpoint
    // restored into a default-but-distinct buffer would diverge — instead the
    // restored buffer carries the original 5s lag and the buffered history.
    let series = [
        (1000, 100.0),
        (2000, 110.0),
        (3000, 120.0),
        (6000, 130.0),
        (7000, 140.0),
    ];

    let mut reference = StatefulTracker::new(
        tflo_core::primitives::LagBuffer::new(Duration::from_secs(5)),
        tflo_ops::ops::trackers::LagStep,
    );
    let reference_out = drive_nan(&mut reference, &series);

    let mut original = StatefulTracker::new(
        tflo_core::primitives::LagBuffer::new(Duration::from_secs(5)),
        tflo_ops::ops::trackers::LagStep,
    );
    let first_half = drive_nan(&mut original, &series[..2]);
    let bytes = original.save().expect("save should succeed");

    // Restore into a buffer with a *different* lag — `load` must overwrite it
    // with the serialized 5s lag for the resumed series to match.
    let mut restored = StatefulTracker::new(
        tflo_core::primitives::LagBuffer::new(Duration::from_secs(999)),
        tflo_ops::ops::trackers::LagStep,
    );
    restored.load(&bytes).expect("load should succeed");
    let second_half = drive_nan(&mut restored, &series[2..]);

    let resumed: Vec<f64> = first_half.into_iter().chain(second_half).collect();
    assert_series_eq(&resumed, &reference_out);
}

#[test]
fn rate_checkpoint_round_trip() {
    // A rate operator's checkpoint must preserve `prev_ts` and `prev_value`
    // so the resumed stream produces the same per-second rates as an
    // uninterrupted reference run.
    //
    // Series (ms timestamps): 0→100, 1000→200, 2000→250, 3000→280, 4000→320.
    // Reference rates: WarmingUp, 100/s, 50/s, 30/s, 40/s.
    let series: &[(i64, f64)] = &[
        (0, 100.0),
        (1000, 200.0),
        (2000, 250.0),
        (3000, 280.0),
        (4000, 320.0),
    ];

    let mut reference = StatefulTracker::new(
        tflo_ops::ops::trackers::DerivativeState::default(),
        tflo_ops::ops::trackers::RateStep,
    );
    let reference_out = drive_nan(&mut reference, series);

    // Drive the first two records, checkpoint, restore into a fresh operator,
    // then continue from record 3 onward.
    let mut original = StatefulTracker::new(
        tflo_ops::ops::trackers::DerivativeState::default(),
        tflo_ops::ops::trackers::RateStep,
    );
    let first_half = drive_nan(&mut original, &series[..2]);
    let bytes = original.save().expect("save should succeed");

    let mut restored = StatefulTracker::new(
        tflo_ops::ops::trackers::DerivativeState::default(),
        tflo_ops::ops::trackers::RateStep,
    );
    restored.load(&bytes).expect("load should succeed");
    let second_half = drive_nan(&mut restored, &series[2..]);

    let resumed: Vec<f64> = first_half.into_iter().chain(second_half).collect();
    assert_series_eq(&resumed, &reference_out);
}
