//! Median and quantile window primitives.
//!
//! Provides rolling median and quantile calculations using a sorted buffer approach.
//! For count-based windows, uses insertion sort for O(n) per update.
//! For time-based windows, uses a VecDeque with sorting on access.

use crate::operator::WindowPrimitive;
use std::collections::VecDeque;
use std::time::Duration;

/// Count-based sliding window for median and quantile calculations.
///
/// Maintains a sorted buffer of the last N values for efficient
/// median and arbitrary quantile calculations.
///
/// # Examples
///
/// ```rust
/// use tflo_core::primitives::MedianCountWindow;
///
/// let mut window = MedianCountWindow::new(5);
///
/// window.push(3.0);
/// window.push(1.0);
/// window.push(4.0);
/// window.push(1.0);
/// window.push(5.0);
///
/// assert_eq!(window.median(), 3.0);  // Middle value of [1, 1, 3, 4, 5]
/// assert_eq!(window.quantile(0.25), 1.0);  // 25th percentile
/// ```
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MedianCountWindow {
    max_count: usize,
    /// Values in insertion order for FIFO eviction
    buffer: VecDeque<f64>,
    /// Values in sorted order for median/quantile calculation
    sorted: Vec<f64>,
}

impl MedianCountWindow {
    /// Create a new count-based median window with the specified capacity.
    #[must_use]
    pub fn new(max_count: usize) -> Self {
        Self {
            max_count,
            buffer: VecDeque::with_capacity(max_count),
            sorted: Vec::with_capacity(max_count),
        }
    }

    /// Add a new value, evicting the oldest if at capacity.
    pub fn push(&mut self, value: f64) {
        // Evict oldest if at capacity
        if self.buffer.len() >= self.max_count {
            if let Some(old) = self.buffer.pop_front() {
                // Remove from sorted (binary search to find position)
                if let Ok(pos) = self.sorted.binary_search_by(|a| a.total_cmp(&old)) {
                    let _ = self.sorted.remove(pos);
                }
            }
        }

        // Add new value to buffer
        self.buffer.push_back(value);

        // Insert into sorted position
        let pos = self
            .sorted
            .binary_search_by(|a| a.total_cmp(&value))
            .unwrap_or_else(|p| p);
        self.sorted.insert(pos, value);
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

    /// Get the median value.
    ///
    /// Returns `f64::NAN` if the window is empty.
    #[must_use]
    pub fn median(&self) -> f64 {
        self.quantile(0.5)
    }

    /// Get the value at a given quantile (0.0 to 1.0).
    ///
    /// Uses linear interpolation for positions between elements.
    /// Returns `f64::NAN` if the window is empty.
    #[must_use]
    pub fn quantile(&self, q: f64) -> f64 {
        if self.sorted.is_empty() {
            return f64::NAN;
        }

        let q = q.clamp(0.0, 1.0);
        let n = self.sorted.len();

        if n == 1 {
            return self.sorted[0];
        }

        // Linear interpolation method (same as pandas default)
        let pos = q * (n - 1) as f64;
        let lower = pos.floor() as usize;
        let upper = pos.ceil() as usize;
        let frac = pos - lower as f64;

        if lower == upper {
            self.sorted[lower]
        } else {
            self.sorted[lower] * (1.0 - frac) + self.sorted[upper] * frac
        }
    }

    /// Get the percentile (0 to 100).
    ///
    /// Returns `f64::NAN` if the window is empty.
    #[must_use]
    pub fn percentile(&self, p: f64) -> f64 {
        self.quantile(p / 100.0)
    }

    /// Get the interquartile range (Q3 - Q1).
    #[must_use]
    pub fn iqr(&self) -> f64 {
        self.quantile(0.75) - self.quantile(0.25)
    }

    /// Get the rank of the most recent value within the window (0.0 to 1.0).
    ///
    /// A rank of 0.0 means it's the smallest, 1.0 means it's the largest.
    #[must_use]
    pub fn current_rank(&self) -> f64 {
        if self.sorted.is_empty() {
            return f64::NAN;
        }

        if let Some(&current) = self.buffer.back() {
            // Count how many values are less than current
            let less_than = self.sorted.iter().filter(|&&v| v < current).count();
            less_than as f64 / (self.sorted.len() - 1).max(1) as f64
        } else {
            f64::NAN
        }
    }

    /// Clear all values from the window.
    pub fn clear(&mut self) {
        self.buffer.clear();
        self.sorted.clear();
    }
}

/// Time-based sliding window for median and quantile calculations.
///
/// Maintains a buffer of timestamped values and computes median/quantile
/// on demand by sorting the values in the window.
///
/// # Performance
///
/// - Push: O(1)
/// - Median/Quantile: O(n log n) for sorting
///
/// For frequently accessed medians, consider using the count-based window
/// if your data arrives at regular intervals.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MedianTimeWindow {
    window_ms: i64,
    buffer: VecDeque<(i64, f64)>,
}

impl MedianTimeWindow {
    /// Create a new time-based median window with the specified duration.
    #[must_use]
    pub fn new(window: Duration) -> Self {
        #[allow(clippy::cast_possible_wrap)]
        let window_ms = window.as_millis() as i64;
        Self {
            window_ms,
            buffer: VecDeque::new(),
        }
    }

    /// Add a new value at the given timestamp and evict old values.
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

    /// Check if the window is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.buffer.is_empty()
    }

    /// Get the median value.
    ///
    /// Returns `f64::NAN` if the window is empty.
    #[must_use]
    pub fn median(&self) -> f64 {
        self.quantile(0.5)
    }

    /// Get the value at a given quantile (0.0 to 1.0).
    ///
    /// Uses linear interpolation for positions between elements.
    /// Returns `f64::NAN` if the window is empty.
    #[must_use]
    pub fn quantile(&self, q: f64) -> f64 {
        if self.buffer.is_empty() {
            return f64::NAN;
        }

        let q = q.clamp(0.0, 1.0);

        // Collect and sort values
        let mut values: Vec<f64> = self.buffer.iter().map(|(_, v)| *v).collect();
        values.sort_by(|a, b| a.total_cmp(b));

        let n = values.len();
        if n == 1 {
            return values[0];
        }

        // Linear interpolation method
        let pos = q * (n - 1) as f64;
        let lower = pos.floor() as usize;
        let upper = pos.ceil() as usize;
        let frac = pos - lower as f64;

        if lower == upper {
            values[lower]
        } else {
            values[lower] * (1.0 - frac) + values[upper] * frac
        }
    }

    /// Get the percentile (0 to 100).
    #[must_use]
    pub fn percentile(&self, p: f64) -> f64 {
        self.quantile(p / 100.0)
    }

    /// Get the interquartile range (Q3 - Q1).
    #[must_use]
    pub fn iqr(&self) -> f64 {
        self.quantile(0.75) - self.quantile(0.25)
    }

    /// Get the rank of the most recent value within the window (0.0 to 1.0).
    #[must_use]
    pub fn current_rank(&self) -> f64 {
        if self.buffer.is_empty() {
            return f64::NAN;
        }

        if let Some(&(_, current)) = self.buffer.back() {
            let less_than = self.buffer.iter().filter(|(_, v)| *v < current).count();
            less_than as f64 / (self.buffer.len() - 1).max(1) as f64
        } else {
            f64::NAN
        }
    }

    /// Clear all values from the window.
    pub fn clear(&mut self) {
        self.buffer.clear();
    }
}

impl WindowPrimitive for MedianCountWindow {
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

impl WindowPrimitive for MedianTimeWindow {
    fn push(&mut self, ts: i64, value: f64) {
        self.push(ts, value);
    }

    fn len(&self) -> usize {
        self.count()
    }

    fn is_empty(&self) -> bool {
        self.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_median_count_window_basic() {
        let mut window = MedianCountWindow::new(5);

        window.push(3.0);
        window.push(1.0);
        window.push(4.0);
        window.push(1.0);
        window.push(5.0);

        // Sorted: [1, 1, 3, 4, 5]
        assert_eq!(window.median(), 3.0);
        assert_eq!(window.count(), 5);
    }

    #[test]
    fn test_median_count_window_even() {
        let mut window = MedianCountWindow::new(4);

        window.push(1.0);
        window.push(2.0);
        window.push(3.0);
        window.push(4.0);

        // Sorted: [1, 2, 3, 4], median should be 2.5 (interpolated)
        assert_eq!(window.median(), 2.5);
    }

    #[test]
    fn test_median_count_window_eviction() {
        let mut window = MedianCountWindow::new(3);

        window.push(10.0);
        window.push(20.0);
        window.push(30.0);
        window.push(40.0); // Evicts 10.0

        // Sorted: [20, 30, 40]
        assert_eq!(window.median(), 30.0);
        assert_eq!(window.count(), 3);
    }

    #[test]
    fn test_quantile() {
        let mut window = MedianCountWindow::new(10);

        for i in 1..=10 {
            window.push(i as f64);
        }

        // Values: [1, 2, 3, 4, 5, 6, 7, 8, 9, 10]
        assert!((window.quantile(0.0) - 1.0).abs() < 0.001);
        assert!((window.quantile(0.5) - 5.5).abs() < 0.001);
        assert!((window.quantile(1.0) - 10.0).abs() < 0.001);
    }

    #[test]
    fn test_percentile() {
        let mut window = MedianCountWindow::new(100);

        for i in 1..=100 {
            window.push(i as f64);
        }

        assert!((window.percentile(25.0) - 25.75).abs() < 0.001);
        assert!((window.percentile(75.0) - 75.25).abs() < 0.001);
    }

    #[test]
    fn test_rank() {
        let mut window = MedianCountWindow::new(5);

        window.push(1.0);
        window.push(2.0);
        window.push(3.0);
        window.push(4.0);
        window.push(5.0);

        // Current value is 5.0, which is the largest
        assert!((window.current_rank() - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_median_time_window() {
        let mut window = MedianTimeWindow::new(Duration::from_secs(5));

        window.push(1000, 3.0);
        window.push(2000, 1.0);
        window.push(3000, 4.0);
        window.push(4000, 1.0);
        window.push(5000, 5.0);

        assert_eq!(window.median(), 3.0);

        // Push value that evicts first entry
        window.push(7000, 9.0);
        // Values now: [1, 4, 1, 5, 9] -> sorted: [1, 1, 4, 5, 9]
        assert_eq!(window.median(), 4.0);
    }

    #[test]
    fn test_empty_window() {
        let count_window = MedianCountWindow::new(5);
        let time_window = MedianTimeWindow::new(Duration::from_secs(5));

        assert!(count_window.median().is_nan());
        assert!(time_window.median().is_nan());
        assert!(count_window.quantile(0.5).is_nan());
    }
}
