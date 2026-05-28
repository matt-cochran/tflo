//! Previous value tracker partitioned by key.
//!
//! [`PrevByTracker`] maintains separate previous values for each unique key,
//! useful for tracking previous values per symbol, sensor, or category.

use std::collections::HashMap;
use std::hash::Hash;

/// Tracks previous values partitioned by a key.
///
/// Maintains a separate previous value for each unique key, useful when
/// processing interleaved streams of different entities.
///
/// # Examples
///
/// ```rust
/// use tflo_ops::primitives::PrevByTracker;
///
/// let mut tracker = PrevByTracker::new();
///
/// // Track previous prices per symbol
/// assert_eq!(tracker.update("AAPL", 150.0), None);
/// assert_eq!(tracker.update("GOOG", 2800.0), None);
/// assert_eq!(tracker.update("AAPL", 151.0), Some(150.0));  // Previous AAPL
/// assert_eq!(tracker.update("GOOG", 2810.0), Some(2800.0)); // Previous GOOG
/// ```
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(bound(
    serialize = "K: serde::Serialize + std::hash::Hash + Eq",
    deserialize = "K: serde::Deserialize<'de> + std::hash::Hash + Eq"
))]
pub struct PrevByTracker<K> {
    prev: HashMap<K, f64>,
    max_keys: Option<usize>,
}

impl<K: Hash + Eq + Clone> Default for PrevByTracker<K> {
    fn default() -> Self {
        Self::new()
    }
}

impl<K: Hash + Eq + Clone> PrevByTracker<K> {
    /// Create a new partitioned previous value tracker.
    #[must_use]
    pub fn new() -> Self {
        Self {
            prev: HashMap::new(),
            max_keys: None,
        }
    }

    /// Create a new tracker with a maximum number of tracked keys.
    ///
    /// When the limit is exceeded, the oldest keys may be evicted
    /// (note: `HashMap` doesn't preserve order, so eviction is arbitrary).
    #[must_use]
    pub fn with_max_keys(max_keys: usize) -> Self {
        Self {
            prev: HashMap::new(),
            max_keys: Some(max_keys),
        }
    }

    /// Update with a new value for the given key and return the previous value.
    pub fn update(&mut self, key: K, value: f64) -> Option<f64> {
        // Check max keys limit
        if let Some(max) = self.max_keys {
            if !self.prev.contains_key(&key) && self.prev.len() >= max {
                // Evict an arbitrary key (HashMap has no order)
                if let Some(k) = self.prev.keys().next().cloned() {
                    let _ = self.prev.remove(&k);
                }
            }
        }

        self.prev.insert(key, value)
    }

    /// Get the previous value for a key without updating.
    #[must_use]
    pub fn get(&self, key: &K) -> Option<f64> {
        self.prev.get(key).copied()
    }

    /// Get the delta for a key (current - previous) if previous exists.
    pub fn delta(&mut self, key: K, current: f64) -> Option<f64> {
        let prev = self.prev.insert(key, current);
        prev.map(|p| current - p)
    }

    /// Get the number of tracked keys.
    #[must_use]
    pub fn key_count(&self) -> usize {
        self.prev.len()
    }

    /// Check if a key is being tracked.
    #[must_use]
    pub fn contains_key(&self, key: &K) -> bool {
        self.prev.contains_key(key)
    }

    /// Clear all tracked keys.
    pub fn clear(&mut self) {
        self.prev.clear();
    }

    /// Remove tracking for a specific key.
    pub fn remove(&mut self, key: &K) -> Option<f64> {
        self.prev.remove(key)
    }
}

/// Tracks previous timestamped values partitioned by key.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(bound(
    serialize = "K: serde::Serialize + std::hash::Hash + Eq",
    deserialize = "K: serde::Deserialize<'de> + std::hash::Hash + Eq"
))]
#[allow(dead_code)]
pub struct TimestampedPrevByTracker<K> {
    prev: HashMap<K, (i64, f64)>,
    max_keys: Option<usize>,
}

impl<K: Hash + Eq + Clone> Default for TimestampedPrevByTracker<K> {
    fn default() -> Self {
        Self::new()
    }
}

impl<K: Hash + Eq + Clone> TimestampedPrevByTracker<K> {
    /// Create a new partitioned timestamped previous value tracker.
    #[must_use]
    pub fn new() -> Self {
        Self {
            prev: HashMap::new(),
            max_keys: None,
        }
    }

    /// Create a new tracker with a maximum number of tracked keys.
    #[must_use]
    pub fn with_max_keys(max_keys: usize) -> Self {
        Self {
            prev: HashMap::new(),
            max_keys: Some(max_keys),
        }
    }

    /// Update with a new timestamped value and return the previous.
    pub fn update(&mut self, key: K, ts: i64, value: f64) -> Option<(i64, f64)> {
        if let Some(max) = self.max_keys {
            if !self.prev.contains_key(&key) && self.prev.len() >= max {
                if let Some(k) = self.prev.keys().next().cloned() {
                    let _ = self.prev.remove(&k);
                }
            }
        }

        self.prev.insert(key, (ts, value))
    }

    /// Get the previous value for a key without updating.
    #[must_use]
    pub fn get(&self, key: &K) -> Option<(i64, f64)> {
        self.prev.get(key).copied()
    }

    /// Calculate the rate of change for a key.
    pub fn rate(&mut self, key: K, ts: i64, value: f64) -> Option<f64> {
        let result = self.prev.get(&key).and_then(|(prev_ts, prev_val)| {
            // SAFETY: `prev_ts` was captured from a prior `ts` for the same
            // key; under monotonic timestamps `ts >= *prev_ts`.
            // `saturating_sub` collapses an out-of-order sample to `dt == 0`,
            // which the `dt > 0.0` guard below treats as "no rate".
            let dt = ts.saturating_sub(*prev_ts) as f64;
            if dt > 0.0 {
                Some((value - *prev_val) / dt)
            } else {
                None
            }
        });
        let _ = self.prev.insert(key, (ts, value));
        result
    }

    /// Clear all tracked keys.
    pub fn clear(&mut self) {
        self.prev.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_prev_by_tracker() {
        let mut tracker: PrevByTracker<&str> = PrevByTracker::new();

        assert_eq!(tracker.update("A", 10.0), None);
        assert_eq!(tracker.update("B", 20.0), None);
        assert_eq!(tracker.update("A", 15.0), Some(10.0));
        assert_eq!(tracker.update("B", 25.0), Some(20.0));
    }

    #[test]
    fn test_delta_by_key() {
        let mut tracker: PrevByTracker<&str> = PrevByTracker::new();

        assert_eq!(tracker.delta("A", 100.0), None);
        assert_eq!(tracker.delta("A", 150.0), Some(50.0));
        assert_eq!(tracker.delta("B", 200.0), None);
        assert_eq!(tracker.delta("B", 180.0), Some(-20.0));
    }

    #[test]
    fn test_max_keys() {
        let mut tracker: PrevByTracker<i32> = PrevByTracker::with_max_keys(2);

        let _ = tracker.update(1, 10.0);
        let _ = tracker.update(2, 20.0);
        assert_eq!(tracker.key_count(), 2);

        let _ = tracker.update(3, 30.0); // Should evict one
        assert_eq!(tracker.key_count(), 2);
    }

    #[test]
    fn test_timestamped_rate() {
        let mut tracker: TimestampedPrevByTracker<&str> = TimestampedPrevByTracker::new();

        assert_eq!(tracker.rate("A", 1000, 100.0), None);
        // Rate = (200 - 100) / (2000 - 1000) = 0.1
        assert_eq!(tracker.rate("A", 2000, 200.0), Some(0.1));
    }
}
