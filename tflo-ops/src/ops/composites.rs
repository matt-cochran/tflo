//! Composite operators and the [`Composites`] extension trait.
//!
//! These operators are pure graph-builders: each method composes existing
//! `Comp` operations (windowed aggregates, stateful trackers, arithmetic) to
//! produce higher-level signals. They add no new runtime nodes.
//!
//! Methods are ported verbatim from `tflo-core/src/comp/dual_use.rs`. The
//! bodies use plain method-call syntax; the extension traits [`WindowOps`] and
//! [`StatefulOps`] are imported below so those methods resolve to the
//! `tflo-ops` extension-trait impls.
//!
//! Every method is exposed on `Comp<R, f64>` through the single [`Composites`]
//! extension trait so call sites read naturally — e.g. `price.zscore(20)`.

use crate::ops::trackers::StatefulOps;
use crate::ops::windows::WindowOps;
use tflo_core::comp::Comp;
use tflo_core::compile::{Absent, Computed, NodeOutput};
use tflo_core::operator::{Operator, require};
use tflo_core::window::Window;

/// Binary operator node that computes `numerator / denominator` and emits a
/// typed [`Absent::DivideByZero`] when the denominator is zero.
///
/// The four composite expressions that previously used the closure-`Div`
/// operator (`zscore`, `peak_decline`, `rate_of_change`, `normalize_range`)
/// now route through this node so a zero denominator surfaces as
/// `Absent::DivideByZero` rather than being flattened to `WarmingUp` by the
/// downstream `finite_or_warming` mapping of an `inf`/`NaN` arithmetic result.
///
/// The semantics match the existing `PctChangeStep` tracker in
/// `tflo-ops/src/ops/trackers.rs`, which already returns the typed
/// `DivideByZero` reason for the same situation.
struct SafeDivOp;

impl Operator for SafeDivOp {
    fn eval(&mut self, inputs: &[Computed], _ts: i64) -> NodeOutput {
        let num = require(inputs, 0);
        let denom = require(inputs, 1);
        let result: Computed = match (num, denom) {
            (Err(a), _) | (_, Err(a)) => Err(a),
            (Ok(_), Ok(0.0)) => Err(Absent::DivideByZero),
            (Ok(n), Ok(d)) => Ok(n / d),
        };
        NodeOutput::computed(result)
    }

    fn name(&self) -> &str {
        "divide_safe"
    }
}

/// Build a `numerator / denominator` node that yields
/// [`Absent::DivideByZero`] when the denominator is zero, rather than letting
/// the result drift into `±inf`/`NaN` and be flattened to
/// [`Absent::WarmingUp`] by the downstream `finite_or_warming` mapping.
fn divide_safe<R: 'static>(num: &Comp<R, f64>, denom: &Comp<R, f64>) -> Comp<R, f64> {
    Comp::custom_node(num, &[denom], || SafeDivOp)
}

/// Composite signal-conditioning and analytics operations on `Comp`.
///
/// All methods are graph-builders — they compose existing `Comp` methods and
/// add no new runtime nodes. The single blanket impl below adds every method
/// to `Comp<R, f64>`.
pub trait Composites<R> {
    // ========================================================================
    // OUTLIER / TREND
    // ========================================================================

    /// Deviation band: returns `(middle, upper, lower)`.
    ///
    /// - `middle` = SMA over the window
    /// - `upper` = `middle + k * std`
    /// - `lower` = `middle - k * std`
    ///
    /// A point outside `[lower, upper]` is `k` standard deviations from the
    /// rolling mean — the textbook rolling outlier test. Accepts either a
    /// `Duration` (time-based) or `usize` (count-based) window.
    ///
    /// Known in finance as Bollinger Bands; `tflo-fintech` re-exports this as
    /// `bollinger_bands`.
    #[must_use]
    fn deviation_band(
        &self,
        window: impl Into<Window>,
        k: f64,
    ) -> (Comp<R, f64>, Comp<R, f64>, Comp<R, f64>);

    /// Z-Score: `(value - mean) / std`.
    ///
    /// Measures how many standard deviations the current value sits from the
    /// rolling mean. Accepts either a `Duration` (time-based) or `usize`
    /// (count-based) window. A zero rolling standard deviation (constant
    /// series) surfaces as [`Absent::DivideByZero`]; `O = f64` callers see
    /// that flattened to `NaN`.
    #[must_use]
    fn zscore(&self, window: impl Into<Window>) -> Comp<R, f64>;

    /// Peak decline: `(current - running_max) / running_max`.
    ///
    /// Measures the decline from the running peak; always `<= 0`. Useful for
    /// degradation and drop-off detection on any monotone-ish signal. A zero
    /// running peak surfaces as [`Absent::DivideByZero`]; `O = f64` callers
    /// see that flattened to `NaN`.
    ///
    /// Known in finance as drawdown; `tflo-fintech` re-exports this as
    /// `drawdown`.
    #[must_use]
    fn peak_decline(&self) -> Comp<R, f64>;

    /// Momentum: `current - value `period` records ago`.
    #[must_use]
    fn momentum(&self, period: usize) -> Comp<R, f64>;

    /// Rate of change: `((current / value `period` ago) - 1) * 100`.
    ///
    /// A zero lagged value surfaces as [`Absent::DivideByZero`]; `O = f64`
    /// callers see that flattened to `NaN`.
    ///
    /// `tflo-fintech` re-exports this as `roc_n`.
    #[must_use]
    fn rate_of_change(&self, period: usize) -> Comp<R, f64>;

    // ========================================================================
    // SIGNAL CONDITIONING
    // ========================================================================

    /// Remove DC offset (mean) from the signal over a window.
    ///
    /// Equivalent to AC coupling — centers the signal around zero by
    /// subtracting the rolling mean. `Output = signal - SMA(signal, window)`.
    ///
    /// Accepts either a `Duration` (time-based) or `usize` (count-based) window.
    #[must_use]
    fn dc_remove(&self, window: impl Into<Window>) -> Comp<R, f64>;

    /// Correct for a drifting baseline using a low percentile.
    ///
    /// Subtracts a rolling percentile (typically 5th–20th) to remove a
    /// slowly-drifting "floor" from the signal.
    /// `Output = signal - quantile(signal, window, percentile)`.
    ///
    /// # Arguments
    ///
    /// * `window` - Window for baseline estimation
    /// * `percentile` - Percentile used as the baseline (0.0–1.0, typically 0.05–0.2)
    #[must_use]
    fn baseline_correct(&self, window: impl Into<Window>, percentile: f64) -> Comp<R, f64>;

    /// Normalize the signal to the `[0, 1]` range based on observed min/max.
    ///
    /// `Output = (signal - min) / (max - min)`.
    ///
    /// Accepts either a `Duration` (time-based) or `usize` (count-based) window.
    /// A zero rolling range (`max == min`) surfaces as [`Absent::DivideByZero`];
    /// `O = f64` callers see that flattened to `NaN`.
    #[must_use]
    fn normalize_range(&self, window: impl Into<Window>) -> Comp<R, f64>;

    /// Apply linear calibration: `output = input * gain + offset`.
    ///
    /// The standard transformation for converting raw sensor/ADC readings to
    /// physical units.
    #[must_use]
    fn calibrate(&self, gain: f64, offset: f64) -> Comp<R, f64>;
}

// SAFETY: every `+ - *` in this `impl` block is a `Comp` operator-overload
// (graph-builder), not integer arithmetic. The arithmetic is wrapped in
// closures that fire per-sample on `f64` data at runtime; overflow there is
// `f64::INFINITY` / `NaN`, mapped to `Absent::WarmingUp` /
// `Absent::DivideByZero` by `finite_or_warming` and the explicit
// `SafeDivOp`. There is no integer arithmetic to overflow here.
#[allow(clippy::arithmetic_side_effects)]
impl<R: 'static> Composites<R> for Comp<R, f64> {
    fn deviation_band(&self, window: impl Into<Window>, k: f64) -> (Self, Self, Self) {
        let w: Window = window.into();
        let middle = self.sma(w);
        let std = self.std(w);
        let band_width = &std * k;
        let upper = &middle + &band_width;
        let lower = &middle - &band_width;
        (middle, upper, lower)
    }

    fn zscore(&self, window: impl Into<Window>) -> Self {
        let w: Window = window.into();
        let mean = self.sma(w);
        let std = self.std(w);
        let centered = self - &mean;
        divide_safe(&centered, &std)
    }

    fn peak_decline(&self) -> Self {
        let peak = self.cummax();
        let drop = self - &peak;
        divide_safe(&drop, &peak)
    }

    fn momentum(&self, period: usize) -> Self {
        let mut prev = self.clone();
        for _ in 0..period {
            prev = prev.prev();
        }
        self - &prev
    }

    fn rate_of_change(&self, period: usize) -> Self {
        let mut prev = self.clone();
        for _ in 0..period {
            prev = prev.prev();
        }
        let diff = self - &prev;
        divide_safe(&diff, &prev) * 100.0
    }

    fn dc_remove(&self, window: impl Into<Window>) -> Self {
        let mean = self.sma(window);
        self - &mean
    }

    fn baseline_correct(&self, window: impl Into<Window>, percentile: f64) -> Self {
        let baseline = self.quantile(window, percentile);
        self - &baseline
    }

    fn normalize_range(&self, window: impl Into<Window>) -> Self {
        let w: Window = window.into();
        let min = self.min(w);
        let max = self.max(w);
        let range = &max - &min;
        let centered = self - &min;
        divide_safe(&centered, &range)
    }

    fn calibrate(&self, gain: f64, offset: f64) -> Self {
        self * gain + offset
    }
}
