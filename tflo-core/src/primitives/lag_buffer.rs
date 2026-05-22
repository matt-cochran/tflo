//! Lookback buffer for retrieving values from the past.
//!
//! [`LagBuffer`] maintains a history of timestamped values and allows
//! retrieval of values from a specified time in the past.

use std::collections::VecDeque;
use std::time::Duration;

/// Lookback buffer for time-based lag operations.
///
/// Stores a history of timestamped values and provides access to values
/// from a specified duration in the past.
///
/// # Examples
///
/// ```rust
/// use tflo_core::primitives::LagBuffer;
/// use std::time::Duration;
///
/// let mut buffer = LagBuffer::new(Duration::from_secs(5));
///
/// buffer.push(1000, 100.0);
/// buffer.push(2000, 110.0);
/// buffer.push(3000, 120.0);
///
/// // At ts=6000, look back 5 seconds to find value near ts=1000
/// let lagged = buffer.push(6000, 130.0);
/// assert_eq!(lagged, Some(100.0));
/// ```
#[derive(Debug, Clone)]
pub struct LagBuffer {
    lag_ms: i64,
    buffer: VecDeque<(i64, f64)>,
    max_buffer_size: usize,
}

impl LagBuffer {
    /// Create a new lag buffer with the specified lookback duration.
    #[must_use]
    pub fn new(lag: Duration) -> Self {
        Self::with_max_size(lag, 10_000)
    }

    /// Create a new lag buffer with a maximum buffer size.
    ///
    /// The buffer will not grow beyond this size, which helps prevent
    /// memory exhaustion with high-frequency data.
    #[must_use]
    pub fn with_max_size(lag: Duration, max_size: usize) -> Self {
        #[allow(clippy::cast_possible_wrap)]
        let lag_ms = lag.as_millis() as i64;
        Self {
            lag_ms,
            buffer: VecDeque::new(),
            max_buffer_size: max_size,
        }
    }

    /// Add a new value and return the lagged value (if available).
    ///
    /// Returns `Some(value)` if there's a value from approximately `lag` time ago,
    /// or `None` if insufficient history.
    pub fn push(&mut self, ts: i64, value: f64) -> Option<f64> {
        self.buffer.push_back((ts, value));

        // Enforce max buffer size
        while self.buffer.len() > self.max_buffer_size {
            let _ = self.buffer.pop_front();
        }

        let target_ts = ts - self.lag_ms;

        // Clean up values we definitely don't need anymore
        // Keep values that might be useful for interpolation
        while self.buffer.len() > 1 {
            if let Some(&(next_ts, _)) = self.buffer.get(1) {
                if next_ts <= target_ts {
                    let _ = self.buffer.pop_front();
                } else {
                    break;
                }
            } else {
                break;
            }
        }

        // Find value at target time (use closest preceding value)
        self.buffer
            .front()
            .filter(|(first_ts, _)| *first_ts <= target_ts)
            .map(|(_, v)| *v)
    }

    /// Get the lagged value for a given timestamp without adding a new value.
    #[must_use]
    pub fn get_at(&self, ts: i64) -> Option<f64> {
        let target_ts = ts - self.lag_ms;

        // Find the value at or before target_ts
        let mut result = None;
        for &(entry_ts, value) in &self.buffer {
            if entry_ts <= target_ts {
                result = Some(value);
            } else {
                break;
            }
        }
        result
    }

    /// Get the delta (current - lagged) for a given timestamp.
    ///
    /// Returns `None` if there's no lagged value available.
    #[must_use]
    pub fn delta_at(&self, ts: i64, current: f64) -> Option<f64> {
        self.get_at(ts).map(|lagged| current - lagged)
    }

    /// Get the number of entries in the buffer.
    #[must_use]
    pub fn len(&self) -> usize {
        self.buffer.len()
    }

    /// Check if the buffer is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.buffer.is_empty()
    }

    /// Clear all entries from the buffer.
    pub fn clear(&mut self) {
        self.buffer.clear();
    }

    /// Get the configured lag duration in milliseconds.
    #[must_use]
    pub fn lag_ms(&self) -> i64 {
        self.lag_ms
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_lag() {
        let mut buffer = LagBuffer::new(Duration::from_secs(5)); // 5000ms

        assert_eq!(buffer.push(1000, 100.0), None);
        assert_eq!(buffer.push(2000, 110.0), None);
        assert_eq!(buffer.push(3000, 120.0), None);

        // At ts=6000, target = 1000, should find 100.0
        assert_eq!(buffer.push(6000, 130.0), Some(100.0));

        // At ts=7000, target = 2000, should find 110.0
        assert_eq!(buffer.push(7000, 140.0), Some(110.0));
    }

    #[test]
    fn test_insufficient_history() {
        let mut buffer = LagBuffer::new(Duration::from_secs(10));

        let _ = buffer.push(1000, 100.0);
        let _ = buffer.push(2000, 110.0);

        // At ts=5000, target = -5000, no value available
        assert_eq!(buffer.push(5000, 120.0), None);
    }

    #[test]
    fn test_delta() {
        let mut buffer = LagBuffer::new(Duration::from_secs(5));

        let _ = buffer.push(1000, 100.0);
        let _ = buffer.push(6000, 150.0);

        // Delta at 6000: 150 - 100 = 50
        assert_eq!(buffer.delta_at(6000, 150.0), Some(50.0));
    }

    #[test]
    fn test_max_buffer_size() {
        let mut buffer = LagBuffer::with_max_size(Duration::from_secs(100), 3);

        let _ = buffer.push(1000, 100.0);
        let _ = buffer.push(2000, 110.0);
        let _ = buffer.push(3000, 120.0);
        let _ = buffer.push(4000, 130.0);

        assert_eq!(buffer.len(), 3);
    }
}
