//! Time-based sliding window aggregator.
//!
//! [`TimeWindow`] maintains a buffer of timestamped values and provides
//! efficient computation of common aggregations (mean, std, sum, min, max, count).

use std::collections::VecDeque;
use std::time::Duration;

/// Time-based sliding window aggregator.
///
/// Maintains a buffer of `(timestamp, value)` pairs and automatically evicts
/// entries older than the window duration. Provides efficient O(1) computation
/// for sum and count, and O(n) for min/max.
///
/// # Examples
///
/// ```rust
/// use tflo_core::primitives::TimeWindow;
/// use std::time::Duration;
///
/// let mut window = TimeWindow::new(Duration::from_secs(5));
///
/// window.push(1000, 10.0);
/// window.push(2000, 20.0);
/// window.push(3000, 30.0);
///
/// assert_eq!(window.count(), 3);
/// assert_eq!(window.sum(), 60.0);
/// assert_eq!(window.mean(), 20.0);
///
/// // Push a value that evicts the first entry
/// window.push(7000, 40.0);  // ts=1000 is now outside the 5s window
/// assert_eq!(window.count(), 3);  // Only 2000, 3000, 7000 remain
/// ```
#[derive(Debug, Clone)]
pub struct TimeWindow {
    window_ms: i64,
    buffer: VecDeque<(i64, f64)>,
    sum: f64,
    sum_sq: f64,
}

impl TimeWindow {
    /// Create a new time window with the specified duration.
    #[must_use]
    pub fn new(window: Duration) -> Self {
        #[allow(clippy::cast_possible_wrap)]
        let window_ms = window.as_millis() as i64;
        Self {
            window_ms,
            buffer: VecDeque::new(),
            sum: 0.0,
            sum_sq: 0.0,
        }
    }

    /// Add a new value at the given timestamp and evict old values.
    ///
    /// Values with timestamps older than `ts - window_duration` are removed.
    pub fn push(&mut self, ts: i64, value: f64) {
        // Add new value
        self.buffer.push_back((ts, value));
        self.sum += value;
        self.sum_sq += value * value;

        // Evict old values
        let cutoff = ts - self.window_ms;
        while let Some(&(old_ts, old_val)) = self.buffer.front() {
            if old_ts < cutoff {
                let _ = self.buffer.pop_front();
                self.sum -= old_val;
                self.sum_sq -= old_val * old_val;
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

    /// Check if the window is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.buffer.is_empty()
    }

    /// Get the sum of all values in the window.
    #[must_use]
    pub fn sum(&self) -> f64 {
        self.sum
    }

    /// Get the arithmetic mean of values in the window.
    ///
    /// Returns `f64::NAN` if the window is empty.
    #[must_use]
    pub fn mean(&self) -> f64 {
        if self.buffer.is_empty() {
            f64::NAN
        } else {
            self.sum / self.buffer.len() as f64
        }
    }

    /// Get the population variance of values in the window.
    ///
    /// Returns `f64::NAN` if the window has fewer than 2 values.
    #[must_use]
    pub fn variance(&self) -> f64 {
        let n = self.buffer.len() as f64;
        if n < 2.0 {
            f64::NAN
        } else {
            let mean = self.sum / n;
            let variance = (self.sum_sq / n) - (mean * mean);
            variance.max(0.0) // Clamp to avoid negative due to floating point errors
        }
    }

    /// Get the population standard deviation of values in the window.
    ///
    /// Returns `f64::NAN` if the window has fewer than 2 values.
    #[must_use]
    pub fn std(&self) -> f64 {
        self.variance().sqrt()
    }

    /// Get the maximum value in the window.
    ///
    /// Returns `f64::NEG_INFINITY` if the window is empty.
    #[must_use]
    pub fn max(&self) -> f64 {
        self.buffer
            .iter()
            .map(|(_, v)| *v)
            .fold(f64::NEG_INFINITY, f64::max)
    }

    /// Get the minimum value in the window.
    ///
    /// Returns `f64::INFINITY` if the window is empty.
    #[must_use]
    pub fn min(&self) -> f64 {
        self.buffer
            .iter()
            .map(|(_, v)| *v)
            .fold(f64::INFINITY, f64::min)
    }

    /// Get the oldest value in the window.
    #[must_use]
    pub fn first(&self) -> Option<f64> {
        self.buffer.front().map(|(_, v)| *v)
    }

    /// Get the newest value in the window.
    #[must_use]
    pub fn last(&self) -> Option<f64> {
        self.buffer.back().map(|(_, v)| *v)
    }

    /// Get the timestamp of the oldest entry.
    #[must_use]
    pub fn oldest_timestamp(&self) -> Option<i64> {
        self.buffer.front().map(|(ts, _)| *ts)
    }

    /// Get the timestamp of the newest entry.
    #[must_use]
    pub fn newest_timestamp(&self) -> Option<i64> {
        self.buffer.back().map(|(ts, _)| *ts)
    }

    /// Clear all values from the window.
    pub fn clear(&mut self) {
        self.buffer.clear();
        self.sum = 0.0;
        self.sum_sq = 0.0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_aggregations() {
        let mut window = TimeWindow::new(Duration::from_secs(10));

        window.push(1000, 10.0);
        window.push(2000, 20.0);
        window.push(3000, 30.0);

        assert_eq!(window.count(), 3);
        assert_eq!(window.sum(), 60.0);
        assert_eq!(window.mean(), 20.0);
        assert_eq!(window.min(), 10.0);
        assert_eq!(window.max(), 30.0);
    }

    #[test]
    fn test_eviction() {
        let mut window = TimeWindow::new(Duration::from_secs(5)); // 5000ms

        window.push(1000, 10.0);
        window.push(2000, 20.0);
        window.push(3000, 30.0);
        assert_eq!(window.count(), 3);

        // Push at ts=7000, evicts ts=1000 (cutoff = 7000-5000 = 2000)
        window.push(7000, 40.0);
        assert_eq!(window.count(), 3); // 2000, 3000, 7000 remain
        assert_eq!(window.sum(), 90.0);
    }

    #[test]
    fn test_empty_window() {
        let window = TimeWindow::new(Duration::from_secs(5));

        assert!(window.is_empty());
        assert!(window.mean().is_nan());
        assert!(window.variance().is_nan());
        assert_eq!(window.max(), f64::NEG_INFINITY);
        assert_eq!(window.min(), f64::INFINITY);
    }

    #[test]
    fn test_single_value_variance() {
        let mut window = TimeWindow::new(Duration::from_secs(5));
        window.push(1000, 10.0);

        assert!(window.variance().is_nan()); // Need at least 2 values
    }

    #[test]
    fn test_std_calculation() {
        let mut window = TimeWindow::new(Duration::from_secs(10));

        // Values: 2, 4, 4, 4, 5, 5, 7, 9
        // Mean = 5, Variance = 4, Std = 2
        for (i, v) in [2.0, 4.0, 4.0, 4.0, 5.0, 5.0, 7.0, 9.0].iter().enumerate() {
            #[allow(clippy::cast_possible_wrap)]
            window.push((i as i64) * 100, *v);
        }

        let std = window.std();
        assert!((std - 2.0).abs() < 0.0001);
    }
}
