use std::collections::VecDeque;

/// Computes rolling linear regression (slope and intercept).
///
/// # What is Rolling Linear Regression?
///
/// Linear regression fits a line `y = slope × x + intercept` to data points.
/// This rolling version continuously updates the fit as new (x, y) pairs arrive.
///
/// # Use Cases
///
/// - **Calibration**: Fit raw ADC readings to known reference values
/// - **Trend detection**: Compute the slope of a time series
/// - **Sensor fusion**: Correlate two measurement streams
/// - **Gain/offset estimation**: Determine correction factors
///
/// # How It Works
///
/// Uses the least-squares formulas:
///
/// ```text
/// slope = Σ(x - x̄)(y - ȳ) / Σ(x - x̄)²
/// intercept = ȳ - slope × x̄
/// ```
///
/// Maintained incrementally for O(1) per-sample updates.
///
/// # Examples
///
/// ```rust
/// use tflo_core::primitives::LinearRegressor;
///
/// // Fit a line through calibration points
/// let mut regressor = LinearRegressor::new(10);
///
/// // Known calibration: y = 2.5x + 10
/// for i in 0..10 {
///     let x = i as f64 * 10.0;              // ADC: 0, 10, 20, ..., 90
///     let y = 2.5 * x + 10.0;               // Reference: 10, 35, 60, ..., 235
///     regressor.update(x, y);
/// }
///
/// // Get computed calibration coefficients
/// let (slope, intercept) = regressor.coefficients();
/// assert!((slope - 2.5).abs() < 0.01);
/// assert!((intercept - 10.0).abs() < 0.1);
///
/// // Use to calibrate a new reading
/// let raw_adc = 50.0;
/// let calibrated = regressor.predict(raw_adc);
/// assert!((calibrated - 135.0).abs() < 0.1);  // 2.5 * 50 + 10 = 135
/// ```
#[derive(Debug, Clone)]
pub struct LinearRegressor {
    buffer: VecDeque<(f64, f64)>,
    max_samples: usize,
    sum_x: f64,
    sum_y: f64,
    sum_xy: f64,
    sum_xx: f64,
}

/// Applies linear calibration: `output = input × gain + offset`.
///
/// # What is Gain/Offset Calibration?
///
/// This is the standard linear transformation for converting raw sensor/ADC
/// readings to physical units:
///
/// ```text
/// physical_value = raw_reading × gain + offset
/// ```
///
/// Where:
/// - **gain** (slope): Conversion factor (units per count)
/// - **offset** (intercept): Zero-point correction
///
/// # Typical Workflow
///
/// 1. Collect calibration data (raw readings + known references)
/// 2. Use [`LinearRegressor`] to compute gain and offset
/// 3. Apply calibration using this struct
///
/// # Examples
///
/// ```rust
/// use tflo_core::primitives::GainOffsetCalibrator;
///
/// // Temperature sensor: 0.1°C per ADC count, offset of -40°C
/// let calibrator = GainOffsetCalibrator::new(0.1, -40.0);
///
/// // Raw ADC reading of 500 → 500 × 0.1 - 40 = 10°C
/// assert!((calibrator.apply(500.0) - 10.0).abs() < 0.001);
///
/// // Raw ADC reading of 800 → 800 × 0.1 - 40 = 40°C
/// assert!((calibrator.apply(800.0) - 40.0).abs() < 0.001);
/// ```
///
/// # Inverse Calibration
///
/// You can also convert from physical units back to raw:
///
/// ```rust
/// use tflo_core::primitives::GainOffsetCalibrator;
///
/// let calibrator = GainOffsetCalibrator::new(0.1, -40.0);
///
/// // What raw reading corresponds to 25°C?
/// let raw = calibrator.inverse(25.0);
/// assert!((raw - 650.0).abs() < 0.001);  // (25 + 40) / 0.1 = 650
/// ```
#[derive(Debug, Clone, Copy)]
pub struct GainOffsetCalibrator {
    gain: f64,
    offset: f64,
}

impl LinearRegressor {
    /// Create a new linear regressor.
    ///
    /// # Arguments
    ///
    /// * `window_samples` - Number of (x, y) pairs for regression
    ///
    /// `window_samples` is clamped to a minimum of 2 (a regression needs at
    /// least two points). Use [`try_new`](Self::try_new) to receive an error
    /// for an invalid window instead.
    #[must_use]
    pub fn new(window_samples: usize) -> Self {
        let window_samples = window_samples.max(2);
        Self {
            buffer: VecDeque::with_capacity(window_samples),
            max_samples: window_samples,
            sum_x: 0.0,
            sum_y: 0.0,
            sum_xy: 0.0,
            sum_xx: 0.0,
        }
    }

    /// Create a new linear regressor, validating the window size.
    ///
    /// # Errors
    ///
    /// Returns [`TFloError::Configuration`](crate::error::TFloError::Configuration)
    /// if `window_samples` is less than 2.
    pub fn try_new(window_samples: usize) -> Result<Self, crate::error::TFloError> {
        if window_samples < 2 {
            return Err(crate::error::TFloError::Configuration {
                message: "LinearRegressor window_samples must be > 1 for regression".to_string(),
            });
        }
        Ok(Self::new(window_samples))
    }

    /// Add a new (x, y) data point and update the regression.
    pub fn update(&mut self, x: f64, y: f64) {
        // Add new point
        self.buffer.push_back((x, y));
        self.sum_x += x;
        self.sum_y += y;
        self.sum_xy += x * y;
        self.sum_xx += x * x;

        // Remove old point if window is full
        if self.buffer.len() > self.max_samples {
            if let Some((old_x, old_y)) = self.buffer.pop_front() {
                self.sum_x -= old_x;
                self.sum_y -= old_y;
                self.sum_xy -= old_x * old_y;
                self.sum_xx -= old_x * old_x;
            }
        }
    }

    /// Get the current regression coefficients (slope, intercept).
    ///
    /// Returns (0.0, mean_y) if there's insufficient variation in x.
    #[must_use]
    pub fn coefficients(&self) -> (f64, f64) {
        let n = self.buffer.len() as f64;
        if n < 2.0 {
            return (0.0, 0.0);
        }

        let mean_x = self.sum_x / n;
        let mean_y = self.sum_y / n;

        // Compute covariance and variance
        let cov_xy = (self.sum_xy / n) - (mean_x * mean_y);
        let var_x = (self.sum_xx / n) - (mean_x * mean_x);

        if var_x.abs() < f64::EPSILON {
            // All x values are the same - can't fit a line
            (0.0, mean_y)
        } else {
            let slope = cov_xy / var_x;
            let intercept = mean_y - slope * mean_x;
            (slope, intercept)
        }
    }

    /// Predict y for a given x using the current regression.
    #[must_use]
    pub fn predict(&self, x: f64) -> f64 {
        let (slope, intercept) = self.coefficients();
        slope * x + intercept
    }

    /// Get the coefficient of determination (R²).
    ///
    /// R² indicates how well the line fits the data:
    /// - R² = 1.0: Perfect fit
    /// - R² = 0.0: No linear relationship
    /// - R² < 0.0: Worse than horizontal line (shouldn't happen with least squares)
    #[must_use]
    pub fn r_squared(&self) -> f64 {
        let n = self.buffer.len() as f64;
        if n < 2.0 {
            return 0.0;
        }

        let mean_y = self.sum_y / n;
        let (slope, intercept) = self.coefficients();

        let mut ss_tot = 0.0;
        let mut ss_res = 0.0;

        for &(x, y) in &self.buffer {
            let y_pred = slope * x + intercept;
            ss_tot += (y - mean_y).powi(2);
            ss_res += (y - y_pred).powi(2);
        }

        if ss_tot.abs() < f64::EPSILON {
            1.0 // All y values are the same - "perfect" fit
        } else {
            1.0 - (ss_res / ss_tot)
        }
    }

    /// Get the number of data points in the window.
    #[must_use]
    pub fn sample_count(&self) -> usize {
        self.buffer.len()
    }

    /// Check if the window is fully populated.
    #[must_use]
    pub fn is_primed(&self) -> bool {
        self.buffer.len() >= self.max_samples
    }

    /// Reset the regressor state.
    pub fn reset(&mut self) {
        self.buffer.clear();
        self.sum_x = 0.0;
        self.sum_y = 0.0;
        self.sum_xy = 0.0;
        self.sum_xx = 0.0;
    }
}

impl GainOffsetCalibrator {
    /// Create a new calibrator with specified gain and offset.
    ///
    /// # Arguments
    ///
    /// * `gain` - Multiplicative factor (slope)
    /// * `offset` - Additive constant (intercept)
    #[must_use]
    pub const fn new(gain: f64, offset: f64) -> Self {
        Self { gain, offset }
    }

    /// Create a calibrator from two known points.
    ///
    /// Given two calibration points (raw1, ref1) and (raw2, ref2),
    /// computes the gain and offset.
    ///
    /// # Arguments
    ///
    /// * `raw1`, `ref1` - First calibration point
    /// * `raw2`, `ref2` - Second calibration point
    ///
    /// When `raw1` and `raw2` are equal the gain is undetermined; the result
    /// degrades to a constant calibration (`gain = 0`, `offset = ref1`). Use
    /// [`try_from_two_points`](Self::try_from_two_points) to receive an error
    /// for that case instead.
    #[must_use]
    pub fn from_two_points(raw1: f64, ref1: f64, raw2: f64, ref2: f64) -> Self {
        if (raw2 - raw1).abs() <= f64::EPSILON {
            // Coincident raw points — slope is undetermined.
            return Self {
                gain: 0.0,
                offset: ref1,
            };
        }
        let gain = (ref2 - ref1) / (raw2 - raw1);
        let offset = ref1 - gain * raw1;
        Self { gain, offset }
    }

    /// Build a calibration from two points, validating that they are distinct.
    ///
    /// # Errors
    ///
    /// Returns [`TFloError::Configuration`](crate::error::TFloError::Configuration)
    /// if `raw1` and `raw2` are equal (the gain cannot be determined).
    pub fn try_from_two_points(
        raw1: f64,
        ref1: f64,
        raw2: f64,
        ref2: f64,
    ) -> Result<Self, crate::error::TFloError> {
        if (raw2 - raw1).abs() <= f64::EPSILON {
            return Err(crate::error::TFloError::Configuration {
                message: "GainOffsetCalibrator raw1 and raw2 must be different".to_string(),
            });
        }
        Ok(Self::from_two_points(raw1, ref1, raw2, ref2))
    }

    /// Apply the calibration: output = input × gain + offset.
    #[must_use]
    #[inline]
    pub fn apply(&self, raw: f64) -> f64 {
        raw * self.gain + self.offset
    }

    /// Apply inverse calibration: raw = (output - offset) / gain.
    ///
    /// Returns NaN if gain is zero.
    #[must_use]
    #[inline]
    pub fn inverse(&self, calibrated: f64) -> f64 {
        (calibrated - self.offset) / self.gain
    }

    /// Get the gain (slope).
    #[must_use]
    pub const fn gain(&self) -> f64 {
        self.gain
    }

    /// Get the offset (intercept).
    #[must_use]
    pub const fn offset(&self) -> f64 {
        self.offset
    }

    /// Create an identity calibrator (gain=1, offset=0).
    #[must_use]
    pub const fn identity() -> Self {
        Self {
            gain: 1.0,
            offset: 0.0,
        }
    }
}

impl Default for GainOffsetCalibrator {
    fn default() -> Self {
        Self::identity()
    }
}
