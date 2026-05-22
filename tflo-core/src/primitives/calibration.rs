use super::conditioning::{
    BaselineCorrector, DcRemover, RangeNormalizer, TimeDcRemover, ZScoreNormalizer,
};
pub use super::linear_calib::{GainOffsetCalibrator, LinearRegressor};
use std::collections::VecDeque;

impl DcRemover {
    /// Create a new DC remover with the specified window size.
    ///
    /// # Arguments
    ///
    /// * `window_samples` - Number of samples to use for DC estimation
    ///
    /// `window_samples` is clamped to a minimum of 1. Use
    /// [`try_new`](Self::try_new) to receive an error for an invalid window.
    #[must_use]
    pub fn new(window_samples: usize) -> Self {
        let window_samples = window_samples.max(1);
        Self {
            buffer: VecDeque::with_capacity(window_samples),
            max_samples: window_samples,
            sum: 0.0,
        }
    }

    /// Create a new DC remover, validating the window size.
    ///
    /// # Errors
    ///
    /// Returns [`TFloError::Configuration`](crate::error::TFloError::Configuration)
    /// if `window_samples` is 0.
    pub fn try_new(window_samples: usize) -> Result<Self, crate::error::TFloError> {
        if window_samples == 0 {
            return Err(crate::error::TFloError::Configuration {
                message: "DcRemover window_samples must be > 0".to_string(),
            });
        }
        Ok(Self::new(window_samples))
    }

    /// Process a new sample and return the DC-removed value.
    ///
    /// Returns `input - rolling_mean`.
    pub fn update(&mut self, value: f64) -> f64 {
        // Add new value
        self.buffer.push_back(value);
        self.sum += value;

        // Remove old value if window is full
        if self.buffer.len() > self.max_samples {
            if let Some(old) = self.buffer.pop_front() {
                self.sum -= old;
            }
        }

        // Compute and subtract DC
        let dc = self.sum / self.buffer.len() as f64;
        value - dc
    }

    /// Get the current DC estimate (rolling mean).
    #[must_use]
    pub fn dc_estimate(&self) -> f64 {
        if self.buffer.is_empty() {
            0.0
        } else {
            self.sum / self.buffer.len() as f64
        }
    }

    /// Check if the window is fully populated.
    #[must_use]
    pub fn is_primed(&self) -> bool {
        self.buffer.len() >= self.max_samples
    }

    /// Get the number of samples currently in the buffer.
    #[must_use]
    pub fn sample_count(&self) -> usize {
        self.buffer.len()
    }

    /// Reset the DC remover state.
    pub fn reset(&mut self) {
        self.buffer.clear();
        self.sum = 0.0;
    }
}

impl TimeDcRemover {
    /// Create a new time-based DC remover.
    ///
    /// # Arguments
    ///
    /// * `window` - Duration of the rolling window for DC estimation
    #[must_use]
    pub fn new(window: std::time::Duration) -> Self {
        #[allow(clippy::cast_possible_wrap)]
        let window_ms = window.as_millis() as i64;
        Self {
            buffer: VecDeque::new(),
            window_ms,
            sum: 0.0,
        }
    }

    /// Process a new timestamped sample and return the DC-removed value.
    pub fn update(&mut self, value: f64, ts_ms: i64) -> f64 {
        // Evict old samples
        let cutoff = ts_ms - self.window_ms;
        while let Some(&(old_ts, old_val)) = self.buffer.front() {
            if old_ts < cutoff {
                let _ = self.buffer.pop_front();
                self.sum -= old_val;
            } else {
                break;
            }
        }

        // Add new sample
        self.buffer.push_back((ts_ms, value));
        self.sum += value;

        // Compute and subtract DC
        let dc = self.sum / self.buffer.len() as f64;
        value - dc
    }

    /// Get the current DC estimate.
    #[must_use]
    pub fn dc_estimate(&self) -> f64 {
        if self.buffer.is_empty() {
            0.0
        } else {
            self.sum / self.buffer.len() as f64
        }
    }

    /// Reset the remover state.
    pub fn reset(&mut self) {
        self.buffer.clear();
        self.sum = 0.0;
    }
}

impl BaselineCorrector {
    /// Create a new baseline corrector.
    ///
    /// # Arguments
    ///
    /// * `window_samples` - Number of samples for baseline estimation
    /// * `percentile` - Percentile to use as baseline (0.0-1.0, typically 0.05-0.2)
    ///
    /// `window_samples` is clamped to a minimum of 1 and `percentile` to
    /// `[0.0, 1.0]`. Use [`try_new`](Self::try_new) to receive an error for an
    /// invalid argument instead.
    #[must_use]
    pub fn new(window_samples: usize, percentile: f64) -> Self {
        let window_samples = window_samples.max(1);
        let percentile = percentile.clamp(0.0, 1.0);
        Self {
            buffer: VecDeque::with_capacity(window_samples),
            sorted: Vec::with_capacity(window_samples),
            max_samples: window_samples,
            percentile,
        }
    }

    /// Create a new baseline corrector, validating its arguments.
    ///
    /// # Errors
    ///
    /// Returns [`TFloError::Configuration`](crate::error::TFloError::Configuration)
    /// if `window_samples` is 0 or `percentile` is not in `[0.0, 1.0]`.
    pub fn try_new(
        window_samples: usize,
        percentile: f64,
    ) -> Result<Self, crate::error::TFloError> {
        if window_samples == 0 {
            return Err(crate::error::TFloError::Configuration {
                message: "BaselineCorrector window_samples must be > 0".to_string(),
            });
        }
        if !(0.0..=1.0).contains(&percentile) {
            return Err(crate::error::TFloError::Configuration {
                message: "BaselineCorrector percentile must be in [0.0, 1.0]".to_string(),
            });
        }
        Ok(Self::new(window_samples, percentile))
    }

    /// Process a new sample and return the baseline-corrected value.
    pub fn update(&mut self, value: f64) -> f64 {
        // Add to buffer
        self.buffer.push_back(value);

        // Insert into sorted list (maintain sorted order)
        let pos = self.sorted.partition_point(|&x| x < value);
        self.sorted.insert(pos, value);

        // Remove old value if window is full
        if self.buffer.len() > self.max_samples {
            if let Some(old) = self.buffer.pop_front() {
                // Remove from sorted list
                if let Ok(pos) = self
                    .sorted
                    .binary_search_by(|a| a.partial_cmp(&old).unwrap_or(std::cmp::Ordering::Equal))
                {
                    let _ = self.sorted.remove(pos);
                }
            }
        }

        // Compute baseline (Nth percentile)
        let baseline = self.compute_percentile();

        value - baseline
    }

    fn compute_percentile(&self) -> f64 {
        if self.sorted.is_empty() {
            return 0.0;
        }

        let n = self.sorted.len();
        let idx = ((n - 1) as f64 * self.percentile).floor() as usize;
        let idx = idx.min(n - 1);

        self.sorted[idx]
    }

    /// Get the current baseline estimate.
    #[must_use]
    pub fn baseline(&self) -> f64 {
        self.compute_percentile()
    }

    /// Check if the window is fully populated.
    #[must_use]
    pub fn is_primed(&self) -> bool {
        self.buffer.len() >= self.max_samples
    }

    /// Reset the corrector state.
    pub fn reset(&mut self) {
        self.buffer.clear();
        self.sorted.clear();
    }
}

impl RangeNormalizer {
    /// Create a new range normalizer.
    ///
    /// # Arguments
    ///
    /// * `window_samples` - Number of samples for min/max estimation
    ///
    /// `window_samples` is clamped to a minimum of 1. Use
    /// [`try_new`](Self::try_new) to receive an error for an invalid window.
    #[must_use]
    pub fn new(window_samples: usize) -> Self {
        let window_samples = window_samples.max(1);
        Self {
            buffer: VecDeque::with_capacity(window_samples),
            max_samples: window_samples,
            current_min: f64::INFINITY,
            current_max: f64::NEG_INFINITY,
        }
    }

    /// Create a new range normalizer, validating the window size.
    ///
    /// # Errors
    ///
    /// Returns [`TFloError::Configuration`](crate::error::TFloError::Configuration)
    /// if `window_samples` is 0.
    pub fn try_new(window_samples: usize) -> Result<Self, crate::error::TFloError> {
        if window_samples == 0 {
            return Err(crate::error::TFloError::Configuration {
                message: "RangeNormalizer window_samples must be > 0".to_string(),
            });
        }
        Ok(Self::new(window_samples))
    }

    /// Process a new sample and return the normalized value [0, 1].
    ///
    /// Returns 0.5 if the range is zero (all values identical).
    pub fn update(&mut self, value: f64) -> f64 {
        // Add new value
        self.buffer.push_back(value);

        // Remove old value and recompute min/max if needed
        if self.buffer.len() > self.max_samples {
            let _ = self.buffer.pop_front();
            // Recompute min/max from scratch (could optimize with segment tree)
            self.recompute_minmax();
        } else {
            // Just update incrementally
            self.current_min = self.current_min.min(value);
            self.current_max = self.current_max.max(value);
        }

        // Normalize
        let range = self.current_max - self.current_min;
        if range.abs() < f64::EPSILON {
            0.5 // Avoid division by zero
        } else {
            (value - self.current_min) / range
        }
    }

    fn recompute_minmax(&mut self) {
        self.current_min = f64::INFINITY;
        self.current_max = f64::NEG_INFINITY;
        for &v in &self.buffer {
            self.current_min = self.current_min.min(v);
            self.current_max = self.current_max.max(v);
        }
    }

    /// Get the current observed range (min, max).
    #[must_use]
    pub fn range(&self) -> (f64, f64) {
        (self.current_min, self.current_max)
    }

    /// Check if the window is fully populated.
    #[must_use]
    pub fn is_primed(&self) -> bool {
        self.buffer.len() >= self.max_samples
    }

    /// Reset the normalizer state.
    pub fn reset(&mut self) {
        self.buffer.clear();
        self.current_min = f64::INFINITY;
        self.current_max = f64::NEG_INFINITY;
    }
}

impl ZScoreNormalizer {
    /// Create a new z-score normalizer.
    ///
    /// # Arguments
    ///
    /// * `window_samples` - Number of samples for mean/std estimation
    ///
    /// `window_samples` is clamped to a minimum of 2 (a standard deviation
    /// needs at least two samples). Use [`try_new`](Self::try_new) to receive
    /// an error for an invalid window instead.
    #[must_use]
    pub fn new(window_samples: usize) -> Self {
        let window_samples = window_samples.max(2);
        Self {
            buffer: VecDeque::with_capacity(window_samples),
            max_samples: window_samples,
            sum: 0.0,
            sum_sq: 0.0,
        }
    }

    /// Create a new z-score normalizer, validating the window size.
    ///
    /// # Errors
    ///
    /// Returns [`TFloError::Configuration`](crate::error::TFloError::Configuration)
    /// if `window_samples` is less than 2.
    pub fn try_new(window_samples: usize) -> Result<Self, crate::error::TFloError> {
        if window_samples < 2 {
            return Err(crate::error::TFloError::Configuration {
                message: "ZScoreNormalizer window_samples must be > 1 for std calculation"
                    .to_string(),
            });
        }
        Ok(Self::new(window_samples))
    }

    /// Process a new sample and return the z-score.
    ///
    /// Returns 0.0 if standard deviation is zero.
    pub fn update(&mut self, value: f64) -> f64 {
        // Add new value
        self.buffer.push_back(value);
        self.sum += value;
        self.sum_sq += value * value;

        // Remove old value if window is full
        if self.buffer.len() > self.max_samples {
            if let Some(old) = self.buffer.pop_front() {
                self.sum -= old;
                self.sum_sq -= old * old;
            }
        }

        // Compute z-score
        let n = self.buffer.len() as f64;
        let mean = self.sum / n;
        let variance = (self.sum_sq / n) - (mean * mean);
        let std = variance.max(0.0).sqrt();

        if std < f64::EPSILON {
            0.0
        } else {
            (value - mean) / std
        }
    }

    /// Get the current mean estimate.
    #[must_use]
    pub fn mean(&self) -> f64 {
        if self.buffer.is_empty() {
            0.0
        } else {
            self.sum / self.buffer.len() as f64
        }
    }

    /// Get the current standard deviation estimate.
    #[must_use]
    pub fn std(&self) -> f64 {
        if self.buffer.len() < 2 {
            return 0.0;
        }
        let n = self.buffer.len() as f64;
        let mean = self.sum / n;
        let variance = (self.sum_sq / n) - (mean * mean);
        variance.max(0.0).sqrt()
    }

    /// Check if the window is fully populated.
    #[must_use]
    pub fn is_primed(&self) -> bool {
        self.buffer.len() >= self.max_samples
    }

    /// Reset the normalizer state.
    pub fn reset(&mut self) {
        self.buffer.clear();
        self.sum = 0.0;
        self.sum_sq = 0.0;
    }
}
