//! Window types for time-based and count-based aggregations.
//!
//! Windows define the scope of data considered for aggregations.
//! Two types are supported:
//!
//! - **Time-based**: Include all values within a duration (e.g., last 5 minutes)
//! - **Count-based**: Include the last N values

mod spec;

pub use spec::{IntoSamples, WindowSpec, WindowSpecExt};

use std::time::Duration;

/// Window specification for aggregations.
///
/// Four variants spanning the two common shapes:
///
/// - **Sliding** windows ([`Time`](Self::Time), [`Count`](Self::Count)) emit
///   the aggregate at every record, over the values currently inside the
///   window. The sliding-aggregation operators (`sma`, `ema`, `std`, `max`,
///   `min`, `sum`, `count`, `wma`, etc.) accept these.
/// - **Emit-trigger** windows ([`Session`](Self::Session),
///   [`Tumbling`](Self::Tumbling)) emit only when their close trigger
///   fires — a gap of inactivity for `Session`, a bucket-edge for
///   `Tumbling`. The emit-trigger operators (`session_sum`,
///   `tumbling_sum`, etc., introduced in Phase 2 of the closure plan)
///   accept these and require the Phase 1 `TimerService`.
///
/// Passing an emit-trigger variant to a sliding-aggregation operator
/// panics at construction with an actionable message; passing a sliding
/// variant to an emit-trigger operator does the same. The semantic
/// mismatch is large enough that runtime fail-fast is preferable to
/// silently producing wrong outputs.
///
/// # Examples
///
/// ```rust
/// use tflo_core::window::Window;
/// use std::time::Duration;
///
/// let time_window = Window::Time(Duration::from_secs(300));
/// let count_window = Window::Count(100);
/// let session = Window::Session { gap: Duration::from_secs(30) };
/// let tumbling = Window::Tumbling { size: Duration::from_secs(60) };
///
/// // Sugar: From<Duration> defaults to Time (sliding).
/// let from_duration: Window = Duration::from_secs(60).into();
/// assert!(matches!(from_duration, Window::Time(_)));
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Window {
    /// Sliding time-based window: include values within the specified
    /// duration, sliding past as new records arrive.
    Time(Duration),
    /// Sliding count-based window: include the last N values.
    Count(usize),
    /// Session window: emit on inactivity gap exceeding `gap`.
    ///
    /// Aggregate state accumulates across consecutive records on the same
    /// key; emission happens when no record has arrived within `gap`
    /// of the last (via a Phase 1 event-time timer).
    Session {
        /// Inactivity duration that closes the session.
        gap: Duration,
    },
    /// Tumbling window: emit on bucket-edge events spaced by `size`.
    ///
    /// Non-overlapping fixed-width buckets. Aggregate accumulates within
    /// a bucket; emission happens on the bucket-edge timer; the
    /// accumulator resets on emission.
    Tumbling {
        /// Bucket width.
        size: Duration,
    },
}

impl Window {
    /// Create a time-based window from a duration.
    #[must_use]
    pub const fn time(duration: Duration) -> Self {
        Self::Time(duration)
    }

    /// Create a count-based window for the last N records.
    #[must_use]
    pub const fn count(n: usize) -> Self {
        Self::Count(n)
    }

    /// Construct a session window with the given inactivity gap.
    #[must_use]
    pub const fn session(gap: Duration) -> Self {
        Self::Session { gap }
    }

    /// Construct a tumbling window with the given bucket size.
    #[must_use]
    pub const fn tumbling(size: Duration) -> Self {
        Self::Tumbling { size }
    }

    /// Returns true if this is a sliding time-based window.
    #[must_use]
    pub const fn is_time_based(&self) -> bool {
        matches!(self, Self::Time(_))
    }

    /// Returns true if this is a sliding count-based window.
    #[must_use]
    pub const fn is_count_based(&self) -> bool {
        matches!(self, Self::Count(_))
    }

    /// Returns true if this is an emit-trigger window
    /// ([`Session`](Self::Session) or [`Tumbling`](Self::Tumbling)).
    #[must_use]
    pub const fn is_emit_trigger(&self) -> bool {
        matches!(self, Self::Session { .. } | Self::Tumbling { .. })
    }

    /// Get the duration of a sliding time-based window.
    #[must_use]
    pub const fn as_duration(&self) -> Option<Duration> {
        match self {
            Self::Time(d) => Some(*d),
            Self::Count(_) | Self::Session { .. } | Self::Tumbling { .. } => None,
        }
    }

    /// Get the count of a sliding count-based window.
    #[must_use]
    pub const fn as_count(&self) -> Option<usize> {
        match self {
            Self::Count(n) => Some(*n),
            Self::Time(_) | Self::Session { .. } | Self::Tumbling { .. } => None,
        }
    }
}

impl From<Duration> for Window {
    fn from(duration: Duration) -> Self {
        Self::Time(duration)
    }
}

impl From<usize> for Window {
    fn from(count: usize) -> Self {
        Self::Count(count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_window_time_based() {
        let w = Window::time(Duration::from_mins(1));
        assert!(w.is_time_based());
        assert!(!w.is_count_based());
        assert_eq!(w.as_duration(), Some(Duration::from_mins(1)));
        assert_eq!(w.as_count(), None);
    }

    #[test]
    fn test_window_count_based() {
        let w = Window::count(100);
        assert!(!w.is_time_based());
        assert!(w.is_count_based());
        assert_eq!(w.as_duration(), None);
        assert_eq!(w.as_count(), Some(100));
    }

    #[test]
    fn test_window_from_duration() {
        let w: Window = Duration::from_secs(30).into();
        assert!(matches!(w, Window::Time(d) if d == Duration::from_secs(30)));
    }

    #[test]
    fn test_window_from_usize() {
        let w: Window = 50_usize.into();
        assert!(matches!(w, Window::Count(50)));
    }
}
