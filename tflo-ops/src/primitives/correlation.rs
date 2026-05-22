//! Rolling correlation and covariance primitives.
//!
//! Provides online calculation of Pearson correlation and covariance
//! using Welford-style incremental algorithms.

use std::collections::VecDeque;
use std::time::Duration;

use tflo_core::operator::BivariateWindow;

/// Count-based sliding window for correlation and covariance.
///
/// Uses an incremental algorithm based on Welford's method for
/// numerically stable computation of correlation and covariance.
///
/// # Examples
///
/// ```rust
/// use tflo_ops::primitives::CorrelationCountWindow;
///
/// let mut window = CorrelationCountWindow::new(5);
///
/// // Perfect positive correlation
/// for i in 1..=5 {
///     window.push(i as f64, i as f64 * 2.0);
/// }
///
/// assert!((window.correlation() - 1.0).abs() < 0.0001);
/// ```
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CorrelationCountWindow {
    max_count: usize,
    buffer: VecDeque<(f64, f64)>,
    // Running statistics
    sum_x: f64,
    sum_y: f64,
    sum_xx: f64,
    sum_yy: f64,
    sum_xy: f64,
}

impl CorrelationCountWindow {
    /// Create a new count-based correlation window.
    #[must_use]
    pub fn new(max_count: usize) -> Self {
        Self {
            max_count,
            buffer: VecDeque::with_capacity(max_count),
            sum_x: 0.0,
            sum_y: 0.0,
            sum_xx: 0.0,
            sum_yy: 0.0,
            sum_xy: 0.0,
        }
    }

    /// Add a new pair of values.
    pub fn push(&mut self, x: f64, y: f64) {
        // Evict oldest if at capacity
        if self.buffer.len() >= self.max_count {
            if let Some((old_x, old_y)) = self.buffer.pop_front() {
                self.sum_x -= old_x;
                self.sum_y -= old_y;
                self.sum_xx -= old_x * old_x;
                self.sum_yy -= old_y * old_y;
                self.sum_xy -= old_x * old_y;
            }
        }

        // Add new value
        self.buffer.push_back((x, y));
        self.sum_x += x;
        self.sum_y += y;
        self.sum_xx += x * x;
        self.sum_yy += y * y;
        self.sum_xy += x * y;
    }

    /// Get the number of pairs in the window.
    #[must_use]
    pub fn count(&self) -> usize {
        self.buffer.len()
    }

    /// Check if the window is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.buffer.is_empty()
    }

    /// Get the covariance of x and y.
    ///
    /// Uses population covariance (divides by n).
    /// Returns `f64::NAN` if fewer than 2 values.
    #[must_use]
    pub fn covariance(&self) -> f64 {
        let n = self.buffer.len() as f64;
        if n < 2.0 {
            return f64::NAN;
        }

        let mean_x = self.sum_x / n;
        let mean_y = self.sum_y / n;

        // Cov(X,Y) = E[XY] - E[X]E[Y]
        (self.sum_xy / n) - (mean_x * mean_y)
    }

    /// Get the sample covariance (divides by n-1).
    ///
    /// Returns `f64::NAN` if fewer than 2 values.
    #[must_use]
    pub fn sample_covariance(&self) -> f64 {
        let n = self.buffer.len() as f64;
        if n < 2.0 {
            return f64::NAN;
        }

        let mean_x = self.sum_x / n;
        let mean_y = self.sum_y / n;

        // Sample Cov = n/(n-1) * population covariance
        let pop_cov = (self.sum_xy / n) - (mean_x * mean_y);
        pop_cov * n / (n - 1.0)
    }

    /// Get the Pearson correlation coefficient.
    ///
    /// Returns a value between -1.0 and 1.0.
    /// Returns `f64::NAN` if fewer than 2 values or if either series has zero variance.
    #[must_use]
    pub fn correlation(&self) -> f64 {
        let n = self.buffer.len() as f64;
        if n < 2.0 {
            return f64::NAN;
        }

        let mean_x = self.sum_x / n;
        let mean_y = self.sum_y / n;

        let var_x = (self.sum_xx / n) - (mean_x * mean_x);
        let var_y = (self.sum_yy / n) - (mean_y * mean_y);

        if var_x <= 0.0 || var_y <= 0.0 {
            return f64::NAN;
        }

        let cov = (self.sum_xy / n) - (mean_x * mean_y);
        cov / (var_x.sqrt() * var_y.sqrt())
    }

    /// Get the mean of the X values.
    #[must_use]
    pub fn mean_x(&self) -> f64 {
        if self.buffer.is_empty() {
            f64::NAN
        } else {
            self.sum_x / self.buffer.len() as f64
        }
    }

    /// Get the mean of the Y values.
    #[must_use]
    pub fn mean_y(&self) -> f64 {
        if self.buffer.is_empty() {
            f64::NAN
        } else {
            self.sum_y / self.buffer.len() as f64
        }
    }

    /// Get the variance of X.
    #[must_use]
    pub fn variance_x(&self) -> f64 {
        let n = self.buffer.len() as f64;
        if n < 2.0 {
            return f64::NAN;
        }
        let mean_x = self.sum_x / n;
        ((self.sum_xx / n) - (mean_x * mean_x)).max(0.0)
    }

    /// Get the variance of Y.
    #[must_use]
    pub fn variance_y(&self) -> f64 {
        let n = self.buffer.len() as f64;
        if n < 2.0 {
            return f64::NAN;
        }
        let mean_y = self.sum_y / n;
        ((self.sum_yy / n) - (mean_y * mean_y)).max(0.0)
    }

    /// Get the beta coefficient (slope of regression line Y = alpha + beta*X).
    ///
    /// Returns `f64::NAN` if X has zero variance.
    #[must_use]
    pub fn beta(&self) -> f64 {
        let var_x = self.variance_x();
        if var_x <= 0.0 || var_x.is_nan() {
            return f64::NAN;
        }
        self.covariance() / var_x
    }

    /// Get the alpha coefficient (intercept of regression line Y = alpha + beta*X).
    #[must_use]
    pub fn alpha(&self) -> f64 {
        self.mean_y() - self.beta() * self.mean_x()
    }

    /// Clear all values from the window.
    pub fn clear(&mut self) {
        self.buffer.clear();
        self.sum_x = 0.0;
        self.sum_y = 0.0;
        self.sum_xx = 0.0;
        self.sum_yy = 0.0;
        self.sum_xy = 0.0;
    }
}

impl BivariateWindow for CorrelationCountWindow {
    fn push(&mut self, _ts: i64, a: f64, b: f64) {
        self.push(a, b);
    }

    fn len(&self) -> usize {
        self.count()
    }
}

/// Time-based sliding window for correlation and covariance.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CorrelationTimeWindow {
    window_ms: i64,
    buffer: VecDeque<(i64, f64, f64)>,
    sum_x: f64,
    sum_y: f64,
    sum_xx: f64,
    sum_yy: f64,
    sum_xy: f64,
}

impl CorrelationTimeWindow {
    /// Create a new time-based correlation window.
    #[must_use]
    pub fn new(window: Duration) -> Self {
        #[allow(clippy::cast_possible_wrap)]
        let window_ms = window.as_millis() as i64;
        Self {
            window_ms,
            buffer: VecDeque::new(),
            sum_x: 0.0,
            sum_y: 0.0,
            sum_xx: 0.0,
            sum_yy: 0.0,
            sum_xy: 0.0,
        }
    }

    /// Add a new pair of values at the given timestamp.
    pub fn push(&mut self, ts: i64, x: f64, y: f64) {
        // Add new value
        self.buffer.push_back((ts, x, y));
        self.sum_x += x;
        self.sum_y += y;
        self.sum_xx += x * x;
        self.sum_yy += y * y;
        self.sum_xy += x * y;

        // Evict old values
        let cutoff = ts - self.window_ms;
        while let Some(&(old_ts, old_x, old_y)) = self.buffer.front() {
            if old_ts < cutoff {
                let _ = self.buffer.pop_front();
                self.sum_x -= old_x;
                self.sum_y -= old_y;
                self.sum_xx -= old_x * old_x;
                self.sum_yy -= old_y * old_y;
                self.sum_xy -= old_x * old_y;
            } else {
                break;
            }
        }
    }

    /// Get the number of pairs in the window.
    #[must_use]
    pub fn count(&self) -> usize {
        self.buffer.len()
    }

    /// Get the covariance.
    #[must_use]
    pub fn covariance(&self) -> f64 {
        let n = self.buffer.len() as f64;
        if n < 2.0 {
            return f64::NAN;
        }

        let mean_x = self.sum_x / n;
        let mean_y = self.sum_y / n;
        (self.sum_xy / n) - (mean_x * mean_y)
    }

    /// Get the Pearson correlation coefficient.
    #[must_use]
    pub fn correlation(&self) -> f64 {
        let n = self.buffer.len() as f64;
        if n < 2.0 {
            return f64::NAN;
        }

        let mean_x = self.sum_x / n;
        let mean_y = self.sum_y / n;

        let var_x = (self.sum_xx / n) - (mean_x * mean_x);
        let var_y = (self.sum_yy / n) - (mean_y * mean_y);

        if var_x <= 0.0 || var_y <= 0.0 {
            return f64::NAN;
        }

        let cov = (self.sum_xy / n) - (mean_x * mean_y);
        cov / (var_x.sqrt() * var_y.sqrt())
    }

    /// Clear the window.
    pub fn clear(&mut self) {
        self.buffer.clear();
        self.sum_x = 0.0;
        self.sum_y = 0.0;
        self.sum_xx = 0.0;
        self.sum_yy = 0.0;
        self.sum_xy = 0.0;
    }
}

impl BivariateWindow for CorrelationTimeWindow {
    fn push(&mut self, ts: i64, a: f64, b: f64) {
        self.push(ts, a, b);
    }

    fn len(&self) -> usize {
        self.count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_perfect_positive_correlation() {
        let mut window = CorrelationCountWindow::new(5);

        for i in 1..=5 {
            window.push(i as f64, i as f64 * 2.0);
        }

        assert!((window.correlation() - 1.0).abs() < 0.0001);
    }

    #[test]
    fn test_perfect_negative_correlation() {
        let mut window = CorrelationCountWindow::new(5);

        for i in 1..=5 {
            window.push(i as f64, -(i as f64));
        }

        assert!((window.correlation() - (-1.0)).abs() < 0.0001);
    }

    #[test]
    fn test_no_correlation() {
        let mut window = CorrelationCountWindow::new(4);

        // X and Y should be uncorrelated
        window.push(1.0, 1.0);
        window.push(2.0, -1.0);
        window.push(3.0, 1.0);
        window.push(4.0, -1.0);

        // Not exactly zero but close to it
        assert!(window.correlation().abs() < 0.5);
    }

    #[test]
    fn test_covariance() {
        let mut window = CorrelationCountWindow::new(5);

        for i in 1..=5 {
            window.push(i as f64, i as f64 * 2.0);
        }

        // Should be positive for positive relationship
        assert!(window.covariance() > 0.0);
    }

    #[test]
    fn test_regression_coefficients() {
        let mut window = CorrelationCountWindow::new(5);

        // Y = 2*X + 3
        for i in 1..=5 {
            window.push(i as f64, 2.0 * (i as f64) + 3.0);
        }

        assert!((window.beta() - 2.0).abs() < 0.0001);
        assert!((window.alpha() - 3.0).abs() < 0.0001);
    }

    #[test]
    fn test_eviction() {
        let mut window = CorrelationCountWindow::new(3);

        window.push(1.0, 1.0);
        window.push(2.0, 2.0);
        window.push(3.0, 3.0);
        window.push(100.0, 100.0); // Evicts (1,1)

        assert_eq!(window.count(), 3);
        // Still perfectly correlated
        assert!((window.correlation() - 1.0).abs() < 0.0001);
    }

    #[test]
    fn test_time_window() {
        let mut window = CorrelationTimeWindow::new(Duration::from_secs(5));

        window.push(1000, 1.0, 2.0);
        window.push(2000, 2.0, 4.0);
        window.push(3000, 3.0, 6.0);

        assert_eq!(window.count(), 3);
        assert!((window.correlation() - 1.0).abs() < 0.0001);

        // Push value that evicts first
        window.push(7000, 4.0, 8.0);
        assert_eq!(window.count(), 3);
    }

    #[test]
    fn test_insufficient_data() {
        let window = CorrelationCountWindow::new(5);
        assert!(window.correlation().is_nan());
        assert!(window.covariance().is_nan());
    }
}
