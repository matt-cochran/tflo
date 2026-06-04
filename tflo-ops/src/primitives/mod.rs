//! Primitives — streaming windowing, TA indicators, and signal detection.
//!
//! Previously split across `tflo-stats`, `tflo-ta`, and `tflo-signals`
//! crates. Consolidated here to eliminate cross-crate friction.

// --- windowing (ex tflo-stats) ---
mod correlation;
mod count_window;
mod higher_moments;
mod median_window;
mod time_window;
mod welford;
mod wma;

// --- TA indicators (ex tflo-ta) ---
mod cumulative;
mod rsi;
mod time_ema;

// --- signal detection (ex tflo-signals) ---
mod calibration;
mod conditioning;
mod cross;
mod detectors;
mod event_mode;
mod glitch_pulse;
mod lag_buffer;
mod linear_calib;
mod prev;
mod prev_by;
mod results;
mod runt_window;

// --- re-exports ---

// windowing
pub use correlation::{CorrelationCountWindow, CorrelationTimeWindow};
pub use count_window::CountWindow;
pub use higher_moments::{MomentsCountWindow, MomentsTimeWindow};
pub use median_window::{MedianCountWindow, MedianTimeWindow};
pub use time_window::TimeWindow;
pub use welford::{WelfordAccumulator, WelfordWindow};
pub use wma::{WmaCountWindow, WmaTimeWindow};

// TA indicators
pub use cumulative::{
    CumulativeMax, CumulativeMean, CumulativeMin, CumulativeProduct, CumulativeSum,
};
pub use rsi::{RsiCountWindow, RsiTimeWindow};
pub use time_ema::{CountEma, TimeEma};

// signal detection
pub use calibration::{GainOffsetCalibrator, LinearRegressor};
pub use conditioning::{
    BaselineCorrector, DcRemover, RangeNormalizer, TimeDcRemover, ZScoreNormalizer,
};
pub use cross::CrossDetector;
pub use detectors::HysteresisCrossDetector;
pub use event_mode::ThresholdCrossEventMode;
pub use glitch_pulse::{GlitchFilter, PulseWidthDetector};
pub use lag_buffer::LagBuffer;
pub use prev::{PrevTracker, TimestampedPrevTracker};
pub use prev_by::{PrevByTracker, TimestampedPrevByTracker};
pub use results::{GlitchResult, PulseWidthResult, RuntResult, WindowEvent};
pub use runt_window::{RuntDetector, WindowDetector};
