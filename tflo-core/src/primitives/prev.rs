//! Previous value tracker.
//!
//! [`PrevTracker`] maintains the previous value for computing deltas
//! and detecting changes.

/// Tracks the previous value in a stream.
///
/// # Examples
///
/// ```rust
/// use tflo_core::primitives::PrevTracker;
///
/// let mut tracker = PrevTracker::new();
///
/// assert_eq!(tracker.update(10.0), None);
/// assert_eq!(tracker.update(20.0), Some(10.0));
/// assert_eq!(tracker.update(30.0), Some(20.0));
/// ```
#[derive(Debug, Clone, Default)]
pub struct PrevTracker {
    prev: Option<f64>,
}

impl PrevTracker {
    /// Create a new previous value tracker.
    #[must_use]
    pub fn new() -> Self {
        Self { prev: None }
    }

    /// Update with a new value and return the previous value.
    ///
    /// Returns `None` on the first call, then returns the previous value
    /// on subsequent calls.
    pub fn update(&mut self, value: f64) -> Option<f64> {
        let prev = self.prev;
        self.prev = Some(value);
        prev
    }

    /// Get the current previous value without updating.
    #[must_use]
    pub fn get(&self) -> Option<f64> {
        self.prev
    }

    /// Get the delta (current - previous) if previous exists.
    pub fn delta(&mut self, current: f64) -> Option<f64> {
        let result = self.prev.map(|p| current - p);
        self.prev = Some(current);
        result
    }

    /// Check if there's a previous value.
    #[must_use]
    pub fn has_prev(&self) -> bool {
        self.prev.is_some()
    }

    /// Reset the tracker.
    pub fn reset(&mut self) {
        self.prev = None;
    }
}

/// Tracks previous values with timestamps.
///
/// Useful when you need to know both the previous value and when it occurred.
#[derive(Debug, Clone, Default)]
#[allow(dead_code)]
pub struct TimestampedPrevTracker {
    prev: Option<(i64, f64)>,
}

impl TimestampedPrevTracker {
    /// Create a new timestamped previous value tracker.
    #[must_use]
    pub fn new() -> Self {
        Self { prev: None }
    }

    /// Update with a new timestamped value and return the previous.
    pub fn update(&mut self, ts: i64, value: f64) -> Option<(i64, f64)> {
        let prev = self.prev;
        self.prev = Some((ts, value));
        prev
    }

    /// Get the current previous value and timestamp without updating.
    #[must_use]
    pub fn get(&self) -> Option<(i64, f64)> {
        self.prev
    }

    /// Get just the previous value.
    #[must_use]
    pub fn prev_value(&self) -> Option<f64> {
        self.prev.map(|(_, v)| v)
    }

    /// Get just the previous timestamp.
    #[must_use]
    pub fn prev_timestamp(&self) -> Option<i64> {
        self.prev.map(|(ts, _)| ts)
    }

    /// Calculate the rate of change (value change per time unit).
    ///
    /// Returns `None` if there's no previous value or if no time has elapsed.
    pub fn rate(&mut self, ts: i64, value: f64) -> Option<f64> {
        let result = self.prev.and_then(|(prev_ts, prev_val)| {
            let dt = (ts - prev_ts) as f64;
            if dt > 0.0 {
                Some((value - prev_val) / dt)
            } else {
                None
            }
        });
        self.prev = Some((ts, value));
        result
    }

    /// Reset the tracker.
    pub fn reset(&mut self) {
        self.prev = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_prev_tracker() {
        let mut tracker = PrevTracker::new();

        assert!(!tracker.has_prev());
        assert_eq!(tracker.update(10.0), None);
        assert!(tracker.has_prev());
        assert_eq!(tracker.update(20.0), Some(10.0));
        assert_eq!(tracker.get(), Some(20.0));
    }

    #[test]
    fn test_delta() {
        let mut tracker = PrevTracker::new();

        assert_eq!(tracker.delta(100.0), None);
        assert_eq!(tracker.delta(150.0), Some(50.0));
        assert_eq!(tracker.delta(130.0), Some(-20.0));
    }

    #[test]
    fn test_timestamped_tracker() {
        let mut tracker = TimestampedPrevTracker::new();

        assert_eq!(tracker.update(1000, 100.0), None);
        assert_eq!(tracker.update(2000, 200.0), Some((1000, 100.0)));
        assert_eq!(tracker.prev_value(), Some(200.0));
        assert_eq!(tracker.prev_timestamp(), Some(2000));
    }

    #[test]
    fn test_rate() {
        let mut tracker = TimestampedPrevTracker::new();

        assert_eq!(tracker.rate(1000, 100.0), None);
        // Rate = (200 - 100) / (2000 - 1000) = 0.1
        assert_eq!(tracker.rate(2000, 200.0), Some(0.1));
    }
}
