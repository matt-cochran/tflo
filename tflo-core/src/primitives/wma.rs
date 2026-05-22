//! Weighted Moving Average (WMA) primitives.
//!
//! WMA gives more weight to recent observations using linearly decreasing weights.
//! The most recent value has weight N, the previous has weight N-1, etc.

use std::collections::VecDeque;
use std::time::Duration;

/// Count-based Weighted Moving Average.
///
/// Uses linearly decreasing weights where the most recent value has
/// weight N, the previous has weight N-1, and so on.
///
/// # Formula
///
/// WMA = (n*x_n + (n-1)*x_{n-1} + ... + 1*x_1) / (n + (n-1) + ... + 1)
///     = (n*x_n + (n-1)*x_{n-1} + ... + 1*x_1) / (n*(n+1)/2)
///
/// # Examples
///
/// ```rust
/// use tflo_core::primitives::WmaCountWindow;
///
/// let mut wma = WmaCountWindow::new(3);
///
/// wma.push(10.0);
/// wma.push(20.0);
/// wma.push(30.0);
///
/// // Weights: 1, 2, 3 for values 10, 20, 30
/// // WMA = (1*10 + 2*20 + 3*30) / (1+2+3) = (10 + 40 + 90) / 6 = 23.33...
/// let result = wma.wma();
/// assert!((result - 23.333333).abs() < 0.001);
/// ```
#[derive(Debug, Clone)]
pub struct WmaCountWindow {
    max_count: usize,
    buffer: VecDeque<f64>,
}

impl WmaCountWindow {
    /// Create a new count-based WMA window.
    #[must_use]
    pub fn new(max_count: usize) -> Self {
        Self {
            max_count,
            buffer: VecDeque::with_capacity(max_count),
        }
    }

    /// Add a new value.
    pub fn push(&mut self, value: f64) {
        if self.buffer.len() >= self.max_count {
            let _ = self.buffer.pop_front();
        }
        self.buffer.push_back(value);
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

    /// Get the weighted moving average.
    ///
    /// Returns `f64::NAN` if the window is empty.
    #[must_use]
    pub fn wma(&self) -> f64 {
        if self.buffer.is_empty() {
            return f64::NAN;
        }

        let n = self.buffer.len();
        let mut weighted_sum = 0.0;

        // Weight increases with recency: oldest gets weight 1, newest gets weight n
        for (i, &value) in self.buffer.iter().enumerate() {
            let weight = (i + 1) as f64;
            weighted_sum += weight * value;
        }

        // Sum of weights: 1 + 2 + ... + n = n*(n+1)/2
        let weight_sum = (n * (n + 1)) as f64 / 2.0;

        weighted_sum / weight_sum
    }

    /// Clear the window.
    pub fn clear(&mut self) {
        self.buffer.clear();
    }
}

/// Time-based Weighted Moving Average.
///
/// Uses time-based weights where more recent values have higher weights
/// based on their proximity to the current time.
#[derive(Debug, Clone)]
pub struct WmaTimeWindow {
    window_ms: i64,
    buffer: VecDeque<(i64, f64)>,
}

impl WmaTimeWindow {
    /// Create a new time-based WMA window.
    #[must_use]
    pub fn new(window: Duration) -> Self {
        #[allow(clippy::cast_possible_wrap)]
        let window_ms = window.as_millis() as i64;
        Self {
            window_ms,
            buffer: VecDeque::new(),
        }
    }

    /// Add a new value at the given timestamp.
    pub fn push(&mut self, ts: i64, value: f64) {
        // Add new value
        self.buffer.push_back((ts, value));

        // Evict old values
        let cutoff = ts - self.window_ms;
        while let Some(&(old_ts, _)) = self.buffer.front() {
            if old_ts < cutoff {
                let _ = self.buffer.pop_front();
            } else {
                break;
            }
        }
    }

    /// Get the number of values in the window.
    #[must_use]
    pub fn count(&self) -> usize {
        self.buffer.len()
    }

    /// Get the weighted moving average.
    ///
    /// Weights are based on time proximity: values closer to the current time
    /// have higher weights.
    ///
    /// Returns `f64::NAN` if the window is empty.
    #[must_use]
    pub fn wma(&self) -> f64 {
        if self.buffer.is_empty() {
            return f64::NAN;
        }

        if self.buffer.len() == 1 {
            return self.buffer[0].1;
        }

        // Get the time range
        let oldest_ts = self.buffer.front().map(|(ts, _)| *ts).unwrap_or(0);
        let newest_ts = self.buffer.back().map(|(ts, _)| *ts).unwrap_or(0);
        let time_span = (newest_ts - oldest_ts) as f64;

        if time_span <= 0.0 {
            // All at same timestamp - use equal weights (simple average)
            let sum: f64 = self.buffer.iter().map(|(_, v)| v).sum();
            return sum / self.buffer.len() as f64;
        }

        let mut weighted_sum = 0.0;
        let mut weight_sum = 0.0;

        for &(ts, value) in &self.buffer {
            // Weight increases with recency: 0 at oldest, 1 at newest
            let relative_time = (ts - oldest_ts) as f64 / time_span;
            let weight = relative_time + 0.001; // Small epsilon to avoid zero weight
            weighted_sum += weight * value;
            weight_sum += weight;
        }

        weighted_sum / weight_sum
    }

    /// Clear the window.
    pub fn clear(&mut self) {
        self.buffer.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_wma_count_basic() {
        let mut wma = WmaCountWindow::new(3);

        wma.push(10.0);
        wma.push(20.0);
        wma.push(30.0);

        // Weights: 1, 2, 3 for values 10, 20, 30
        // WMA = (1*10 + 2*20 + 3*30) / 6 = 140 / 6 = 23.333...
        let expected = (1.0 * 10.0 + 2.0 * 20.0 + 3.0 * 30.0) / 6.0;
        assert!((wma.wma() - expected).abs() < 0.0001);
    }

    #[test]
    fn test_wma_emphasizes_recent() {
        let mut wma = WmaCountWindow::new(3);

        // First scenario: recent values are high
        wma.push(10.0);
        wma.push(10.0);
        wma.push(100.0);
        let wma_high_recent = wma.wma();

        wma.clear();

        // Second scenario: recent values are low
        wma.push(100.0);
        wma.push(10.0);
        wma.push(10.0);
        let wma_low_recent = wma.wma();

        // WMA should be higher when recent values are higher
        assert!(wma_high_recent > wma_low_recent);
    }

    #[test]
    fn test_wma_count_single_value() {
        let mut wma = WmaCountWindow::new(5);

        wma.push(42.0);
        assert!((wma.wma() - 42.0).abs() < 0.0001);
    }

    #[test]
    fn test_wma_count_eviction() {
        let mut wma = WmaCountWindow::new(2);

        wma.push(10.0);
        wma.push(20.0);
        wma.push(30.0); // Evicts 10.0

        assert_eq!(wma.count(), 2);
        // Weights: 1, 2 for values 20, 30
        // WMA = (1*20 + 2*30) / 3 = 80/3 = 26.666...
        let expected = (1.0 * 20.0 + 2.0 * 30.0) / 3.0;
        assert!((wma.wma() - expected).abs() < 0.0001);
    }

    #[test]
    fn test_wma_empty() {
        let wma = WmaCountWindow::new(5);
        assert!(wma.wma().is_nan());
    }

    #[test]
    fn test_wma_time_basic() {
        let mut wma = WmaTimeWindow::new(Duration::from_secs(5));

        wma.push(1000, 10.0);
        wma.push(2000, 20.0);
        wma.push(3000, 30.0);

        // More recent values should be weighted higher
        let result = wma.wma();
        assert!(result > 20.0); // Should be above simple average
    }

    #[test]
    fn test_wma_time_eviction() {
        let mut wma = WmaTimeWindow::new(Duration::from_secs(5));

        wma.push(1000, 10.0);
        wma.push(2000, 20.0);
        wma.push(3000, 30.0);
        wma.push(7000, 40.0); // Evicts 1000

        assert_eq!(wma.count(), 3);
    }
}
