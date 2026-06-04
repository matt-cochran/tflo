//! Event-detector operators and the [`CrossOps`] / [`DetectorOps`] extension
//! traits.
//!
//! These operators produce *typed* (non-`f64`) outputs — a crossing direction,
//! a pulse classification, a window-transition event. Each is a hand-written
//! [`Operator`] wrapping the matching `tflo_core::primitives` detector struct:
//!
//! - `CrossOp` wraps [`crate::primitives::CrossDetector`] for `cross` /
//!   `cross_above` / `cross_under` (the variant selects which `update*`
//!   method runs).
//! - `CrossHysteresisOp` wraps [`crate::primitives::HysteresisCrossDetector`].
//! - `GlitchOp` wraps [`crate::primitives::GlitchFilter`], `RuntOp`
//!   wraps [`crate::primitives::RuntDetector`], `PulseWidthOp` wraps
//!   [`crate::primitives::PulseWidthDetector`], `WindowDetectOp` wraps
//!   [`crate::primitives::WindowDetector`].
//!
//! The step logic is ported verbatim from the legacy `tflo-core` catalog
//! (`compile/eval/helpers.rs`). The **absent-input semantics**, however, are
//! tightened from the oracle: where the legacy arms substituted `f64::NAN` for
//! an absent input — which silently advanced detector state with a poisoned
//! sample — these operators short-circuit. An absent input emits the
//! detector's "no event" variant (`ThresholdCrossEventMode::None`,
//! `GlitchResult::NoTransition`, `Option::<…>::None`) without invoking
//! `detector.update`, so the next present record sees the prior level rather
//! than NaN-polluted state. The detector's own output values remain
//! bit-identical to the old catalog for all-present input streams.
//!
//! # Layout (post decomposition)
//!
//! - `cross.rs`        — `CrossOp` + `CrossHysteresisOp` + `CrossMode`
//! - `glitch.rs`       — `GlitchOp`
//! - `runt.rs`         — `RuntOp`
//! - `pulse_width.rs`  — `PulseWidthOp`
//! - `window.rs`       — `WindowDetectOp`
//!
//! This `mod.rs` keeps the public [`CrossOps`] and [`DetectorOps`] extension
//! traits plus their blanket impls on `Comp<R, f64>`.
//!
//! Every method is exposed on `Comp<R, f64>` through the [`CrossOps`] and
//! [`DetectorOps`] extension traits, mirroring
//! [`WindowOps`](crate::ops::windows::WindowOps).

mod cross;
mod glitch;
mod pulse_width;
mod runt;
mod window;

use crate::events::{
    GlitchResult, PulseWidthResult, RuntResult, ThresholdCrossEventMode, WindowEvent,
};
use crate::primitives::{
    GlitchFilter, HysteresisCrossDetector, PulseWidthDetector, RuntDetector, WindowDetector,
};
use cross::{CrossHysteresisOp, CrossMode, CrossOp};
use glitch::GlitchOp;
use pulse_width::PulseWidthOp;
use runt::RuntOp;
use tflo_core::comp::Comp;
use tflo_core::operator::{BoxedOperator, Operator};
use window::WindowDetectOp;

// ============================================================================
// CrossOps extension trait
// ============================================================================

/// Box an operator into a [`BoxedOperator`] for a `_dyn` plugin factory.
fn boxed<O: Operator>(op: O) -> BoxedOperator {
    Box::new(op)
}

/// Cross-detection and comparison operations on `Comp`.
///
/// The `cross*` family produces a typed [`ThresholdCrossEventMode`] output;
/// the `gt` / `gte` / `lt` / `lte` comparisons produce a plain `f64`
/// (`1.0` for true, `0.0` for false). The single blanket impl below adds every
/// method to `Comp<R, f64>`.
pub trait CrossOps<R> {
    /// Detect when this value crosses the other (either direction).
    ///
    /// Emits [`ThresholdCrossEventMode::Rising`] on a cross above and
    /// [`ThresholdCrossEventMode::Falling`] on a cross below.
    fn cross(&self, other: &Comp<R, f64>) -> Comp<R, ThresholdCrossEventMode>;
    /// Detect when this value crosses above the other.
    fn cross_above(&self, other: &Comp<R, f64>) -> Comp<R, ThresholdCrossEventMode>;
    /// Detect when this value crosses below the other.
    fn cross_under(&self, other: &Comp<R, f64>) -> Comp<R, ThresholdCrossEventMode>;
    /// Detect when this value crosses a threshold with hysteresis.
    ///
    /// Hysteresis prevents "chatter" by requiring the signal to move beyond
    /// the threshold by `margin` before triggering.
    fn cross_hysteresis(
        &self,
        threshold: &Comp<R, f64>,
        margin: f64,
    ) -> Comp<R, ThresholdCrossEventMode>;
    /// Greater-than comparison. `1.0` if true, `0.0` if false.
    fn gt(&self, other: &Comp<R, f64>) -> Comp<R, f64>;
    /// Greater-than-or-equal comparison. `1.0` if true, `0.0` if false.
    fn gte(&self, other: &Comp<R, f64>) -> Comp<R, f64>;
    /// Less-than comparison. `1.0` if true, `0.0` if false.
    fn lt(&self, other: &Comp<R, f64>) -> Comp<R, f64>;
    /// Less-than-or-equal comparison. `1.0` if true, `0.0` if false.
    fn lte(&self, other: &Comp<R, f64>) -> Comp<R, f64>;
}

impl<R: 'static> CrossOps<R> for Comp<R, f64> {
    fn cross(&self, other: &Self) -> Comp<R, ThresholdCrossEventMode> {
        Self::custom_node_dyn(self, &[other], || boxed(CrossOp::new(CrossMode::Both)))
    }

    fn cross_above(&self, other: &Self) -> Comp<R, ThresholdCrossEventMode> {
        Self::custom_node_dyn(self, &[other], || boxed(CrossOp::new(CrossMode::Above)))
    }

    fn cross_under(&self, other: &Self) -> Comp<R, ThresholdCrossEventMode> {
        Self::custom_node_dyn(self, &[other], || boxed(CrossOp::new(CrossMode::Under)))
    }

    fn cross_hysteresis(&self, threshold: &Self, margin: f64) -> Comp<R, ThresholdCrossEventMode> {
        Self::custom_node_dyn(self, &[threshold], move || {
            boxed(CrossHysteresisOp::new(HysteresisCrossDetector::new(margin)))
        })
    }

    // The comparisons are plain stateless closures — ported from the legacy
    // `NodeOp::Gt`/`Gte`/`Lt`/`Lte` eval arms, which emit `1.0` for true and
    // `0.0` for false. `map2_f64` already short-circuits an absent input.
    fn gt(&self, other: &Self) -> Self {
        self.map2_f64(other, |x, y| if x > y { 1.0 } else { 0.0 })
    }

    fn gte(&self, other: &Self) -> Self {
        self.map2_f64(other, |x, y| if x >= y { 1.0 } else { 0.0 })
    }

    fn lt(&self, other: &Self) -> Self {
        self.map2_f64(other, |x, y| if x < y { 1.0 } else { 0.0 })
    }

    fn lte(&self, other: &Self) -> Self {
        self.map2_f64(other, |x, y| if x <= y { 1.0 } else { 0.0 })
    }
}

// ============================================================================
// DetectorOps extension trait
// ============================================================================

/// Pulse / window event-detector operations on `Comp`.
///
/// Every method takes the detector's threshold parameters and produces a typed
/// event output. The single blanket impl below adds every method to
/// `Comp<R, f64>`.
pub trait DetectorOps<R> {
    /// Filter out pulses shorter than `min_duration_ms` (glitch rejection).
    ///
    /// A pulse is "high" while the value is `>= threshold`. When it ends, the
    /// output is [`GlitchResult::ValidPulse`] if the pulse lasted at least
    /// `min_duration_ms`, else [`GlitchResult::Rejected`]; steps with no pulse
    /// transition emit [`GlitchResult::NoTransition`].
    fn glitch_filter(&self, threshold: f64, min_duration_ms: i64) -> Comp<R, GlitchResult>;
    /// Detect runt pulses — pulses that cross `low` but never reach `high`.
    ///
    /// Emits `Some(RuntResult)` when a pulse completes (returns below `low`),
    /// `None` otherwise.
    fn runt_detect(&self, low: f64, high: f64) -> Comp<R, Option<RuntResult>>;
    /// Classify a completed pulse's width against `[min_width_ms, max_width_ms]`.
    ///
    /// Emits `Some(PulseWidthResult)` when a pulse completes, `None` otherwise.
    fn pulse_width(
        &self,
        threshold: f64,
        min_width_ms: i64,
        max_width_ms: i64,
    ) -> Comp<R, Option<PulseWidthResult>>;
    /// Detect when the value enters or exits the `[low, high]` amplitude window.
    ///
    /// Emits `Some(WindowEvent)` on a transition, `None` otherwise.
    fn window_detect(&self, low: f64, high: f64) -> Comp<R, Option<WindowEvent>>;
}

impl<R: 'static> DetectorOps<R> for Comp<R, f64> {
    fn glitch_filter(&self, threshold: f64, min_duration_ms: i64) -> Comp<R, GlitchResult> {
        Self::custom_node1_dyn(self, move || {
            boxed(GlitchOp::new(GlitchFilter::new(threshold, min_duration_ms)))
        })
    }

    fn runt_detect(&self, low: f64, high: f64) -> Comp<R, Option<RuntResult>> {
        Self::custom_node1_dyn(self, move || {
            boxed(RuntOp::new(RuntDetector::new(low, high)))
        })
    }

    fn pulse_width(
        &self,
        threshold: f64,
        min_width_ms: i64,
        max_width_ms: i64,
    ) -> Comp<R, Option<PulseWidthResult>> {
        Self::custom_node1_dyn(self, move || {
            boxed(PulseWidthOp::new(PulseWidthDetector::new(
                threshold,
                min_width_ms,
                max_width_ms,
            )))
        })
    }

    fn window_detect(&self, low: f64, high: f64) -> Comp<R, Option<WindowEvent>> {
        Self::custom_node1_dyn(self, move || {
            boxed(WindowDetectOp::new(WindowDetector::new(low, high)))
        })
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
#[allow(clippy::indexing_slicing)]
mod tests {
    use super::*;
    use crate::events::GlitchResult;

    /// Drive `(ts, value)` rows through a stateful 1-input detector op via
    /// [`Operator::eval`], cloning the boxed typed output of each step.
    fn drive_typed<O: Clone + 'static>(op: &mut dyn Operator, rows: &[(i64, f64)]) -> Vec<O> {
        rows.iter()
            .map(|&(ts, v)| {
                op.eval(&[Ok(v)], ts)
                    .as_any()
                    .downcast_ref::<O>()
                    .expect("typed output")
                    .clone()
            })
            .collect()
    }

    #[test]
    fn glitch_filter_checkpoint_round_trip() {
        // A series with a pulse straddling the save/load boundary, so the
        // restored operator must carry the in-pulse state across the checkpoint.
        let series: &[(i64, f64)] = &[
            (0, 110.0),  // pulse starts
            (3, 110.0),  // still high (mid-pulse) — checkpoint taken here
            (12, 90.0),  // pulse ends at 12ms -> ValidPulse
            (20, 110.0), // new pulse starts
            (22, 90.0),  // ends at 2ms -> Rejected
        ];

        let mut reference = GlitchOp::new(GlitchFilter::new(100.0, 5));
        let reference_out: Vec<GlitchResult> = drive_typed(&mut reference, series);

        let mut original = GlitchOp::new(GlitchFilter::new(100.0, 5));
        let first_half: Vec<GlitchResult> = drive_typed(&mut original, &series[..2]);
        let bytes = original.save().expect("save should succeed");

        let mut restored = GlitchOp::new(GlitchFilter::new(100.0, 5));
        restored.load(&bytes).expect("load should succeed");
        let second_half: Vec<GlitchResult> = drive_typed(&mut restored, &series[2..]);

        let resumed: Vec<GlitchResult> = first_half.into_iter().chain(second_half).collect();
        assert_eq!(resumed, reference_out);
    }
}
