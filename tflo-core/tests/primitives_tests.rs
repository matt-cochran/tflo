use std::time::Duration;
use tflo_core::primitives::{
    BaselineCorrector, DcRemover, GainOffsetCalibrator, LagBuffer, LinearRegressor,
    RangeNormalizer, TimeDcRemover, WelfordWindow, ZScoreNormalizer,
};


/// When WelfordWindow receives values,
/// the window shall maintain running statistics using Welford's algorithm,
/// So that variance is computed numerically stably,
/// And the statistics will be accurate even for large datasets.
#[test]
fn test_welford_window_basic() {
    let mut window = WelfordWindow::new(Duration::from_secs(10));

    window.push(1000, 10.0);
    window.push(2000, 20.0);
    window.push(3000, 30.0);

    assert_eq!(window.count(), 3);
    assert!((window.mean() - 20.0).abs() < 0.001);
}

/// When values are pushed outside the time window,
/// WelfordWindow shall evict old values,
/// So that statistics reflect only recent data,
/// And the count will decrease as values expire.
#[test]
fn test_welford_window_eviction() {
    let mut window = WelfordWindow::new(Duration::from_secs(5));

    window.push(1000, 10.0);
    window.push(2000, 20.0);
    window.push(8000, 30.0); // 1000 and 2000 should be evicted (>5s ago from 8000)

    // Window is 5000ms. At ts=8000, cutoff is 3000. Both 1000 and 2000 are < 3000.
    assert_eq!(window.count(), 1);
}

/// When with_recompute_interval is used,
/// WelfordWindow shall periodically recompute statistics from scratch,
/// So that numerical drift is prevented,
/// And long-running windows remain accurate.
#[test]
fn test_welford_window_recompute() {
    let mut window = WelfordWindow::with_recompute_interval(Duration::from_secs(100), 5);

    // Push values to trigger recompute
    for i in 0..10 {
        window.push(i * 1000, i as f64 * 10.0);
    }

    // Statistics should still be accurate
    assert!(window.count() > 0);
}

/// When variance is called,
/// WelfordWindow shall return the population variance,
/// So that users can assess data spread,
/// And the variance will be numerically stable.
#[test]
fn test_welford_window_variance() {
    let mut window = WelfordWindow::new(Duration::from_secs(10));

    window.push(1000, 10.0);
    window.push(2000, 20.0);
    window.push(3000, 30.0);

    let variance = window.variance();
    // Variance of [10, 20, 30] = ((10-20)² + (20-20)² + (30-20)²) / 3 = 200/3 ≈ 66.67
    assert!(variance > 60.0 && variance < 70.0);
}

/// When std is called,
/// WelfordWindow shall return the standard deviation,
/// So that users have the common spread measure,
/// And std = sqrt(variance).
#[test]
fn test_welford_window_std() {
    let mut window = WelfordWindow::new(Duration::from_secs(10));

    window.push(1000, 10.0);
    window.push(2000, 20.0);
    window.push(3000, 30.0);

    let std = window.std();
    // std ≈ sqrt(66.67) ≈ 8.16
    assert!(std > 8.0 && std < 9.0);
}

/// When LagBuffer stores values,
/// the buffer shall maintain temporal ordering,
/// So that lagged values can be retrieved,
/// And delta calculations will be accurate.
#[test]
fn test_lag_buffer_comprehensive() {
    let mut buffer = LagBuffer::new(Duration::from_secs(5));

    // Push values
    let _ = buffer.push(1000, 100.0);
    let _ = buffer.push(2000, 110.0);
    let _ = buffer.push(3000, 120.0);
    let _ = buffer.push(6000, 150.0);

    // Test delta at a known point
    let delta = buffer.delta_at(6000, 150.0);
    assert!(delta.is_some());
}

/// When update is called on DcRemover with constant signal,
/// the DcRemover shall return values close to zero,
/// So that the DC offset is removed,
/// And the output is AC-coupled.
#[test]
fn test_dc_remover_constant_signal() {
    let mut remover = DcRemover::new(10);

    // Prime with constant values
    for _ in 0..10 {
        let _ = remover.update(100.0);
    }

    // DC estimate should be ~100
    assert!((remover.dc_estimate() - 100.0).abs() < 0.01);

    // AC component of a value at the DC level should be ~0
    let ac = remover.update(100.0);
    assert!(ac.abs() < 1.0);
}

/// When update is called on DcRemover with varying signal,
/// the DcRemover shall track the mean and remove it,
/// So that the signal is centered around zero,
/// And variations remain visible.
#[test]
fn test_dc_remover_varying_signal() {
    let mut remover = DcRemover::new(5);

    // Push values with mean = 100
    for v in [98.0, 100.0, 102.0, 100.0, 100.0] {
        let _ = remover.update(v);
    }

    assert!(remover.is_primed());
    assert!((remover.dc_estimate() - 100.0).abs() < 0.1);
}

/// When reset is called on DcRemover,
/// the DcRemover shall clear its state,
/// So that the DC estimate restarts,
/// And subsequent samples start fresh.
#[test]
fn test_dc_remover_reset() {
    let mut remover = DcRemover::new(5);

    for _ in 0..5 {
        let _ = remover.update(100.0);
    }

    assert_eq!(remover.sample_count(), 5);
    remover.reset();
    assert_eq!(remover.sample_count(), 0);
    assert!(!remover.is_primed());
}

/// When update is called on TimeDcRemover with timestamps,
/// the TimeDcRemover shall evict old samples based on time,
/// So that only recent samples contribute to DC estimate,
/// And time-based windowing is respected.
#[test]
fn test_time_dc_remover_eviction() {
    let mut remover = TimeDcRemover::new(std::time::Duration::from_millis(100));

    let _ = remover.update(100.0, 0);
    let _ = remover.update(100.0, 50);
    let _ = remover.update(200.0, 150); // First sample should be evicted

    // DC should be based on samples at 50 and 150 only: (100 + 200) / 2 = 150
    assert!((remover.dc_estimate() - 150.0).abs() < 1.0);
}

/// When reset is called on TimeDcRemover,
/// the TimeDcRemover shall clear its state,
/// So that subsequent observations start fresh.
#[test]
fn test_time_dc_remover_reset() {
    let mut remover = TimeDcRemover::new(std::time::Duration::from_secs(1));

    let _ = remover.update(100.0, 0);
    remover.reset();
    assert!((remover.dc_estimate() - 0.0).abs() < 0.01);
}

/// When update is called on BaselineCorrector,
/// the BaselineCorrector shall subtract the baseline percentile,
/// So that drifting floor is removed,
/// And signal peaks remain visible.
#[test]
fn test_baseline_corrector_basic() {
    let mut corrector = BaselineCorrector::new(20, 0.1);

    // Push mixed signal: mostly at 100, some peaks at 150
    for i in 0..20 {
        let value = if i % 5 == 0 { 150.0 } else { 100.0 };
        let _ = corrector.update(value);
    }

    // Baseline (10th percentile) should be ~100
    assert!((corrector.baseline() - 100.0).abs() < 1.0);
    assert!(corrector.is_primed());
}

/// When reset is called on BaselineCorrector,
/// the BaselineCorrector shall clear its state,
/// So that subsequent observations start fresh.
#[test]
fn test_baseline_corrector_reset() {
    let mut corrector = BaselineCorrector::new(10, 0.1);

    for _ in 0..10 {
        let _ = corrector.update(100.0);
    }

    assert!(corrector.is_primed());
    corrector.reset();
    assert!(!corrector.is_primed());
}

/// When update is called on RangeNormalizer,
/// the RangeNormalizer shall scale values to [0, 1],
/// So that min maps to 0 and max maps to 1,
/// And intermediate values are proportionally scaled.
#[test]
fn test_range_normalizer_basic() {
    let mut normalizer = RangeNormalizer::new(10);

    // Push values 0 to 90
    for i in 0..10 {
        let _ = normalizer.update(i as f64 * 10.0);
    }

    let (min, max) = normalizer.range();
    assert!((min - 0.0).abs() < 0.01);
    assert!((max - 90.0).abs() < 0.01);

    // Midpoint should normalize to 0.5
    let normalized = normalizer.update(45.0);
    assert!((normalized - 0.5).abs() < 0.1);
}

/// When update is called on RangeNormalizer with constant signal,
/// the RangeNormalizer shall return 0.5,
/// So that zero-range signals don't cause division by zero.
#[test]
fn test_range_normalizer_constant() {
    let mut normalizer = RangeNormalizer::new(5);

    for _ in 0..5 {
        let _ = normalizer.update(100.0);
    }

    // All same value - should return 0.5
    let normalized = normalizer.update(100.0);
    assert!((normalized - 0.5).abs() < 0.01);
}

/// When reset is called on RangeNormalizer,
/// the RangeNormalizer shall clear its state,
/// So that min/max restart.
#[test]
fn test_range_normalizer_reset() {
    let mut normalizer = RangeNormalizer::new(5);

    for i in 0..5 {
        let _ = normalizer.update(i as f64 * 10.0);
    }

    normalizer.reset();
    assert!(!normalizer.is_primed());

    let (min, max) = normalizer.range();
    assert!(min.is_infinite());
    assert!(max.is_infinite());
}

/// When update is called on ZScoreNormalizer,
/// the ZScoreNormalizer shall return z-scores,
/// So that values are expressed as standard deviations from mean.
#[test]
fn test_zscore_normalizer_basic() {
    let mut normalizer = ZScoreNormalizer::new(10);

    // Push values with known distribution
    for v in [
        95.0, 97.0, 99.0, 101.0, 103.0, 95.0, 97.0, 99.0, 101.0, 103.0,
    ] {
        let _ = normalizer.update(v);
    }

    // Mean should be ~99
    assert!((normalizer.mean() - 99.0).abs() < 0.1);

    // Std should be approximately 3 (for this distribution)
    assert!(normalizer.std() > 2.0 && normalizer.std() < 4.0);

    // Value at mean should have z ≈ 0
    let z = normalizer.update(99.0);
    assert!(z.abs() < 0.5);
}

/// When update is called on ZScoreNormalizer with outlier,
/// the ZScoreNormalizer shall return high z-score,
/// So that anomalies are detectable.
#[test]
fn test_zscore_normalizer_outlier() {
    let mut normalizer = ZScoreNormalizer::new(20);

    // Add normal values
    for i in 0..20 {
        let _ = normalizer.update(100.0 + (i % 5) as f64 - 2.0);
    }

    // Add an outlier
    let z = normalizer.update(150.0);
    assert!(z.abs() > 2.0); // Should be an outlier
}

/// When reset is called on ZScoreNormalizer,
/// the ZScoreNormalizer shall clear its state.
#[test]
fn test_zscore_normalizer_reset() {
    let mut normalizer = ZScoreNormalizer::new(10);

    for _ in 0..10 {
        let _ = normalizer.update(100.0);
    }

    normalizer.reset();
    assert!(!normalizer.is_primed());
    assert!((normalizer.mean() - 0.0).abs() < 0.01);
}

/// When update is called on LinearRegressor with perfect linear data,
/// the LinearRegressor shall compute exact slope and intercept,
/// So that calibration coefficients are accurate.
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

/// When predict is called on LinearRegressor,
/// the LinearRegressor shall use computed coefficients,
/// So that new x values can be calibrated.
#[test]
fn test_linear_regressor_predict() {
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

/// When reset is called on LinearRegressor,
/// the LinearRegressor shall clear its state.
#[test]
fn test_linear_regressor_reset() {
    let mut regressor = LinearRegressor::new(10);

    for i in 0..10 {
        regressor.update(i as f64, i as f64 * 2.0);
    }

    assert!(regressor.is_primed());
    regressor.reset();
    assert!(!regressor.is_primed());
    assert_eq!(regressor.sample_count(), 0);
}

/// When apply is called on GainOffsetCalibrator,
/// the GainOffsetCalibrator shall apply linear transformation,
/// So that raw values are converted to physical units.
#[test]
fn test_gain_offset_apply() {
    let cal = GainOffsetCalibrator::new(2.5, 10.0);

    assert!((cal.apply(0.0) - 10.0).abs() < 0.001);
    assert!((cal.apply(10.0) - 35.0).abs() < 0.001);
    assert!((cal.apply(100.0) - 260.0).abs() < 0.001);
}

/// When inverse is called on GainOffsetCalibrator,
/// the GainOffsetCalibrator shall reverse the transformation,
/// So that physical units can be converted back to raw.
#[test]
fn test_gain_offset_inverse() {
    let cal = GainOffsetCalibrator::new(2.5, 10.0);

    // Round trip
    for raw in [0.0, 50.0, 100.0] {
        let calibrated = cal.apply(raw);
        let back = cal.inverse(calibrated);
        assert!((back - raw).abs() < 0.001);
    }
}

/// When from_two_points is called on GainOffsetCalibrator,
/// the GainOffsetCalibrator shall compute gain/offset from calibration points.
#[test]
fn test_gain_offset_from_two_points() {
    // Point 1: raw=0, reference=100
    // Point 2: raw=100, reference=200
    // Expected: gain=1, offset=100
    let cal = GainOffsetCalibrator::from_two_points(0.0, 100.0, 100.0, 200.0);

    assert!((cal.gain() - 1.0).abs() < 0.001);
    assert!((cal.offset() - 100.0).abs() < 0.001);
}

/// When identity is called on GainOffsetCalibrator,
/// the GainOffsetCalibrator shall have gain=1, offset=0.
#[test]
fn test_gain_offset_identity() {
    let cal = GainOffsetCalibrator::identity();

    assert!((cal.apply(123.0) - 123.0).abs() < 0.001);
    assert!((cal.gain() - 1.0).abs() < 0.001);
    assert!((cal.offset() - 0.0).abs() < 0.001);
}
