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

pub use crate::events::{
    GlitchResult, PulseWidthResult, RuntResult, ThresholdCrossEventMode, WindowEvent,
};
pub use crate::ops::composites::Composites;
pub use crate::ops::detectors::{CrossOps, DetectorOps};
pub use crate::ops::math::MathOps;
pub use crate::ops::trackers::StatefulOps;
pub use crate::ops::windows::WindowOps;
pub use crate::primitives::{
    BaselineCorrector, CrossDetector, DcRemover, GainOffsetCalibrator, GlitchFilter,
    HysteresisCrossDetector, LagBuffer, LinearRegressor, PrevByTracker, PrevTracker,
    PulseWidthDetector, RangeNormalizer, RuntDetector, TimeDcRemover, TimestampedPrevByTracker,
    TimestampedPrevTracker, WelfordAccumulator, WelfordWindow, WindowDetector, ZScoreNormalizer,
};
