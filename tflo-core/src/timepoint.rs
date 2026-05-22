/// TimePoint trait for generic time types.
///
/// This module provides the [`TimePoint`] trait which abstracts over different
/// time representations, allowing the library to work with various time types.

/// Trait for types that represent a point in time.
///
/// This trait allows the library to work with different time representations
/// (timestamps, sequence numbers, etc.) in a generic way.
pub trait TimePoint: Copy + Clone + PartialOrd + Ord + PartialEq + Eq + Send + Sync {
    /// Convert to milliseconds since epoch (or equivalent ordering value).
    fn to_millis(self) -> i64;

    /// Create from milliseconds since epoch (or equivalent ordering value).
    fn from_millis(ms: i64) -> Self;

    /// Get the ordering key for this time point.
    ///
    /// This is used for windowing and ordering operations.
    fn ordering_key(self) -> i64 {
        self.to_millis()
    }
}

impl TimePoint for i64 {
    fn to_millis(self) -> i64 {
        self
    }

    fn from_millis(ms: i64) -> Self {
        ms
    }
}

impl TimePoint for u64 {
    fn to_millis(self) -> i64 {
        self as i64
    }

    fn from_millis(ms: i64) -> Self {
        ms as u64
    }
}

impl TimePoint for i32 {
    fn to_millis(self) -> i64 {
        self as i64
    }

    fn from_millis(ms: i64) -> Self {
        ms as i32
    }
}

impl TimePoint for u32 {
    fn to_millis(self) -> i64 {
        self as i64
    }

    fn from_millis(ms: i64) -> Self {
        ms as u32
    }
}
