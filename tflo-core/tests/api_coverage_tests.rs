//! API coverage tests for prev trackers.
//!
//! These tests ensure all public APIs are exercised and follow EARS format:
//! When <Event>, <Component> shall <Action>, So that <Response>,
//! And <Component> will be in a state where <State>.

use tflo_core::primitives::{PrevByTracker, PrevTracker, TimestampedPrevByTracker, TimestampedPrevTracker};

// ============================================================================
// TIMESTAMPED PREV TRACKER TESTS
// ============================================================================

/// When TimestampedPrevTracker receives a value,
/// the tracker shall store timestamp and value together,
/// So that users can access both when needed,
/// And the tracker will contain the previous observation.
#[test]
fn test_timestamped_prev_tracker_update() {
    let mut tracker = TimestampedPrevTracker::new();

    // First update - no previous
    assert_eq!(tracker.update(1000, 100.0), None);

    // Second update - returns previous with timestamp
    let prev = tracker.update(2000, 150.0);
    assert_eq!(prev, Some((1000, 100.0)));
}

/// When get is called on TimestampedPrevTracker,
/// the tracker shall return the most recent stored value without modifying state,
/// So that users can peek at the stored value,
/// And the tracker state will remain unchanged.
#[test]
fn test_timestamped_prev_tracker_get() {
    let mut tracker = TimestampedPrevTracker::new();
    let _ = tracker.update(1000, 100.0);
    let _ = tracker.update(2000, 150.0);

    // Get returns the most recently stored value (last one passed to update)
    assert_eq!(tracker.get(), Some((2000, 150.0)));

    // State unchanged - get returns same value
    assert_eq!(tracker.get(), Some((2000, 150.0)));
}

/// When prev_value is called,
/// TimestampedPrevTracker shall return just the stored value without timestamp,
/// So that users can easily access just the stored value,
/// And the full (ts, value) pair remains stored.
#[test]
fn test_timestamped_prev_value() {
    let mut tracker = TimestampedPrevTracker::new();
    let _ = tracker.update(1000, 100.0);
    let _ = tracker.update(2000, 150.0);

    // Returns the most recently stored value
    assert_eq!(tracker.prev_value(), Some(150.0));
}

/// When prev_timestamp is called,
/// TimestampedPrevTracker shall return just the stored timestamp,
/// So that users can access timing information separately,
/// And the full pair remains stored.
#[test]
fn test_timestamped_prev_timestamp() {
    let mut tracker = TimestampedPrevTracker::new();
    let _ = tracker.update(1000, 100.0);
    let _ = tracker.update(2000, 150.0);

    // Returns the most recently stored timestamp
    assert_eq!(tracker.prev_timestamp(), Some(2000));
}

/// When rate is called with timestamped values,
/// TimestampedPrevTracker shall compute rate of change per time unit,
/// So that users can calculate velocity/acceleration,
/// And the rate will be (new_value - prev_value) / (new_ts - prev_ts).
#[test]
fn test_timestamped_prev_rate() {
    let mut tracker = TimestampedPrevTracker::new();

    // First call - no previous, returns None
    assert_eq!(tracker.rate(1000, 100.0), None);

    // Second call - rate = (200 - 100) / (2000 - 1000) = 0.1
    assert_eq!(tracker.rate(2000, 200.0), Some(0.1));
}

/// When reset is called on TimestampedPrevTracker,
/// the tracker shall clear all stored state,
/// So that subsequent operations start fresh,
/// And the tracker will behave as if newly created.
#[test]
fn test_timestamped_prev_reset() {
    let mut tracker = TimestampedPrevTracker::new();
    let _ = tracker.update(1000, 100.0);

    tracker.reset();

    assert_eq!(tracker.get(), None);
}

// ============================================================================
// TIMESTAMPED PREV BY TRACKER TESTS
// ============================================================================

/// When TimestampedPrevByTracker receives values with different keys,
/// the tracker shall maintain separate previous values per key,
/// So that each key's history is independent,
/// And cross-key values will not interfere.
#[test]
fn test_timestamped_prev_by_tracker() {
    let mut tracker: TimestampedPrevByTracker<&str> = TimestampedPrevByTracker::new();

    // First observations for each key
    assert_eq!(tracker.update("A", 1000, 100.0), None);
    assert_eq!(tracker.update("B", 1500, 200.0), None);

    // Second observations return previous for that key
    assert_eq!(tracker.update("A", 2000, 150.0), Some((1000, 100.0)));
    assert_eq!(tracker.update("B", 2500, 250.0), Some((1500, 200.0)));
}

/// When with_max_keys is used,
/// TimestampedPrevByTracker shall limit the number of tracked keys,
/// So that memory usage is bounded,
/// And old keys will be evicted when the limit is reached.
#[test]
fn test_timestamped_prev_by_max_keys() {
    let mut tracker: TimestampedPrevByTracker<i32> = TimestampedPrevByTracker::with_max_keys(2);

    let _ = tracker.update(1, 1000, 100.0);
    let _ = tracker.update(2, 1000, 200.0);
    let _ = tracker.update(3, 1000, 300.0); // Should evict one
}

/// When get is called on TimestampedPrevByTracker,
/// the tracker shall return the stored value for the given key,
/// So that users can peek without modifying state,
/// And the stored value will remain unchanged.
#[test]
fn test_timestamped_prev_by_get() {
    let mut tracker: TimestampedPrevByTracker<&str> = TimestampedPrevByTracker::new();
    let _ = tracker.update("A", 1000, 100.0);
    let _ = tracker.update("A", 2000, 150.0);

    // Returns the most recently stored value for key "A"
    assert_eq!(tracker.get(&"A"), Some((2000, 150.0)));
}

/// When rate is called with a key,
/// TimestampedPrevByTracker shall compute rate of change for that key,
/// So that per-key velocity can be calculated,
/// And the rate will be independent of other keys.
#[test]
fn test_timestamped_prev_by_rate() {
    let mut tracker: TimestampedPrevByTracker<&str> = TimestampedPrevByTracker::new();

    assert_eq!(tracker.rate("A", 1000, 100.0), None);
    // Rate = (200 - 100) / (2000 - 1000) = 0.1
    assert_eq!(tracker.rate("A", 2000, 200.0), Some(0.1));
}

/// When clear is called,
/// TimestampedPrevByTracker shall remove all stored keys,
/// So that memory is freed,
/// And the tracker will be empty.
#[test]
fn test_timestamped_prev_by_clear() {
    let mut tracker: TimestampedPrevByTracker<&str> = TimestampedPrevByTracker::new();
    let _ = tracker.update("A", 1000, 100.0);
    let _ = tracker.update("B", 1000, 200.0);

    tracker.clear();

    assert_eq!(tracker.get(&"A"), None);
    assert_eq!(tracker.get(&"B"), None);
}

// ============================================================================
// PREV BY TRACKER TESTS
// ============================================================================

/// When PrevByTracker tracks values by key,
/// the tracker shall maintain separate histories,
/// So that per-key analysis is possible,
/// And different entities are handled independently.
#[test]
fn test_prev_by_comprehensive() {
    let mut tracker: PrevByTracker<&str> = PrevByTracker::new();

    // First values - no previous
    assert_eq!(tracker.update("AAPL", 100.0), None);
    assert_eq!(tracker.update("GOOG", 2800.0), None);

    // Second values - returns previous
    assert_eq!(tracker.update("AAPL", 101.0), Some(100.0));
    assert_eq!(tracker.update("GOOG", 2810.0), Some(2800.0));

    // Delta by key
    assert_eq!(tracker.delta("AAPL", 102.0), Some(1.0)); // 102 - 101
}

/// When PrevTracker tracks a single value stream,
/// the tracker shall return the previous value on each update,
/// So that difference calculations are simple,
/// And the delta can be computed.
#[test]
fn test_prev_tracker_comprehensive() {
    let mut tracker = PrevTracker::new();

    assert_eq!(tracker.update(100.0), None);
    assert_eq!(tracker.update(110.0), Some(100.0));
    assert_eq!(tracker.delta(120.0), Some(10.0)); // 120 - 110
    assert_eq!(tracker.delta(125.0), Some(5.0)); // 125 - 120
}
