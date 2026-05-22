//! Domain-neutral outlier, trend, and signal-conditioning operations.
//!
//! These operations are generically useful across any event domain — anomaly
//! and outlier detection, trend estimation, sensor conditioning — so they live
//! in `tflo-core` rather than in a domain-specific crate. The `tflo-fintech`
//! crate re-exports several of them under their traditional finance names
//! (e.g. `bollinger_bands` → [`deviation_band`](Comp::deviation_band)).
//!
//! All methods are composites: they build graphs from existing `Comp`
//! operations and add no new runtime nodes.

use super::Comp;
use crate::window::Window;

impl<R: 'static> Comp<R, f64> {
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
    pub fn deviation_band(&self, window: impl Into<Window>, k: f64) -> (Comp<R>, Comp<R>, Comp<R>) {
        let w: Window = window.into();
        let middle = self.sma(w);
        let std = self.std(w);
        let band_width = &std * k;
        let upper = &middle + &band_width;
        let lower = &middle - &band_width;
        (middle, upper, lower)
    }

    /// Z-Score: `(value - mean) / std`.
    ///
    /// Measures how many standard deviations the current value sits from the
    /// rolling mean. Accepts either a `Duration` (time-based) or `usize`
    /// (count-based) window.
    #[must_use]
    pub fn zscore(&self, window: impl Into<Window>) -> Comp<R> {
        let w: Window = window.into();
        let mean = self.sma(w);
        let std = self.std(w);
        (self - &mean) / &std
    }

    /// Peak decline: `(current - running_max) / running_max`.
    ///
    /// Measures the decline from the running peak; always `<= 0`. Useful for
    /// degradation and drop-off detection on any monotone-ish signal.
    ///
    /// Known in finance as drawdown; `tflo-fintech` re-exports this as
    /// `drawdown`.
    #[must_use]
    pub fn peak_decline(&self) -> Comp<R> {
        let peak = self.cummax();
        (self - &peak) / &peak
    }

    /// Momentum: `current - value `period` records ago`.
    #[must_use]
    pub fn momentum(&self, period: usize) -> Comp<R> {
        let mut prev = self.clone();
        for _ in 0..period {
            prev = prev.prev();
        }
        self - &prev
    }

    /// Rate of change: `((current / value `period` ago) - 1) * 100`.
    ///
    /// `tflo-fintech` re-exports this as `roc_n`.
    #[must_use]
    pub fn rate_of_change(&self, period: usize) -> Comp<R> {
        let mut prev = self.clone();
        for _ in 0..period {
            prev = prev.prev();
        }
        ((self - &prev) / &prev) * 100.0
    }

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
    pub fn dc_remove(&self, window: impl Into<Window>) -> Comp<R> {
        let mean = self.sma(window);
        self - &mean
    }

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
    pub fn baseline_correct(&self, window: impl Into<Window>, percentile: f64) -> Comp<R> {
        let baseline = self.quantile(window, percentile);
        self - &baseline
    }

    /// Normalize the signal to the `[0, 1]` range based on observed min/max.
    ///
    /// `Output = (signal - min) / (max - min)`.
    ///
    /// Accepts either a `Duration` (time-based) or `usize` (count-based) window.
    #[must_use]
    pub fn normalize_range(&self, window: impl Into<Window>) -> Comp<R> {
        let w: Window = window.into();
        let min = self.min(w);
        let max = self.max(w);
        let range = &max - &min;
        (self - &min) / &range
    }

    /// Apply linear calibration: `output = input * gain + offset`.
    ///
    /// The standard transformation for converting raw sensor/ADC readings to
    /// physical units.
    #[must_use]
    pub fn calibrate(&self, gain: f64, offset: f64) -> Comp<R> {
        self * gain + offset
    }
}
