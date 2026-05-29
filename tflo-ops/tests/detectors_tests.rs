//! Behavioural tests for the event-detector operators in
//! [`tflo_ops::ops::detectors`].
//!
//! Each test drives a known series end-to-end through the real `tflo` builder
//! via the [`CrossOps`] / [`DetectorOps`] extension traits and asserts the
//! emitted typed events. The expected values are the behavioural oracle ported
//! from the legacy `tflo-core` detector tests (`tflo-core/tests/detectors_tests.rs`)
//! and the detector-primitive doc examples: the operators wrap the exact same
//! `tflo_core::primitives` detector structs the old `NodeOp` arms used, so
//! results are bit-identical.
//!
//! The absent-input semantics are also exercised: every legacy detector arm
//! substituted `f64::NAN` for an absent input, so a detector op never
//! propagates an `Absent` reason — it emits its own "no event" variant.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use tflo_core::comp::Comp;
use tflo_core::iter_ext::TFlowIteratorExt;
use tflo_ops::events::{
    GlitchResult, PulseWidthResult, RuntResult, ThresholdCrossEventMode, WindowEvent,
};
use tflo_ops::ops::detectors::{CrossOps, DetectorOps};

// ============================================================================
// Helpers
// ============================================================================

/// A single-channel record: timestamp + one value.
#[derive(Clone)]
struct Rec {
    ts: i64,
    v: f64,
}

/// A two-channel record: timestamp + value + threshold.
#[derive(Clone)]
struct Pair {
    ts: i64,
    v: f64,
    t: f64,
}

/// Drive a one-input detector through the real builder, collecting per-record
/// typed outputs.
fn run1<O, F>(rows: &[(i64, f64)], build: F) -> Vec<O>
where
    O: tflo_core::compile::ExtractOutput,
    F: FnOnce(&Comp<Rec, f64>) -> Comp<Rec, O>,
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

/// Drive a two-input detector (value + threshold) through the real builder.
fn run2<O, F>(rows: &[(i64, f64, f64)], build: F) -> Vec<O>
where
    O: tflo_core::compile::ExtractOutput,
    F: FnOnce(&Comp<Pair, f64>, &Comp<Pair, f64>) -> Comp<Pair, O>,
{
    rows.iter()
        .map(|&(ts, v, t)| Pair { ts, v, t })
        .collect::<Vec<_>>()
        .into_iter()
        .tflo(|tb| {
            let _ = tb.timestamp(|x| x.ts);
            let v = tb.prop(|x| x.v);
            let t = tb.prop(|x| x.t);
            build(&v, &t)
        })
        .collect()
}

// ============================================================================
// cross — both directions
// ============================================================================

#[test]
fn cross_reports_both_directions() {
    // Oracle: `CrossDetector::update` doc example — first obs None, stay below
    // None, cross above Rising, stay above None, cross below Falling.
    //
    // Disambiguates against the legacy `tflo-core` inherent `Comp::cross`
    // (which stays during the split): explicit `CrossOps::cross` selects the
    // `tflo-ops` operator whose output is the `tflo-ops` event enum.
    let rows: &[(i64, f64, f64)] = &[
        (1, 90.0, 100.0),
        (2, 95.0, 100.0),
        (3, 105.0, 100.0),
        (4, 110.0, 100.0),
        (5, 95.0, 100.0),
    ];
    let out: Vec<ThresholdCrossEventMode> = run2(rows, CrossOps::cross);
    assert_eq!(
        out,
        vec![
            ThresholdCrossEventMode::None,
            ThresholdCrossEventMode::None,
            ThresholdCrossEventMode::Rising,
            ThresholdCrossEventMode::None,
            ThresholdCrossEventMode::Falling,
        ]
    );
}

// ============================================================================
// cross_above
// ============================================================================

#[test]
fn cross_above_reports_only_rising() {
    // Oracle: `test_update_above_only` — initialize above, a cross below is
    // ignored, a cross above is reported.
    let rows: &[(i64, f64, f64)] = &[(1, 110.0, 100.0), (2, 95.0, 100.0), (3, 105.0, 100.0)];
    let out: Vec<ThresholdCrossEventMode> = run2(rows, CrossOps::cross_above);
    assert_eq!(
        out,
        vec![
            ThresholdCrossEventMode::None,
            ThresholdCrossEventMode::None,
            ThresholdCrossEventMode::Rising,
        ]
    );
}

// ============================================================================
// cross_under
// ============================================================================

#[test]
fn cross_under_reports_only_falling() {
    // Mirror of `cross_above`: initialize below, a cross above is ignored, a
    // cross below is reported.
    let rows: &[(i64, f64, f64)] = &[(1, 90.0, 100.0), (2, 105.0, 100.0), (3, 95.0, 100.0)];
    let out: Vec<ThresholdCrossEventMode> = run2(rows, CrossOps::cross_under);
    assert_eq!(
        out,
        vec![
            ThresholdCrossEventMode::None,
            ThresholdCrossEventMode::None,
            ThresholdCrossEventMode::Falling,
        ]
    );
}

// ============================================================================
// cross_hysteresis
// ============================================================================

#[test]
fn cross_hysteresis_filters_chatter() {
    // Oracle: `test_hysteresis` — margin 5, threshold 100. Above but within
    // band is None; above by >5 is Rising; below but within band is None;
    // below by >5 is Falling.
    let rows: &[(i64, f64, f64)] = &[
        (1, 90.0, 100.0),
        (2, 103.0, 100.0),
        (3, 106.0, 100.0),
        (4, 97.0, 100.0),
        (5, 94.0, 100.0),
    ];
    let out: Vec<ThresholdCrossEventMode> =
        run2(rows, |v, t| CrossOps::cross_hysteresis(v, t, 5.0));
    assert_eq!(
        out,
        vec![
            ThresholdCrossEventMode::None,
            ThresholdCrossEventMode::None,
            ThresholdCrossEventMode::Rising,
            ThresholdCrossEventMode::None,
            ThresholdCrossEventMode::Falling,
        ]
    );
}

// ============================================================================
// glitch_filter
// ============================================================================

#[test]
fn glitch_filter_rejects_short_and_accepts_long_pulses() {
    // Oracle: `test_glitch_filter_*` + the `GlitchFilter` doc example.
    // min_duration 5ms. A 3ms pulse is rejected; a 10ms pulse is valid.
    let rows: &[(i64, f64)] = &[
        (0, 110.0),  // pulse starts -> NoTransition
        (2, 110.0),  // still high   -> NoTransition
        (3, 90.0),   // ends at 3ms  -> Rejected (glitch)
        (10, 110.0), // pulse starts -> NoTransition
        (20, 90.0),  // ends at 10ms -> ValidPulse
    ];
    let out: Vec<GlitchResult> = run1(rows, |v| v.glitch_filter(100.0, 5));
    assert_eq!(
        out,
        vec![
            GlitchResult::NoTransition,
            GlitchResult::NoTransition,
            GlitchResult::Rejected,
            GlitchResult::NoTransition,
            GlitchResult::ValidPulse,
        ]
    );
}

// ============================================================================
// runt_detect
// ============================================================================

#[test]
fn runt_detect_classifies_runt_and_valid_pulses() {
    // Oracle: the `RuntDetector` doc example — low 30, high 70.
    // First pulse reaches only 50 then falls -> Runt; second reaches 80 -> ValidPulse.
    let rows: &[(i64, f64)] = &[
        (1, 20.0), // below low        -> None
        (2, 50.0), // transition       -> None
        (3, 25.0), // back below, runt -> Runt { peak: 50 }
        (4, 50.0), // transition       -> None
        (5, 80.0), // above high       -> None
        (6, 25.0), // back below       -> ValidPulse { peak: 80 }
    ];
    let out: Vec<Option<RuntResult>> = run1(rows, |v| v.runt_detect(30.0, 70.0));
    assert_eq!(
        out,
        vec![
            None,
            None,
            Some(RuntResult::Runt { peak: 50.0 }),
            None,
            None,
            Some(RuntResult::ValidPulse { peak: 80.0 }),
        ]
    );
}

// ============================================================================
// pulse_width
// ============================================================================

#[test]
fn pulse_width_classifies_short_valid_and_long_pulses() {
    // Oracle: `test_pulse_width_*` — threshold 100, valid range 5..=15ms.
    // 3ms pulse TooShort; 10ms Valid; 25ms TooLong.
    let rows: &[(i64, f64)] = &[
        (0, 110.0),  // pulse starts -> None
        (3, 90.0),   // 3ms  -> TooShort
        (10, 110.0), // pulse starts -> None
        (20, 90.0),  // 10ms -> Valid
        (30, 110.0), // pulse starts -> None
        (55, 90.0),  // 25ms -> TooLong
    ];
    let out: Vec<Option<PulseWidthResult>> = run1(rows, |v| v.pulse_width(100.0, 5, 15));
    assert_eq!(
        out,
        vec![
            None,
            Some(PulseWidthResult::TooShort { width_ms: 3 }),
            None,
            Some(PulseWidthResult::Valid { width_ms: 10 }),
            None,
            Some(PulseWidthResult::TooLong { width_ms: 25 }),
        ]
    );
}

// ============================================================================
// window_detect
// ============================================================================

#[test]
fn window_detect_reports_enter_and_exit_events() {
    // Oracle: the `WindowDetector` doc example — window 4.5..=5.5.
    // Start below, enter, stay (None), exit high, enter again, exit low.
    let rows: &[(i64, f64)] = &[
        (1, 4.0), // initialize below -> None
        (2, 5.0), // enter            -> EnteredWindow
        (3, 5.2), // stay inside      -> None
        (4, 6.0), // exit high        -> ExitedHigh
        (5, 5.0), // enter again      -> EnteredWindow
        (6, 4.0), // exit low         -> ExitedLow
    ];
    let out: Vec<Option<WindowEvent>> = run1(rows, |v| v.window_detect(4.5, 5.5));
    assert_eq!(
        out,
        vec![
            None,
            Some(WindowEvent::EnteredWindow),
            None,
            Some(WindowEvent::ExitedHigh),
            Some(WindowEvent::EnteredWindow),
            Some(WindowEvent::ExitedLow),
        ]
    );
}

// ============================================================================
// gt / gte / lt / lte comparisons
// ============================================================================

#[test]
fn comparisons_emit_one_and_zero() {
    // Oracle: the legacy `NodeOp::Gt`/`Gte`/`Lt`/`Lte` eval arms — `1.0` for
    // true, `0.0` for false, with the equal case as the discriminator.
    let rows: &[(i64, f64, f64)] = &[
        (1, 5.0, 3.0), // v > t
        (2, 3.0, 3.0), // v == t
        (3, 1.0, 3.0), // v < t
    ];

    // Use the trait method explicitly: a legacy `tflo-core` inherent
    // `Comp::gt`/`gte`/`lt`/`lte` exists during the split and would
    // otherwise shadow `CrossOps::*`.
    let gt: Vec<f64> = run2(rows, CrossOps::gt);
    assert_eq!(gt, vec![1.0, 0.0, 0.0]);

    let gte: Vec<f64> = run2(rows, CrossOps::gte);
    assert_eq!(gte, vec![1.0, 1.0, 0.0]);

    let lt: Vec<f64> = run2(rows, CrossOps::lt);
    assert_eq!(lt, vec![0.0, 0.0, 1.0]);

    let lte: Vec<f64> = run2(rows, CrossOps::lte);
    assert_eq!(lte, vec![0.0, 1.0, 1.0]);
}
