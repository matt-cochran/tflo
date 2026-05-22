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
/// A window can be either time-based (duration) or count-based (number of records).
///
/// # Examples
///
/// ```rust
/// use tflo_core::window::Window;
/// use std::time::Duration;
///
/// // Time-based window
/// let time_window = Window::Time(Duration::from_secs(300));
///
/// // Count-based window
/// let count_window = Window::Count(100);
///
/// // From Duration
/// let from_duration: Window = Duration::from_secs(60).into();
/// assert!(matches!(from_duration, Window::Time(_)));
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Window {
    /// Time-based window: include values within the specified duration.
    Time(Duration),
    /// Count-based window: include the last N values.
    Count(usize),
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

    /// Returns true if this is a time-based window.
    #[must_use]
    pub const fn is_time_based(&self) -> bool {
        matches!(self, Self::Time(_))
    }

    /// Returns true if this is a count-based window.
    #[must_use]
    pub const fn is_count_based(&self) -> bool {
        matches!(self, Self::Count(_))
    }

    /// Get the duration if this is a time-based window.
    #[must_use]
    pub const fn as_duration(&self) -> Option<Duration> {
        match self {
            Self::Time(d) => Some(*d),
            Self::Count(_) => None,
        }
    }

    /// Get the count if this is a count-based window.
    #[must_use]
    pub const fn as_count(&self) -> Option<usize> {
        match self {
            Self::Count(n) => Some(*n),
            Self::Time(_) => None,
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
        let w = Window::time(Duration::from_secs(60));
        assert!(w.is_time_based());
        assert!(!w.is_count_based());
        assert_eq!(w.as_duration(), Some(Duration::from_secs(60)));
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
