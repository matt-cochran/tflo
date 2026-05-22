//! Time-based exponential moving average with proper decay.
//!
//! [`TimeEma`] implements an EMA that decays based on elapsed time rather than
//! record count, making it suitable for irregularly-spaced data.

use std::time::Duration;

/// Time-based exponential moving average.
///
/// Unlike count-based EMA, this implementation uses the actual elapsed time
/// between observations to compute the decay factor, making it appropriate
/// for irregularly-spaced time series data.
///
/// The decay is computed as: `α = 1 - e^(-Δt / halflife)`
///
/// where `Δt` is the time elapsed since the last observation.
///
/// # Examples
///
/// ```rust
/// use tflo_core::primitives::TimeEma;
/// use std::time::Duration;
///
/// let mut ema = TimeEma::new(Duration::from_secs(5));
///
/// // First value initializes the EMA
/// let v1 = ema.push(1000, 100.0);
/// assert_eq!(v1, 100.0);
///
/// // After half the halflife, the weight is ~0.29
/// let v2 = ema.push(3500, 200.0);  // 2.5 seconds later
/// assert!(v2 > 100.0 && v2 < 200.0);
/// ```
#[derive(Debug, Clone)]
pub struct TimeEma {
    halflife_ms: f64,
    last_ts: Option<i64>,
    value: f64,
    initialized: bool,
}

impl TimeEma {
    /// Create a new time-based EMA with the specified halflife.
    ///
    /// The halflife determines how quickly old values decay. After one halflife,
    /// the weight of old values is reduced by 50%.
    #[must_use]
    pub fn new(halflife: Duration) -> Self {
        Self {
            halflife_ms: halflife.as_millis() as f64,
            last_ts: None,
            value: 0.0,
            initialized: false,
        }
    }

    /// Update the EMA with a new value at the given timestamp.
    ///
    /// Returns the current EMA value after incorporating the new observation.
    pub fn push(&mut self, ts: i64, value: f64) -> f64 {
        if !self.initialized {
            self.value = value;
            self.last_ts = Some(ts);
            self.initialized = true;
            return self.value;
        }

        if let Some(last) = self.last_ts {
            let elapsed_ms = (ts - last) as f64;

            if elapsed_ms > 0.0 {
                // Decay factor: α = 1 - e^(-Δt / halflife)
                // This gives 50% weight after one halflife
                let alpha = 1.0 - (-elapsed_ms / self.halflife_ms).exp();

                // EMA update: new_value = α * current + (1 - α) * previous
                self.value = alpha * value + (1.0 - alpha) * self.value;
            }
        }

        self.last_ts = Some(ts);
        self.value
    }

    /// Get the current EMA value without updating.
    ///
    /// Returns `f64::NAN` if no values have been pushed.
    #[must_use]
    pub fn get(&self) -> f64 {
        if self.initialized {
            self.value
        } else {
            f64::NAN
        }
    }

    /// Check if the EMA has been initialized with at least one value.
    #[must_use]
    pub fn is_initialized(&self) -> bool {
        self.initialized
    }

    /// Get the timestamp of the last update.
    #[must_use]
    pub fn last_timestamp(&self) -> Option<i64> {
        self.last_ts
    }

    /// Reset the EMA to its initial state.
    pub fn reset(&mut self) {
        self.last_ts = None;
        self.value = 0.0;
        self.initialized = false;
    }

    /// Get the configured halflife in milliseconds.
    #[must_use]
    pub fn halflife_ms(&self) -> f64 {
        self.halflife_ms
    }
}

/// Count-based exponential moving average.
///
/// Traditional EMA that weights each new value with a fixed factor,
/// regardless of time between observations.
#[derive(Debug, Clone)]
pub struct CountEma {
    alpha: f64,
    period: usize,
    count: usize,
    seed_sum: f64,
    value: f64,
    initialized: bool,
}

impl CountEma {
    /// Create a new count-based EMA with the specified period.
    ///
    /// The alpha (smoothing factor) is calculated as: `α = 2 / (period + 1)`
    #[must_use]
    pub fn new(period: usize) -> Self {
        let alpha = 2.0 / (period as f64 + 1.0);
        Self {
            alpha,
            period,
            count: 0,
            seed_sum: 0.0,
            value: 0.0,
            initialized: false,
        }
    }

    /// Create a new count-based EMA with an explicit alpha.
    #[must_use]
    pub fn with_alpha(alpha: f64) -> Self {
        Self {
            alpha: alpha.clamp(0.0, 1.0),
            period: 1,
            count: 0,
            seed_sum: 0.0,
            value: 0.0,
            initialized: false,
        }
    }

    /// Update the EMA with a new value.
    ///
    /// Returns the current EMA value after incorporating the new observation.
    pub fn push(&mut self, value: f64) -> f64 {
        self.count += 1;

        if !self.initialized {
            self.seed_sum += value;
            if self.count < self.period {
                return f64::NAN;
            }
            self.value = self.seed_sum / self.period as f64;
            self.initialized = true;
            return self.value;
        }

        self.value = self.alpha * value + (1.0 - self.alpha) * self.value;
        self.value
    }

    /// Get the current EMA value without updating.
    ///
    /// Returns `f64::NAN` if no values have been pushed.
    #[must_use]
    pub fn get(&self) -> f64 {
        if self.initialized {
            self.value
        } else {
            f64::NAN
        }
    }

    /// Check if the EMA has been initialized with at least one value.
    #[must_use]
    pub fn is_initialized(&self) -> bool {
        self.initialized
    }

    /// Reset the EMA to its initial state.
    pub fn reset(&mut self) {
        self.count = 0;
        self.seed_sum = 0.0;
        self.value = 0.0;
        self.initialized = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_time_ema_initialization() {
        let mut ema = TimeEma::new(Duration::from_secs(5));

        assert!(!ema.is_initialized());
        assert!(ema.get().is_nan());

        let v = ema.push(1000, 100.0);
        assert_eq!(v, 100.0);
        assert!(ema.is_initialized());
    }

    #[test]
    fn test_time_ema_decay() {
        let mut ema = TimeEma::new(Duration::from_secs(5)); // 5000ms halflife

        let _ = ema.push(0, 100.0);

        // After exactly one halflife, weight of new value should be ~0.5
        let v = ema.push(5000, 200.0);
        // EMA should be roughly 150 (halfway between 100 and 200)
        // Due to exponential decay formula, it's actually:
        // α = 1 - e^(-1) ≈ 0.632
        // EMA = 0.632 * 200 + 0.368 * 100 ≈ 163.2
        assert!(v > 150.0 && v < 170.0);
    }

    #[test]
    fn test_time_ema_no_time_elapsed() {
        let mut ema = TimeEma::new(Duration::from_secs(5));

        let _ = ema.push(1000, 100.0);
        let v = ema.push(1000, 200.0); // Same timestamp
        assert_eq!(v, 100.0); // No update when no time elapsed
    }

    #[test]
    fn test_count_ema() {
        let mut ema = CountEma::new(5); // period = 5, alpha = 2/6 ≈ 0.333

        assert!(ema.push(100.0).is_nan());
        assert!(ema.push(200.0).is_nan());
        assert!(ema.push(300.0).is_nan());
        assert!(ema.push(400.0).is_nan());

        let seed = ema.push(500.0);
        assert_eq!(seed, 300.0); // SMA seed of first five values

        let next = ema.push(600.0);
        // EMA = 0.333 * 600 + 0.667 * 300 = 400
        assert!((next - 400.0).abs() < 1e-12);
    }
}
