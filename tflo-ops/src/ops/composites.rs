//! Composite operators and the [`Composites`] extension trait.
//!
//! These operators are pure graph-builders: each method composes existing
//! `Comp` operations (windowed aggregates, stateful trackers, arithmetic) to
//! produce higher-level signals. They add no new runtime nodes.
//!
//! Methods are ported verbatim from `tflo-core/src/comp/dual_use.rs`. The
//! bodies use plain method-call syntax so they resolve to the legacy
//! `tflo-core` inherent methods during Phase 2 and to the `tflo-ops`
//! extension-trait methods after Phase 3 removes the legacy catalog.
//!
//! Every method is exposed on `Comp<R, f64>` through the single [`Composites`]
//! extension trait so call sites read naturally — e.g. `price.zscore(20)`.

use tflo_core::comp::Comp;
use tflo_core::window::Window;

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
    /// (count-based) window.
    #[must_use]
    fn zscore(&self, window: impl Into<Window>) -> Comp<R, f64>;

    /// Peak decline: `(current - running_max) / running_max`.
    ///
    /// Measures the decline from the running peak; always `<= 0`. Useful for
    /// degradation and drop-off detection on any monotone-ish signal.
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
    #[must_use]
    fn normalize_range(&self, window: impl Into<Window>) -> Comp<R, f64>;

    /// Apply linear calibration: `output = input * gain + offset`.
    ///
    /// The standard transformation for converting raw sensor/ADC readings to
    /// physical units.
    #[must_use]
    fn calibrate(&self, gain: f64, offset: f64) -> Comp<R, f64>;
}

impl<R: 'static> Composites<R> for Comp<R, f64> {
    fn deviation_band(
        &self,
        window: impl Into<Window>,
        k: f64,
    ) -> (Comp<R, f64>, Comp<R, f64>, Comp<R, f64>) {
        let w: Window = window.into();
        let middle = self.sma(w);
        let std = self.std(w);
        let band_width = &std * k;
        let upper = &middle + &band_width;
        let lower = &middle - &band_width;
        (middle, upper, lower)
    }

    fn zscore(&self, window: impl Into<Window>) -> Comp<R, f64> {
        let w: Window = window.into();
        let mean = self.sma(w);
        let std = self.std(w);
        (self - &mean) / &std
    }

    fn peak_decline(&self) -> Comp<R, f64> {
        let peak = self.cummax();
        (self - &peak) / &peak
    }

    fn momentum(&self, period: usize) -> Comp<R, f64> {
        let mut prev = self.clone();
        for _ in 0..period {
            prev = prev.prev();
        }
        self - &prev
    }

    fn rate_of_change(&self, period: usize) -> Comp<R, f64> {
        let mut prev = self.clone();
        for _ in 0..period {
            prev = prev.prev();
        }
        ((self - &prev) / &prev) * 100.0
    }

    fn dc_remove(&self, window: impl Into<Window>) -> Comp<R, f64> {
        let mean = self.sma(window);
        self - &mean
    }

    fn baseline_correct(&self, window: impl Into<Window>, percentile: f64) -> Comp<R, f64> {
        let baseline = self.quantile(window, percentile);
        self - &baseline
    }

    fn normalize_range(&self, window: impl Into<Window>) -> Comp<R, f64> {
        let w: Window = window.into();
        let min = self.min(w);
        let max = self.max(w);
        let range = &max - &min;
        (self - &min) / &range
    }

    fn calibrate(&self, gain: f64, offset: f64) -> Comp<R, f64> {
        self * gain + offset
    }
}
