//! Count-based sliding window aggregator.
//!
//! [`CountWindow`] maintains a fixed-size buffer of the most recent values.

use std::collections::VecDeque;
use tflo_core::operator::WindowPrimitive;

/// Count-based sliding window aggregator.
///
/// Maintains a buffer of the last N values and provides efficient computation
/// of common aggregations (mean, std, sum, min, max).
///
/// # Examples
///
/// ```rust
/// use tflo_ops::primitives::CountWindow;
///
/// let mut window = CountWindow::new(3);
///
/// window.push(10.0);
/// window.push(20.0);
/// window.push(30.0);
/// assert_eq!(window.mean(), 20.0);
///
/// window.push(40.0);  // Evicts 10.0
/// assert_eq!(window.count(), 3);
/// assert_eq!(window.mean(), 30.0);  // (20 + 30 + 40) / 3
/// ```
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CountWindow {
    max_count: usize,
    buffer: VecDeque<f64>,
    sum: f64,
    sum_sq: f64,
}

impl CountWindow {
    /// Create a new count-based window with the specified capacity.
    #[must_use]
    pub fn new(max_count: usize) -> Self {
        Self {
            max_count,
            buffer: VecDeque::with_capacity(max_count),
            sum: 0.0,
            sum_sq: 0.0,
        }
    }

    /// Add a new value, evicting the oldest if at capacity.
    pub fn push(&mut self, value: f64) {
        // Evict oldest if at capacity
        if self.buffer.len() >= self.max_count {
            if let Some(old) = self.buffer.pop_front() {
                self.sum -= old;
                self.sum_sq -= old * old;
            }
        }

        // Add new value
        self.buffer.push_back(value);
        self.sum += value;
        self.sum_sq += value * value;
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

    /// Check if the window is at full capacity.
    #[must_use]
    pub fn is_full(&self) -> bool {
        self.buffer.len() >= self.max_count
    }

    /// Get the maximum capacity of the window.
    #[must_use]
    pub const fn capacity(&self) -> usize {
        self.max_count
    }

    /// Get the sum of all values in the window.
    #[must_use]
    pub const fn sum(&self) -> f64 {
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
            variance.max(0.0)
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
            .copied()
            .fold(f64::NEG_INFINITY, f64::max)
    }

    /// Get the minimum value in the window.
    ///
    /// Returns `f64::INFINITY` if the window is empty.
    #[must_use]
    pub fn min(&self) -> f64 {
        self.buffer.iter().copied().fold(f64::INFINITY, f64::min)
    }

    /// Get the oldest value in the window.
    #[must_use]
    pub fn first(&self) -> Option<f64> {
        self.buffer.front().copied()
    }

    /// Get the newest value in the window.
    #[must_use]
    pub fn last(&self) -> Option<f64> {
        self.buffer.back().copied()
    }

    /// Clear all values from the window.
    pub fn clear(&mut self) {
        self.buffer.clear();
        self.sum = 0.0;
        self.sum_sq = 0.0;
    }
}

impl WindowPrimitive for CountWindow {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_operations() {
        let mut window = CountWindow::new(3);

        window.push(10.0);
        window.push(20.0);
        window.push(30.0);

        assert_eq!(window.count(), 3);
        assert!(window.is_full());
        assert_eq!(window.sum(), 60.0);
        assert_eq!(window.mean(), 20.0);
    }

    #[test]
    fn test_eviction() {
        let mut window = CountWindow::new(3);

        window.push(10.0);
        window.push(20.0);
        window.push(30.0);
        window.push(40.0);

        assert_eq!(window.count(), 3);
        assert_eq!(window.sum(), 90.0); // 20 + 30 + 40
        assert_eq!(window.first(), Some(20.0));
        assert_eq!(window.last(), Some(40.0));
    }

    #[test]
    fn test_empty_window() {
        let window = CountWindow::new(5);

        assert!(window.is_empty());
        assert!(!window.is_full());
        assert!(window.mean().is_nan());
    }
}
