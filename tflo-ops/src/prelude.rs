//! Convenience re-exports for `tflo-ops`.
//!
//! `use tflo_ops::prelude::*;` brings into scope:
//!
//! - **Extension traits** — every operator surface on `Comp<R, f64>`:
//!   [`WindowOps`], [`StatefulOps`], [`CrossOps`], [`DetectorOps`],
//!   [`MathOps`], [`Composites`].
//!
//! - **Event types** — the typed output values produced by the detector
//!   operators: [`ThresholdCrossEventMode`], [`GlitchResult`], [`RuntResult`],
//!   [`PulseWidthResult`], [`WindowEvent`].
//!
//! - **Detector primitives** that back a public `Comp` extension method
//!   (`CrossDetector`, `GlitchFilter`, `RuntDetector`, …) and `PrevTracker` /
//!   `LagBuffer` helpers used in plugin examples.
//!
//! Raw signal-conditioning building blocks (`DcRemover`, `BaselineCorrector`,
//! `RangeNormalizer`, `ZScoreNormalizer`, `WelfordAccumulator`,
//! `WelfordWindow`, `GainOffsetCalibrator`, `TimeDcRemover`) are *not* wired
//! as `Comp` operators — the equivalent functionality is exposed through the
//! [`Composites`] graph-builder methods (`dc_remove`, `baseline_correct`,
//! `normalize_range`, `zscore`, `calibrate`). The raw structs live in
//! [`primitives_prelude`] for the rare consumer that needs them outside the
//! graph.

pub use crate::events::{
    GlitchResult, PulseWidthResult, RuntResult, ThresholdCrossEventMode, WindowEvent,
};
pub use crate::ops::composites::Composites;
pub use crate::ops::detectors::{CrossOps, DetectorOps};
pub use crate::ops::math::MathOps;
pub use crate::ops::trackers::StatefulOps;
pub use crate::ops::windows::WindowOps;
pub use crate::primitives::{
    CrossDetector, GlitchFilter, HysteresisCrossDetector, LagBuffer, LinearRegressor,
    PrevByTracker, PrevTracker, PulseWidthDetector, RuntDetector, TimestampedPrevByTracker,
    TimestampedPrevTracker, WindowDetector,
};

/// Raw signal-conditioning primitives — opt-in sub-prelude.
///
/// These structs (`DcRemover`, `BaselineCorrector`, `RangeNormalizer`,
/// `ZScoreNormalizer`, `WelfordAccumulator`, `WelfordWindow`,
/// `GainOffsetCalibrator`, `TimeDcRemover`) are *not* wired as `Comp`
/// operators — the equivalent functionality is exposed through the
/// [`Composites`](crate::ops::composites::Composites) graph-builder methods.
/// They live here as raw building blocks for callers that need to drive a
/// streaming conditioner outside the `tflo` graph engine.
///
/// `use tflo_ops::prelude::primitives_prelude::*;` brings them into scope.
pub mod primitives_prelude {
    pub use crate::primitives::{
        BaselineCorrector, DcRemover, GainOffsetCalibrator, RangeNormalizer, TimeDcRemover,
        WelfordAccumulator, WelfordWindow, ZScoreNormalizer,
    };
}
