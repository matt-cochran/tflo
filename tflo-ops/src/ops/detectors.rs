//! Event-detector operators and the [`CrossOps`] / [`DetectorOps`] extension
//! traits.
//!
//! These operators produce *typed* (non-`f64`) outputs — a crossing direction,
//! a pulse classification, a window-transition event. Each is a hand-written
//! [`Operator`] wrapping the matching `tflo_core::primitives` detector struct:
//!
//! - [`CrossOp`] wraps [`CrossDetector`] for `cross` / `cross_above` /
//!   `cross_under` (the variant selects which `update*` method runs).
//! - [`CrossHysteresisOp`] wraps [`HysteresisCrossDetector`].
//! - [`GlitchOp`] wraps [`GlitchFilter`], [`RuntOp`] wraps [`RuntDetector`],
//!   [`PulseWidthOp`] wraps [`PulseWidthDetector`], [`WindowDetectOp`] wraps
//!   [`WindowDetector`].
//!
//! The step logic is ported verbatim from the legacy `tflo-core` catalog
//! (`compile/eval/helpers.rs`). In particular the **absent-input semantics
//! match the oracle exactly**: every detector arm substituted `f64::NAN` for an
//! absent input rather than propagating the [`Absent`] reason, so these
//! operators do the same — an absent input is fed to the detector as `NaN`,
//! and the detector emits whatever its "no event" variant is. Results are
//! bit-identical to the old catalog.
//!
//! # Output types
//!
//! - `cross*` → [`ThresholdCrossEventMode`] (the detector's own "no event"
//!   variant is `None`, so there is no `Option` wrapper).
//! - `glitch_filter` → [`GlitchResult`] (the legacy `eval_glitch` mapped the
//!   `Option<bool>` to a `GlitchResult`, with `None` → `NoTransition`).
//! - `runt_detect` / `pulse_width` / `window_detect` → `Option<…>` — the
//!   legacy arms emitted `NodeOutput::other(Option<RuntResult>)` etc., so the
//!   absent / "no completed pulse" case is a `None`.
//!
//! Every method is exposed on `Comp<R, f64>` through the [`CrossOps`] and
//! [`DetectorOps`] extension traits, mirroring
//! [`WindowOps`](crate::ops::windows::WindowOps).

use crate::checkpoint;
use crate::events::{
    GlitchResult, PulseWidthResult, RuntResult, ThresholdCrossEventMode, WindowEvent,
};
use serde::{Deserialize, Serialize};
use tflo_core::comp::Comp;
use tflo_core::compile::{Computed, NodeOutput};
use tflo_core::operator::{BoxedOperator, Operator, OperatorLoadError, require};
use tflo_core::primitives::{
    CrossDetector, GlitchFilter, HysteresisCrossDetector, PulseWidthDetector, RuntDetector,
    WindowDetector,
};

// ============================================================================
// Cross detection — 2-input operators
// ============================================================================

/// Which `CrossDetector` update method a [`CrossOp`] runs.
///
/// `cross` reports both directions; `cross_above` / `cross_under` filter the
/// `CrossDetector::update` result down to a single direction — exactly the
/// `update` / `update_above` / `update_below` split of the legacy catalog's
/// [`eval_cross`] dispatch.
#[derive(Serialize, Deserialize, Clone, Copy)]
enum CrossMode {
    /// Report a cross in either direction.
    Both,
    /// Report only crosses above the threshold.
    Above,
    /// Report only crosses below the threshold.
    Under,
}

/// Threshold-crossing detector — `cross` / `cross_above` / `cross_under`.
///
/// A 2-input operator: `inputs[0]` is the value, `inputs[1]` is the threshold.
/// An absent input is substituted with `f64::NAN` before the detector update,
/// matching the legacy `eval_cross` helper, and the detector still emits a
/// `ThresholdCrossEventMode` (its own `None` is the "no event" variant).
#[derive(Serialize, Deserialize)]
struct CrossOp {
    mode: CrossMode,
    detector: CrossDetector,
}

impl CrossOp {
    fn new(mode: CrossMode) -> Self {
        Self {
            mode,
            detector: CrossDetector::new(),
        }
    }
}

impl Operator for CrossOp {
    fn eval(&mut self, inputs: &[Computed], _ts: i64) -> NodeOutput {
        // The legacy `eval_cross` substituted `f64::NAN` for an absent input
        // rather than propagating the `Absent` reason — replicate that here.
        let value = require(inputs, 0).unwrap_or(f64::NAN);
        let threshold = require(inputs, 1).unwrap_or(f64::NAN);
        let edge = match self.mode {
            CrossMode::Both => self.detector.update(value, threshold),
            CrossMode::Above => self.detector.update_above(value, threshold),
            CrossMode::Under => self.detector.update_below(value, threshold),
        };
        NodeOutput::other(to_event(edge))
    }

    fn name(&self) -> &str {
        match self.mode {
            CrossMode::Both => "cross",
            CrossMode::Above => "cross_above",
            CrossMode::Under => "cross_under",
        }
    }

    fn save(&self) -> Option<Vec<u8>> {
        checkpoint::save(self)
    }

    fn load(&mut self, bytes: &[u8]) -> Result<(), OperatorLoadError> {
        checkpoint::load(self, bytes)
    }
}

/// Hysteresis threshold-crossing detector — `cross_hysteresis`.
///
/// A 2-input operator: `inputs[0]` is the value, `inputs[1]` is the threshold.
/// An absent input is substituted with `f64::NAN`, matching the legacy
/// `NodeOp::CrossHysteresis` arm.
#[derive(Serialize, Deserialize)]
struct CrossHysteresisOp {
    detector: HysteresisCrossDetector,
}

impl Operator for CrossHysteresisOp {
    fn eval(&mut self, inputs: &[Computed], _ts: i64) -> NodeOutput {
        let value = require(inputs, 0).unwrap_or(f64::NAN);
        let threshold = require(inputs, 1).unwrap_or(f64::NAN);
        let edge = self.detector.update(value, threshold);
        NodeOutput::other(to_event(edge))
    }

    fn name(&self) -> &str {
        "cross_hysteresis"
    }

    fn save(&self) -> Option<Vec<u8>> {
        checkpoint::save(self)
    }

    fn load(&mut self, bytes: &[u8]) -> Result<(), OperatorLoadError> {
        checkpoint::load(self, bytes)
    }
}

/// Map the `tflo-core` primitive's `ThresholdCrossEventMode` to the `tflo-ops`
/// copy of that enum (see [`crate::events`]).
fn to_event(mode: tflo_core::primitives::ThresholdCrossEventMode) -> ThresholdCrossEventMode {
    use tflo_core::primitives::ThresholdCrossEventMode as Core;
    match mode {
        Core::Rising => ThresholdCrossEventMode::Rising,
        Core::Falling => ThresholdCrossEventMode::Falling,
        Core::None => ThresholdCrossEventMode::None,
    }
}

// ============================================================================
// Glitch / runt / pulse-width / window — 1-input operators
// ============================================================================

/// Glitch filter — `glitch_filter`.
///
/// Wraps [`GlitchFilter`]; an absent input is substituted with `f64::NAN`,
/// matching the legacy `eval_glitch`. The `Option<bool>` the primitive returns
/// is mapped to a [`GlitchResult`]: `Some(true)` → `ValidPulse`,
/// `Some(false)` → `Rejected`, `None` → `NoTransition`.
///
/// The [`DetectorOps::glitch_filter`] builder method is the normal entry point.
#[derive(Serialize, Deserialize)]
struct GlitchOp {
    detector: GlitchFilter,
}

impl GlitchOp {
    fn new(detector: GlitchFilter) -> Self {
        Self { detector }
    }
}

impl Operator for GlitchOp {
    fn eval(&mut self, inputs: &[Computed], ts: i64) -> NodeOutput {
        let value = require(inputs, 0).unwrap_or(f64::NAN);
        let result = match self.detector.update(value, ts) {
            Some(true) => GlitchResult::ValidPulse,
            Some(false) => GlitchResult::Rejected,
            None => GlitchResult::NoTransition,
        };
        NodeOutput::other(result)
    }

    fn name(&self) -> &str {
        "glitch_filter"
    }

    fn save(&self) -> Option<Vec<u8>> {
        checkpoint::save(self)
    }

    fn load(&mut self, bytes: &[u8]) -> Result<(), OperatorLoadError> {
        checkpoint::load(self, bytes)
    }
}

/// Runt detector — `runt_detect`.
///
/// Wraps [`RuntDetector`]; an absent input is substituted with `f64::NAN`,
/// matching the legacy `eval_runt`. The output is `Option<RuntResult>` — the
/// legacy arm emitted `NodeOutput::other(Option<RuntResult>)`, with `None`
/// meaning "no pulse completed this step".
#[derive(Serialize, Deserialize)]
struct RuntOp {
    detector: RuntDetector,
}

impl Operator for RuntOp {
    fn eval(&mut self, inputs: &[Computed], _ts: i64) -> NodeOutput {
        let value = require(inputs, 0).unwrap_or(f64::NAN);
        let result: Option<RuntResult> = self.detector.update(value).map(to_runt);
        NodeOutput::other(result)
    }

    fn name(&self) -> &str {
        "runt_detect"
    }

    fn save(&self) -> Option<Vec<u8>> {
        checkpoint::save(self)
    }

    fn load(&mut self, bytes: &[u8]) -> Result<(), OperatorLoadError> {
        checkpoint::load(self, bytes)
    }
}

/// Map the `tflo-core` primitive's `RuntResult` to the `tflo-ops` copy.
fn to_runt(result: tflo_core::primitives::RuntResult) -> RuntResult {
    use tflo_core::primitives::RuntResult as Core;
    match result {
        Core::Runt { peak } => RuntResult::Runt { peak },
        Core::ValidPulse { peak } => RuntResult::ValidPulse { peak },
    }
}

/// Pulse-width detector — `pulse_width`.
///
/// Wraps [`PulseWidthDetector`]; an absent input is substituted with
/// `f64::NAN`, matching the legacy `eval_pulse_width`. The output is
/// `Option<PulseWidthResult>` — `None` means "no pulse completed this step".
#[derive(Serialize, Deserialize)]
struct PulseWidthOp {
    detector: PulseWidthDetector,
}

impl Operator for PulseWidthOp {
    fn eval(&mut self, inputs: &[Computed], ts: i64) -> NodeOutput {
        let value = require(inputs, 0).unwrap_or(f64::NAN);
        let result: Option<PulseWidthResult> = self.detector.update(value, ts).map(to_pulse_width);
        NodeOutput::other(result)
    }

    fn name(&self) -> &str {
        "pulse_width"
    }

    fn save(&self) -> Option<Vec<u8>> {
        checkpoint::save(self)
    }

    fn load(&mut self, bytes: &[u8]) -> Result<(), OperatorLoadError> {
        checkpoint::load(self, bytes)
    }
}

/// Map the `tflo-core` primitive's `PulseWidthResult` to the `tflo-ops` copy.
fn to_pulse_width(result: tflo_core::primitives::PulseWidthResult) -> PulseWidthResult {
    use tflo_core::primitives::PulseWidthResult as Core;
    match result {
        Core::TooShort { width_ms } => PulseWidthResult::TooShort { width_ms },
        Core::Valid { width_ms } => PulseWidthResult::Valid { width_ms },
        Core::TooLong { width_ms } => PulseWidthResult::TooLong { width_ms },
    }
}

/// Window detector — `window_detect`.
///
/// Wraps [`WindowDetector`]; an absent input is substituted with `f64::NAN`,
/// matching the legacy `eval_window_detect`. The output is
/// `Option<WindowEvent>` — `None` means "no window transition this step".
#[derive(Serialize, Deserialize)]
struct WindowDetectOp {
    detector: WindowDetector,
}

impl Operator for WindowDetectOp {
    fn eval(&mut self, inputs: &[Computed], _ts: i64) -> NodeOutput {
        let value = require(inputs, 0).unwrap_or(f64::NAN);
        let result: Option<WindowEvent> = self.detector.update(value).map(to_window_event);
        NodeOutput::other(result)
    }

    fn name(&self) -> &str {
        "window_detect"
    }

    fn save(&self) -> Option<Vec<u8>> {
        checkpoint::save(self)
    }

    fn load(&mut self, bytes: &[u8]) -> Result<(), OperatorLoadError> {
        checkpoint::load(self, bytes)
    }
}

/// Map the `tflo-core` primitive's `WindowEvent` to the `tflo-ops` copy.
fn to_window_event(event: tflo_core::primitives::WindowEvent) -> WindowEvent {
    use tflo_core::primitives::WindowEvent as Core;
    match event {
        Core::EnteredWindow => WindowEvent::EnteredWindow,
        Core::ExitedLow => WindowEvent::ExitedLow,
        Core::ExitedHigh => WindowEvent::ExitedHigh,
    }
}

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
    fn cross(&self, other: &Comp<R, f64>) -> Comp<R, ThresholdCrossEventMode> {
        Comp::custom_node_dyn(self, &[other], || boxed(CrossOp::new(CrossMode::Both)))
    }

    fn cross_above(&self, other: &Comp<R, f64>) -> Comp<R, ThresholdCrossEventMode> {
        Comp::custom_node_dyn(self, &[other], || boxed(CrossOp::new(CrossMode::Above)))
    }

    fn cross_under(&self, other: &Comp<R, f64>) -> Comp<R, ThresholdCrossEventMode> {
        Comp::custom_node_dyn(self, &[other], || boxed(CrossOp::new(CrossMode::Under)))
    }

    fn cross_hysteresis(
        &self,
        threshold: &Comp<R, f64>,
        margin: f64,
    ) -> Comp<R, ThresholdCrossEventMode> {
        Comp::custom_node_dyn(self, &[threshold], move || {
            boxed(CrossHysteresisOp {
                detector: HysteresisCrossDetector::new(margin),
            })
        })
    }

    // The comparisons are plain stateless closures — ported from the legacy
    // `NodeOp::Gt`/`Gte`/`Lt`/`Lte` eval arms, which emit `1.0` for true and
    // `0.0` for false. `map2_f64` already short-circuits an absent input.
    fn gt(&self, other: &Comp<R, f64>) -> Comp<R, f64> {
        self.map2_f64(other, |x, y| if x > y { 1.0 } else { 0.0 })
    }

    fn gte(&self, other: &Comp<R, f64>) -> Comp<R, f64> {
        self.map2_f64(other, |x, y| if x >= y { 1.0 } else { 0.0 })
    }

    fn lt(&self, other: &Comp<R, f64>) -> Comp<R, f64> {
        self.map2_f64(other, |x, y| if x < y { 1.0 } else { 0.0 })
    }

    fn lte(&self, other: &Comp<R, f64>) -> Comp<R, f64> {
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
        Comp::custom_node1_dyn(self, move || {
            boxed(GlitchOp::new(GlitchFilter::new(threshold, min_duration_ms)))
        })
    }

    fn runt_detect(&self, low: f64, high: f64) -> Comp<R, Option<RuntResult>> {
        Comp::custom_node1_dyn(self, move || {
            boxed(RuntOp {
                detector: RuntDetector::new(low, high),
            })
        })
    }

    fn pulse_width(
        &self,
        threshold: f64,
        min_width_ms: i64,
        max_width_ms: i64,
    ) -> Comp<R, Option<PulseWidthResult>> {
        Comp::custom_node1_dyn(self, move || {
            boxed(PulseWidthOp {
                detector: PulseWidthDetector::new(threshold, min_width_ms, max_width_ms),
            })
        })
    }

    fn window_detect(&self, low: f64, high: f64) -> Comp<R, Option<WindowEvent>> {
        Comp::custom_node1_dyn(self, move || {
            boxed(WindowDetectOp {
                detector: WindowDetector::new(low, high),
            })
        })
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
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
