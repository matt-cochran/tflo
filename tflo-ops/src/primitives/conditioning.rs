//! Signal conditioning primitives for streaming data processing.
//!
//! This module provides primitives for real-time signal conditioning—transformations
//! that prepare raw signals for analysis by removing noise, drift, and scaling issues.
//!
//! # Overview
//!
//! Signal conditioning sits between raw data acquisition and analysis:
//!
//! ```text
//! ┌─────────────┐     ┌──────────────────┐     ┌─────────────┐
//! │   Raw ADC   │ ──▶ │  Conditioning    │ ──▶ │  Analysis   │
//! │   Signal    │     │  (this module)   │     │  (SMA, etc) │
//! └─────────────┘     └──────────────────┘     └─────────────┘
//!                            │
//!                     ┌──────┴──────┐
//!                     │ • DC removal │
//!                     │ • Baseline   │
//!                     │ • Normalize  │
//!                     │ • Regression │
//!                     └─────────────┘
//! ```
//!
//! # Primitives
//!
//! | Primitive | Purpose | Use Case |
//! |-----------|---------|----------|
//! | [`DcRemover`] | Remove DC offset (mean) | AC coupling, centering |
//! | [`BaselineCorrector`] | Remove drifting baseline | Sensor drift compensation |
//! | [`RangeNormalizer`] | Scale to [0,1] range | Auto-ranging, comparison |
//! | [`ZScoreNormalizer`] | Standardize to z-scores | Anomaly detection |
//! | [`LinearRegressor`] | Compute slope/intercept | Calibration, trend fitting |
//! | [`GainOffsetCalibrator`] | Apply computed calibration | Signal correction |
//!
//! # Visual Guide to Signal Conditioning
//!
//! ```text
//! DC REMOVAL (DcRemover)
//! ════════════════════════════════════════════════════════════════
//! Removes the DC component (mean) to center signal around zero.
//! Essential for analyzing AC signals on top of DC bias.
//!
//!                     BEFORE                    AFTER
//!            ╭─╮    ╭─╮    ╭─╮            ╭─╮    ╭─╮    ╭─╮
//!     ──────╭┴─┴────┴─┴────┴─┴───        ─┴─┴────┴─┴────┴─┴──
//!           │                      ──▶        │
//!     DC ───┼───────────────────         0 ───┼───────────────
//!           │                                 │
//!                                         ╰─╯    ╰─╯    ╰─╯
//!
//!     Signal with DC offset of 100      Signal centered at 0
//!
//!
//! BASELINE CORRECTION (BaselineCorrector)
//! ════════════════════════════════════════════════════════════════
//! Removes a drifting baseline using a low percentile (e.g., 10th).
//! Useful when the "floor" of the signal drifts over time.
//!
//!                     BEFORE                    AFTER
//!     ╭─╮                     ╭───╮    ╭─╮                ╭───╮
//!     │ │  ╭──╮        ╭─╮    │   │    │ │  ╭──╮   ╭─╮    │   │
//!     │ │  │  │ ╭─╮    │ │    │   │    │ │  │  │   │ │    │   │
//!   ──┴─┴──┴──┴─┴─┴────┴─┴────┴───┴  ──┴─┴──┴──┴───┴─┴────┴───┴──
//!        ╱                    ╲         │                    │
//!       ╱   drifting baseline  ╲    0 ──┼────────────────────┼──
//!      ╱                        ╲       │                    │
//!
//!     Signal with drifting floor      Signal with flat baseline
//!
//!
//! RANGE NORMALIZATION (RangeNormalizer)
//! ════════════════════════════════════════════════════════════════
//! Scales signal to [0,1] range based on observed min/max.
//! Enables comparison of signals with different amplitudes.
//!
//!                     BEFORE                    AFTER
//!
//!     200 ─┬─ ╭───╮                    1.0 ─┬─ ╭───╮
//!          │  │   │                         │  │   │
//!     150 ─┼──┼───┼───╭───╮            0.75─┼──┼───┼───╭───╮
//!          │  │   │   │   │                 │  │   │   │   │
//!     100 ─┼──┴───┴───┴───┴──    ──▶   0.5 ─┼──┴───┴───┴───┴──
//!          │                                │
//!      50 ─┼─                          0.25─┼─
//!          │                                │
//!       0 ─┴─                           0.0─┴─
//!
//!     Signal: 50-200 range             Signal: 0.0-1.0 range
//!
//!
//! Z-SCORE NORMALIZATION (ZScoreNormalizer)
//! ════════════════════════════════════════════════════════════════
//! Standardizes to mean=0, std=1. Values express "standard deviations
//! from mean" - useful for detecting outliers and anomalies.
//!
//!                     BEFORE                    AFTER
//!
//!     200 ─┬─ ╭─╮                      +3σ ─┬─ ╭─╮  ANOMALY!
//!          │  │ │                           │  │ │
//!     150 ─┼──┼─┼────╭──╮               +1σ─┼──┼─┼────╭──╮
//!          │  │ │    │  │                   │  │ │    │  │
//!     100 ─┼──┴─┴────┴──┴──    ──▶       0 ─┼──┴─┴────┴──┴──
//!          │                                │
//!      50 ─┼───────────────            -1σ ─┼───────────────
//!          │                                │
//!
//!     Signal in original units        Signal in std deviations
//!                                     (easy to spot outliers)
//!
//!
//! LINEAR REGRESSION (LinearRegressor)
//! ════════════════════════════════════════════════════════════════
//! Computes rolling slope and intercept from (x, y) pairs.
//! Use for calibration: fit raw ADC to known reference values.
//!
//!         Known Reference (y)
//!              ▲
//!          100 ┼           ×
//!              │         ×
//!           75 ┼       ×      ← slope = (y₂-y₁)/(x₂-x₁)
//!              │     ×          intercept = y - slope×x
//!           50 ┼   ×
//!              │ ×
//!           25 ┼×
//!              │
//!              └───┬───┬───┬───▶ Raw ADC (x)
//!                 10  20  30  40
//!
//!     Result: y = 2.5 × x + 0  (gain=2.5, offset=0)
//!
//!
//! GAIN/OFFSET CALIBRATION (GainOffsetCalibrator)
//! ════════════════════════════════════════════════════════════════
//! Applies linear transformation: corrected = raw × gain + offset
//!
//!                     BEFORE                    AFTER
//!
//!          Raw ADC Values              Calibrated Units
//!     400 ─┬─ ╭───╮               100 ─┬─ ╭───╮
//!          │  │   │                    │  │   │
//!     300 ─┼──┼───┼───╭───╮        75 ─┼──┼───┼───╭───╮
//!          │  │   │   │   │            │  │   │   │   │
//!     200 ─┼──┴───┴───┴───┴──  ──▶ 50 ─┼──┴───┴───┴───┴──
//!          │                           │
//!     100 ─┼─                      25 ─┼─
//!          │                           │
//!
//!     Raw: 100-400 counts        Calibrated: 25-100°C
//!                                (gain=0.25, offset=0)
//! ```

pub use super::calibration::{GainOffsetCalibrator, LinearRegressor};
use std::collections::VecDeque;

// ============================================================================
// DC REMOVAL
// ============================================================================

/// Removes DC offset from a signal using a running mean.
///
/// # What is DC Removal?
///
/// DC removal (also called AC coupling) subtracts the mean value from the signal,
/// centering it around zero. This is essential when:
///
/// - Analyzing AC components on top of a DC bias
/// - Comparing signals with different DC levels
/// - Preparing signals for frequency analysis (FFT)
/// - Removing sensor bias/offset
///
/// # How It Works
///
/// The DC remover maintains a rolling window of values and computes their mean.
/// Each output is: `output = input - rolling_mean`
///
/// ```text
/// Input:  [102, 98, 105, 97, 103, 101, ...]  (mean ≈ 100)
/// Output: [  2, -2,   5, -3,   3,   1, ...]  (mean ≈ 0)
/// ```
///
/// # Choosing the Window Size
///
/// - **Larger window**: More stable DC estimate, slower response to true DC changes
/// - **Smaller window**: Faster response, but may remove low-frequency signal content
/// - **Rule of thumb**: Window should be 5-10x the period of the lowest frequency you want to keep
///
/// # Examples
///
/// ```rust
/// use tflo_ops::primitives::DcRemover;
///
/// // Remove DC using a 5-sample window
/// let mut remover = DcRemover::new(5);
///
/// // Signal with DC offset of ~100
/// assert!((remover.update(100.0) - 0.0).abs() < 0.01);  // First sample, DC unknown
/// assert!((remover.update(102.0)).abs() < 5.0);         // Close to 0 as DC estimate improves
/// assert!((remover.update(98.0)).abs() < 5.0);
/// assert!((remover.update(104.0)).abs() < 5.0);
/// assert!((remover.update(96.0)).abs() < 5.0);          // Window full, DC ≈ 100
///
/// // Check DC estimate
/// assert!((remover.dc_estimate() - 100.0).abs() < 2.0);
/// ```
#[derive(Debug, Clone)]
pub struct DcRemover {
    pub(crate) buffer: VecDeque<f64>,
    pub(crate) max_samples: usize,
    pub(crate) sum: f64,
}

// ============================================================================
// TIME-BASED DC REMOVAL
// ============================================================================

/// Removes DC offset using a time-based window.
///
/// Similar to [`DcRemover`] but uses timestamps to define the window,
/// which is more appropriate for irregularly-sampled data.
///
/// # Examples
///
/// ```rust
/// use tflo_ops::primitives::TimeDcRemover;
/// use std::time::Duration;
///
/// // Remove DC using a 1-second window
/// let mut remover = TimeDcRemover::new(Duration::from_secs(1));
///
/// // Add samples with timestamps (in milliseconds)
/// let _ = remover.update(100.0, 0);
/// let _ = remover.update(102.0, 200);
/// let _ = remover.update(98.0, 400);
/// let _ = remover.update(104.0, 600);
/// let ac = remover.update(96.0, 800);
///
/// // AC component should be close to 0 (96 - 100 = -4)
/// assert!((ac - (-4.0)).abs() < 1.0);
/// ```
#[derive(Debug, Clone)]
pub struct TimeDcRemover {
    pub(crate) buffer: VecDeque<(i64, f64)>,
    pub(crate) window_ms: i64,
    pub(crate) sum: f64,
}

// ============================================================================
// BASELINE CORRECTION
// ============================================================================

/// Corrects for drifting baseline using a low percentile.
///
/// # What is Baseline Correction?
///
/// Baseline correction removes a slowly-drifting "floor" from a signal by
/// subtracting a low percentile (typically 5th-20th) computed over a window.
/// This is different from DC removal, which uses the mean.
///
/// # When to Use
///
/// - **Sensor drift**: Temperature, humidity, or aging causes slow baseline drift
/// - **Spectroscopy**: Remove background fluorescence/absorption
/// - **ECG/EEG**: Remove wandering baseline from physiological signals
/// - **RF signals**: Remove noise floor drift
///
/// # How It Works
///
/// 1. Maintain a rolling window of samples
/// 2. Compute the Nth percentile (e.g., 10th) as the "baseline"
/// 3. Output = Input - Baseline
///
/// Using a low percentile (rather than minimum) provides robustness against
/// noise spikes that would otherwise corrupt the baseline estimate.
///
/// # Examples
///
/// ```rust
/// use tflo_ops::primitives::BaselineCorrector;
///
/// // Correct using 10th percentile over 20 samples
/// let mut corrector = BaselineCorrector::new(20, 0.1);
///
/// // Simulate signal: mostly at baseline (100), with occasional spikes (+50)
/// // The baseline corrector identifies the "floor" as the baseline
/// for i in 0..20 {
///     // 80% of values at baseline, 20% are spikes
///     let value = if i % 5 == 0 { 150.0 } else { 100.0 };
///     let _ = corrector.update(value);
/// }
///
/// // Baseline (10th percentile) should be ~100 (the floor)
/// assert!((corrector.baseline() - 100.0).abs() < 1.0);
///
/// // A value at the baseline corrects to ~0
/// let at_baseline = corrector.update(100.0);
/// assert!(at_baseline.abs() < 5.0);
///
/// // A spike corrects to ~50 (150 - 100)
/// let spike = corrector.update(150.0);
/// assert!((spike - 50.0).abs() < 5.0);
/// ```
#[derive(Debug, Clone)]
pub struct BaselineCorrector {
    pub(crate) buffer: VecDeque<f64>,
    pub(crate) sorted: Vec<f64>,
    pub(crate) max_samples: usize,
    pub(crate) percentile: f64,
}

// ============================================================================
// RANGE NORMALIZATION
// ============================================================================

/// Normalizes signal to [0, 1] range based on observed min/max.
///
/// # What is Range Normalization?
///
/// Range normalization (min-max scaling) transforms values to a standard [0, 1] range:
///
/// ```text
/// normalized = (value - min) / (max - min)
/// ```
///
/// This enables:
/// - Comparison of signals with different scales
/// - Input normalization for ML models
/// - Visualization with consistent axes
/// - Percentage-of-range calculations
///
/// # Adaptive Behavior
///
/// The normalizer tracks the running min/max over a window, automatically
/// adapting to the signal's actual range. This provides "auto-ranging"
/// similar to oscilloscope auto-scale.
///
/// # Examples
///
/// ```rust
/// use tflo_ops::primitives::RangeNormalizer;
///
/// // Normalize using a 50-sample window
/// let mut normalizer = RangeNormalizer::new(50);
///
/// // Signal varying from 100 to 200
/// for i in 0..50 {
///     let value = 100.0 + (i as f64 * 2.0); // 100, 102, 104, ..., 198
///     let _ = normalizer.update(value);
/// }
///
/// // After warmup:
/// // - Value 100 should map to ~0.0
/// // - Value 200 should map to ~1.0
/// // - Value 150 should map to ~0.5
/// assert!((normalizer.update(100.0) - 0.0).abs() < 0.1);
/// assert!((normalizer.update(200.0) - 1.0).abs() < 0.1);
/// assert!((normalizer.update(150.0) - 0.5).abs() < 0.1);
/// ```
#[derive(Debug, Clone)]
pub struct RangeNormalizer {
    pub(crate) buffer: VecDeque<f64>,
    pub(crate) max_samples: usize,
    pub(crate) current_min: f64,
    pub(crate) current_max: f64,
}

// ============================================================================
// Z-SCORE NORMALIZATION
// ============================================================================

/// Normalizes signal to z-scores (standard deviations from mean).
///
/// # What is Z-Score Normalization?
///
/// Z-score normalization (standardization) transforms values to express
/// how many standard deviations they are from the mean:
///
/// ```text
/// z = (value - mean) / std_dev
/// ```
///
/// The resulting distribution has mean=0 and std=1, making it easy to:
/// - Detect outliers (|z| > 2 or 3)
/// - Compare signals with different scales and variances
/// - Apply statistical thresholds consistently
///
/// # Statistical Interpretation
///
/// For normally distributed data:
/// - ~68% of values have |z| < 1
/// - ~95% of values have |z| < 2
/// - ~99.7% of values have |z| < 3
///
/// Values with |z| > 3 are strong candidates for anomalies.
///
/// # Examples
///
/// ```rust
/// use tflo_ops::primitives::ZScoreNormalizer;
///
/// // Normalize using a 100-sample window
/// let mut normalizer = ZScoreNormalizer::new(100);
///
/// // Add samples from a distribution with mean ~100, std ~10
/// for i in 0..100 {
///     let value = 100.0 + ((i % 20) as f64 - 10.0);  // Varies from 90 to 110
///     let _ = normalizer.update(value);
/// }
///
/// // Value at the mean should have z ≈ 0
/// let z_at_mean = normalizer.update(100.0);
/// assert!(z_at_mean.abs() < 0.5);
///
/// // Value 2 std devs above mean should have z ≈ 2
/// // (std is roughly 6 for this data, so 112 is about 2 std devs up)
/// ```
#[derive(Debug, Clone)]
pub struct ZScoreNormalizer {
    pub(crate) buffer: VecDeque<f64>,
    pub(crate) max_samples: usize,
    pub(crate) sum: f64,
    pub(crate) sum_sq: f64,
}

// ============================================================================
// LINEAR REGRESSION
// ============================================================================

// ============================================================================
// GAIN/OFFSET CALIBRATOR
// ============================================================================

// ============================================================================
// TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // --- DcRemover Tests ---

    #[test]
    fn test_dc_remover_basic() {
        let mut remover = DcRemover::new(5);

        // Add samples with DC offset of 100
        for _ in 0..5 {
            let _ = remover.update(100.0);
        }

        assert!(remover.is_primed());
        assert!((remover.dc_estimate() - 100.0).abs() < 0.001);

        // Next sample should have DC removed
        let ac = remover.update(110.0);
        // AC component should be close to 10 (110 - 100)
        // But DC estimate shifts slightly with new sample
        assert!(ac.abs() < 15.0);
    }

    #[test]
    fn test_dc_remover_varying_signal() {
        let mut remover = DcRemover::new(4);

        let values = [98.0, 102.0, 96.0, 104.0]; // Mean = 100
        for v in values {
            let _ = remover.update(v);
        }

        assert!((remover.dc_estimate() - 100.0).abs() < 0.001);
    }

    #[test]
    fn test_dc_remover_reset() {
        let mut remover = DcRemover::new(5);
        let _ = remover.update(100.0);
        remover.reset();
        assert_eq!(remover.sample_count(), 0);
        assert!(!remover.is_primed());
    }

    // --- TimeDcRemover Tests ---

    #[test]
    fn test_time_dc_remover() {
        let mut remover = TimeDcRemover::new(std::time::Duration::from_secs(1));

        let _ = remover.update(100.0, 0);
        let _ = remover.update(100.0, 500);
        let ac = remover.update(110.0, 800);

        // DC estimate should be ~100, AC should be ~10
        assert!((remover.dc_estimate() - 103.33).abs() < 1.0);
        assert!(ac.abs() < 15.0);
    }

    #[test]
    fn test_time_dc_remover_eviction() {
        let mut remover = TimeDcRemover::new(std::time::Duration::from_millis(100));

        let _ = remover.update(100.0, 0);
        let _ = remover.update(100.0, 50);
        let _ = remover.update(200.0, 150); // First sample should be evicted

        // DC estimate should now be based on samples at 50 and 150
        assert!((remover.dc_estimate() - 150.0).abs() < 1.0);
    }

    // --- BaselineCorrector Tests ---

    #[test]
    fn test_baseline_corrector_basic() {
        let mut corrector = BaselineCorrector::new(10, 0.1);

        // Add samples: 5 at baseline (100), 5 with signal (150)
        for _ in 0..5 {
            let _ = corrector.update(100.0);
        }
        for _ in 0..5 {
            let _ = corrector.update(150.0);
        }

        // Baseline (10th percentile) should be ~100
        assert!((corrector.baseline() - 100.0).abs() < 1.0);
    }

    #[test]
    fn test_baseline_corrector_correction() {
        let mut corrector = BaselineCorrector::new(20, 0.1);

        // Prime with values from 100-200
        for i in 0..20 {
            let _ = corrector.update(100.0 + i as f64 * 5.0);
        }

        // Baseline should be around 10th percentile
        // For 100, 105, 110, ..., 195: 10th percentile ≈ 110
        let baseline = corrector.baseline();
        assert!((100.0..=120.0).contains(&baseline));
    }

    // --- RangeNormalizer Tests ---

    #[test]
    fn test_range_normalizer_basic() {
        let mut normalizer = RangeNormalizer::new(10);

        // Add values 0-90
        for i in 0..10 {
            let _ = normalizer.update(i as f64 * 10.0);
        }

        // Check range
        let (min, max) = normalizer.range();
        assert!((min - 0.0).abs() < 0.001);
        assert!((max - 90.0).abs() < 0.001);

        // Test normalization
        assert!((normalizer.update(0.0) - 0.0).abs() < 0.1);
        assert!((normalizer.update(90.0) - 1.0).abs() < 0.1);
        assert!((normalizer.update(45.0) - 0.5).abs() < 0.1);
    }

    #[test]
    fn test_range_normalizer_constant_signal() {
        let mut normalizer = RangeNormalizer::new(5);

        for _ in 0..5 {
            let _ = normalizer.update(100.0);
        }

        // All values the same - should return 0.5
        assert!((normalizer.update(100.0) - 0.5).abs() < 0.001);
    }

    // --- ZScoreNormalizer Tests ---

    #[test]
    fn test_zscore_normalizer_basic() {
        let mut normalizer = ZScoreNormalizer::new(10);

        // Add 10 samples: 95, 97, 99, 101, 103, 95, 97, 99, 101, 103
        let values = [
            95.0, 97.0, 99.0, 101.0, 103.0, 95.0, 97.0, 99.0, 101.0, 103.0,
        ];
        for v in values {
            let _ = normalizer.update(v);
        }

        // Mean should be ~99, std should be ~3
        assert!((normalizer.mean() - 99.0).abs() < 0.1);
        assert!(normalizer.std() > 2.0 && normalizer.std() < 4.0);

        // Value at mean should have z ≈ 0
        let z = normalizer.update(99.0);
        assert!(z.abs() < 0.5);
    }

    #[test]
    fn test_zscore_normalizer_outlier() {
        let mut normalizer = ZScoreNormalizer::new(20);

        // Add normal values
        for i in 0..20 {
            let _ = normalizer.update(100.0 + (i % 5) as f64 - 2.0);
        }

        // Add an outlier
        let z = normalizer.update(150.0);

        // Should have high z-score (anomaly)
        assert!(z.abs() > 2.0);
    }

    // --- LinearRegressor Tests ---

    #[test]
    fn test_linear_regressor_perfect_line() {
        let mut regressor = LinearRegressor::new(10);

        // y = 2x + 5
        for i in 0..10 {
            let x = i as f64;
            let y = 2.0 * x + 5.0;
            regressor.update(x, y);
        }

        let (slope, intercept) = regressor.coefficients();
        assert!((slope - 2.0).abs() < 0.001);
        assert!((intercept - 5.0).abs() < 0.001);
        assert!((regressor.r_squared() - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_linear_regressor_prediction() {
        let mut regressor = LinearRegressor::new(10);

        // y = 3x - 10
        for i in 0..10 {
            let x = i as f64 * 10.0;
            let y = 3.0 * x - 10.0;
            regressor.update(x, y);
        }

        // Predict for x = 50
        let pred = regressor.predict(50.0);
        assert!((pred - 140.0).abs() < 0.1); // 3*50 - 10 = 140
    }

    #[test]
    fn test_linear_regressor_noisy_data() {
        let mut regressor = LinearRegressor::new(20);

        // y = x + noise
        for i in 0..20 {
            let x = i as f64;
            let noise = if i % 2 == 0 { 1.0 } else { -1.0 };
            let y = x + noise;
            regressor.update(x, y);
        }

        let (slope, _) = regressor.coefficients();
        // Slope should still be close to 1
        assert!((slope - 1.0).abs() < 0.2);

        // R² should be high but not perfect
        let r2 = regressor.r_squared();
        assert!(r2 > 0.9 && r2 < 1.0);
    }

    // --- GainOffsetCalibrator Tests ---

    #[test]
    fn test_gain_offset_apply() {
        let cal = GainOffsetCalibrator::new(2.5, 10.0);

        assert!((cal.apply(0.0) - 10.0).abs() < 0.001);
        assert!((cal.apply(10.0) - 35.0).abs() < 0.001);
        assert!((cal.apply(100.0) - 260.0).abs() < 0.001);
    }

    #[test]
    fn test_gain_offset_inverse() {
        let cal = GainOffsetCalibrator::new(2.5, 10.0);

        // Round trip: apply then inverse
        for raw in [0.0, 50.0, 100.0] {
            let calibrated = cal.apply(raw);
            let back = cal.inverse(calibrated);
            assert!((back - raw).abs() < 0.001);
        }
    }

    #[test]
    fn test_gain_offset_from_two_points() {
        // Point 1: raw=0, reference=100
        // Point 2: raw=100, reference=200
        // Expected: gain=1, offset=100
        let cal = GainOffsetCalibrator::from_two_points(0.0, 100.0, 100.0, 200.0);

        assert!((cal.gain() - 1.0).abs() < 0.001);
        assert!((cal.offset() - 100.0).abs() < 0.001);
    }

    #[test]
    fn test_gain_offset_identity() {
        let cal = GainOffsetCalibrator::identity();

        assert!((cal.apply(123.0) - 123.0).abs() < 0.001);
    }

    #[test]
    fn test_gain_offset_default() {
        let cal = GainOffsetCalibrator::default();

        // Default should be identity
        assert!((cal.gain() - 1.0).abs() < 0.001);
        assert!((cal.offset() - 0.0).abs() < 0.001);
    }
}
