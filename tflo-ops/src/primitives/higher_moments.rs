//! Higher-order moments: skewness and kurtosis.
//!
//! Provides online calculation of skewness and kurtosis using
//! incremental algorithms based on central moments.

use std::collections::VecDeque;
use std::time::Duration;
use tflo_core::operator::WindowPrimitive;

/// Count-based sliding window for higher-order moments (skewness, kurtosis).
///
/// Uses incremental updates based on the formulas for central moments.
///
/// # Examples
///
/// ```rust
/// use tflo_ops::primitives::MomentsCountWindow;
///
/// let mut window = MomentsCountWindow::new(10);
///
/// // Push some values
/// for x in [1.0, 2.0, 2.0, 3.0, 3.0, 3.0, 4.0, 4.0, 4.0, 4.0] {
///     window.push(x);
/// }
///
/// // This distribution is left-skewed (negative skewness)
/// let skew = window.skewness();
/// assert!(skew < 0.0);
/// ```
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MomentsCountWindow {
    max_count: usize,
    buffer: VecDeque<f64>,
    // Running sums for computing moments
    sum: f64,
    sum_sq: f64,
    sum_cube: f64,
    sum_quad: f64,
}

impl MomentsCountWindow {
    /// Create a new count-based moments window.
    #[must_use]
    pub fn new(max_count: usize) -> Self {
        Self {
            max_count,
            buffer: VecDeque::with_capacity(max_count),
            sum: 0.0,
            sum_sq: 0.0,
            sum_cube: 0.0,
            sum_quad: 0.0,
        }
    }

    /// Add a new value.
    pub fn push(&mut self, value: f64) {
        // Evict oldest if at capacity
        if self.buffer.len() >= self.max_count {
            if let Some(old) = self.buffer.pop_front() {
                self.sum -= old;
                self.sum_sq -= old * old;
                self.sum_cube -= old * old * old;
                self.sum_quad -= old * old * old * old;
            }
        }

        // Add new value
        self.buffer.push_back(value);
        self.sum += value;
        self.sum_sq += value * value;
        self.sum_cube += value * value * value;
        self.sum_quad += value * value * value * value;
    }

    /// Get the number of values in the window.
    #[must_use]
    pub fn count(&self) -> usize {
        self.buffer.len()
    }

    /// Check if the window is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.buffer.is_empty()
    }

    /// Get the mean.
    #[must_use]
    pub fn mean(&self) -> f64 {
        if self.buffer.is_empty() {
            f64::NAN
        } else {
            self.sum / self.buffer.len() as f64
        }
    }

    /// Get the variance (population).
    #[must_use]
    pub fn variance(&self) -> f64 {
        let n = self.buffer.len() as f64;
        if n < 2.0 {
            return f64::NAN;
        }
        let mean = self.sum / n;
        ((self.sum_sq / n) - (mean * mean)).max(0.0)
    }

    /// Get the standard deviation (population).
    #[must_use]
    pub fn std(&self) -> f64 {
        self.variance().sqrt()
    }

    /// Get the skewness (Fisher's definition).
    ///
    /// Skewness measures the asymmetry of the distribution:
    /// - Positive skew: tail extends to the right
    /// - Negative skew: tail extends to the left
    /// - Zero: symmetric distribution
    ///
    /// Returns `f64::NAN` if fewer than 3 values or zero variance.
    #[must_use]
    pub fn skewness(&self) -> f64 {
        let n = self.buffer.len() as f64;
        if n < 3.0 {
            return f64::NAN;
        }

        let mean = self.sum / n;
        let variance = (self.sum_sq / n) - (mean * mean);

        if variance <= 0.0 {
            return f64::NAN;
        }

        let std = variance.sqrt();

        // Third central moment: E[(X - μ)³]
        // Simplify: E[X³] - 3μE[X²] + 2μ³
        let m3 = (self.sum_cube / n) - 3.0 * mean * (self.sum_sq / n) + 2.0 * mean.powi(3);

        // Skewness = m3 / std³
        m3 / std.powi(3)
    }

    /// Get the excess kurtosis (Fisher's definition).
    ///
    /// Kurtosis measures the "tailedness" of the distribution:
    /// - Positive (leptokurtic): heavier tails than normal
    /// - Negative (platykurtic): lighter tails than normal
    /// - Zero (mesokurtic): similar to normal distribution
    ///
    /// Note: This returns *excess* kurtosis (kurtosis - 3), so a normal
    /// distribution has excess kurtosis of 0.
    ///
    /// Returns `f64::NAN` if fewer than 4 values or zero variance.
    #[must_use]
    pub fn kurtosis(&self) -> f64 {
        let n = self.buffer.len() as f64;
        if n < 4.0 {
            return f64::NAN;
        }

        let mean = self.sum / n;
        let variance = (self.sum_sq / n) - (mean * mean);

        if variance <= 0.0 {
            return f64::NAN;
        }

        // Fourth central moment: E[(X - μ)⁴]
        // Expand and simplify to: E[X⁴] - 4μE[X³] + 6μ²E[X²] - 3μ⁴
        let m4 = (self.sum_quad / n) - 4.0 * mean * (self.sum_cube / n)
            + 6.0 * mean * mean * (self.sum_sq / n)
            - 3.0 * mean.powi(4);

        // Excess kurtosis = m4 / variance² - 3
        (m4 / variance.powi(2)) - 3.0
    }

    /// Clear all values from the window.
    pub fn clear(&mut self) {
        self.buffer.clear();
        self.sum = 0.0;
        self.sum_sq = 0.0;
        self.sum_cube = 0.0;
        self.sum_quad = 0.0;
    }
}

/// Time-based sliding window for higher-order moments.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MomentsTimeWindow {
    window_ms: i64,
    buffer: VecDeque<(i64, f64)>,
    sum: f64,
    sum_sq: f64,
    sum_cube: f64,
    sum_quad: f64,
}

impl MomentsTimeWindow {
    /// Create a new time-based moments window.
    #[must_use]
    pub const fn new(window: Duration) -> Self {
        #[allow(clippy::cast_possible_wrap)]
        let window_ms = window.as_millis() as i64;
        Self {
            window_ms,
            buffer: VecDeque::new(),
            sum: 0.0,
            sum_sq: 0.0,
            sum_cube: 0.0,
            sum_quad: 0.0,
        }
    }

    /// Add a new value at the given timestamp.
    pub fn push(&mut self, ts: i64, value: f64) {
        // Add new value
        self.buffer.push_back((ts, value));
        self.sum += value;
        self.sum_sq += value * value;
        self.sum_cube += value * value * value;
        self.sum_quad += value * value * value * value;

        // Evict old values
        let cutoff = ts - self.window_ms;
        while let Some(&(old_ts, old_val)) = self.buffer.front() {
            if old_ts < cutoff {
                let _ = self.buffer.pop_front();
                self.sum -= old_val;
                self.sum_sq -= old_val * old_val;
                self.sum_cube -= old_val * old_val * old_val;
                self.sum_quad -= old_val * old_val * old_val * old_val;
            } else {
                break;
            }
        }
    }

    /// Get the number of values.
    #[must_use]
    pub fn count(&self) -> usize {
        self.buffer.len()
    }

    /// Get the mean.
    #[must_use]
    pub fn mean(&self) -> f64 {
        if self.buffer.is_empty() {
            f64::NAN
        } else {
            self.sum / self.buffer.len() as f64
        }
    }

    /// Get the skewness.
    #[must_use]
    pub fn skewness(&self) -> f64 {
        let n = self.buffer.len() as f64;
        if n < 3.0 {
            return f64::NAN;
        }

        let mean = self.sum / n;
        let variance = (self.sum_sq / n) - (mean * mean);

        if variance <= 0.0 {
            return f64::NAN;
        }

        let std = variance.sqrt();
        let m3 = (self.sum_cube / n) - 3.0 * mean * (self.sum_sq / n) + 2.0 * mean.powi(3);
        m3 / std.powi(3)
    }

    /// Get the excess kurtosis.
    #[must_use]
    pub fn kurtosis(&self) -> f64 {
        let n = self.buffer.len() as f64;
        if n < 4.0 {
            return f64::NAN;
        }

        let mean = self.sum / n;
        let variance = (self.sum_sq / n) - (mean * mean);

        if variance <= 0.0 {
            return f64::NAN;
        }

        let m4 = (self.sum_quad / n) - 4.0 * mean * (self.sum_cube / n)
            + 6.0 * mean * mean * (self.sum_sq / n)
            - 3.0 * mean.powi(4);

        (m4 / variance.powi(2)) - 3.0
    }

    /// Clear the window.
    pub fn clear(&mut self) {
        self.buffer.clear();
        self.sum = 0.0;
        self.sum_sq = 0.0;
        self.sum_cube = 0.0;
        self.sum_quad = 0.0;
    }
}

impl WindowPrimitive for MomentsCountWindow {
    fn push(&mut self, _ts: i64, value: f64) {
        self.push(value);
    }

    fn len(&self) -> usize {
        self.count()
    }

    fn is_empty(&self) -> bool {
        self.is_empty()
    }
}

impl WindowPrimitive for MomentsTimeWindow {
    fn push(&mut self, ts: i64, value: f64) {
        self.push(ts, value);
    }

    fn len(&self) -> usize {
        self.count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_symmetric_distribution() {
        let mut window = MomentsCountWindow::new(7);

        // Symmetric distribution should have near-zero skewness
        for x in [1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0] {
            window.push(x);
        }

        let skew = window.skewness();
        assert!(
            skew.abs() < 0.1,
            "Expected near-zero skewness, got {skew}"
        );
    }

    #[test]
    fn test_right_skewed_distribution() {
        let mut window = MomentsCountWindow::new(10);

        // Right-skewed: more small values, few large values
        for x in [1.0, 1.0, 1.0, 1.0, 2.0, 2.0, 2.0, 3.0, 5.0, 10.0] {
            window.push(x);
        }

        let skew = window.skewness();
        assert!(skew > 0.0, "Expected positive skewness, got {skew}");
    }

    #[test]
    fn test_left_skewed_distribution() {
        let mut window = MomentsCountWindow::new(10);

        // Left-skewed: more large values, few small values
        for x in [1.0, 5.0, 7.0, 8.0, 8.0, 8.0, 9.0, 9.0, 9.0, 9.0] {
            window.push(x);
        }

        let skew = window.skewness();
        assert!(skew < 0.0, "Expected negative skewness, got {skew}");
    }

    #[test]
    fn test_uniform_distribution_kurtosis() {
        let mut window = MomentsCountWindow::new(11);

        // Uniform distribution has negative excess kurtosis (platykurtic)
        for i in 0..=10 {
            window.push(i as f64);
        }

        let kurt = window.kurtosis();
        // Uniform distribution has excess kurtosis of -1.2
        assert!(
            kurt < 0.0,
            "Expected negative excess kurtosis for uniform, got {kurt}"
        );
    }

    #[test]
    fn test_insufficient_data() {
        let mut window = MomentsCountWindow::new(10);

        window.push(1.0);
        window.push(2.0);
        assert!(window.skewness().is_nan()); // Need at least 3

        window.push(3.0);
        assert!(!window.skewness().is_nan()); // Now we have 3

        assert!(window.kurtosis().is_nan()); // Need at least 4

        window.push(4.0);
        assert!(!window.kurtosis().is_nan()); // Now we have 4
    }

    #[test]
    fn test_eviction() {
        let mut window = MomentsCountWindow::new(3);

        window.push(1.0);
        window.push(2.0);
        window.push(3.0);
        window.push(4.0); // Evicts 1.0

        assert_eq!(window.count(), 3);

        // Mean should be (2+3+4)/3 = 3
        assert!((window.mean() - 3.0).abs() < 0.0001);
    }

    #[test]
    fn test_time_window() {
        let mut window = MomentsTimeWindow::new(Duration::from_secs(5));

        window.push(1000, 1.0);
        window.push(2000, 2.0);
        window.push(3000, 3.0);
        window.push(4000, 4.0);

        assert_eq!(window.count(), 4);

        // Push value that evicts first
        window.push(7000, 5.0);
        assert_eq!(window.count(), 4);
    }
}
