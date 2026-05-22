//! Duration literal extensions for ergonomic time specifications.
//!
//! This module provides the [`IntoDuration`] trait which allows writing
//! durations in a natural way:
//!
//! ```rust
//! use tflo_core::duration::IntoDuration;
//! use std::time::Duration;
//!
//! let five_seconds = 5_u64.secs();
//! let two_minutes = 2_u64.mins();
//! let one_hour = 1_u64.hours();
//! let three_days = 3_u64.days();
//!
//! assert_eq!(five_seconds, Duration::from_secs(5));
//! assert_eq!(two_minutes, Duration::from_secs(120));
//! ```

use std::time::Duration;

/// Extension trait for converting integers to [`Duration`].
///
/// Implemented for common integer types to allow ergonomic duration literals.
pub trait IntoDuration {
    /// Convert to milliseconds.
    ///
    /// # Examples
    /// ```
    /// use tflo_core::duration::IntoDuration;
    /// assert_eq!(500_u64.ms(), std::time::Duration::from_millis(500));
    /// ```
    fn ms(self) -> Duration;

    /// Convert to seconds.
    ///
    /// # Examples
    /// ```
    /// use tflo_core::duration::IntoDuration;
    /// assert_eq!(5_u64.secs(), std::time::Duration::from_secs(5));
    /// ```
    fn secs(self) -> Duration;

    /// Convert to minutes.
    ///
    /// # Examples
    /// ```
    /// use tflo_core::duration::IntoDuration;
    /// assert_eq!(2_u64.mins(), std::time::Duration::from_secs(120));
    /// ```
    fn mins(self) -> Duration;

    /// Convert to hours.
    ///
    /// # Examples
    /// ```
    /// use tflo_core::duration::IntoDuration;
    /// assert_eq!(1_u64.hours(), std::time::Duration::from_secs(3600));
    /// ```
    fn hours(self) -> Duration;

    /// Convert to days.
    ///
    /// # Examples
    /// ```
    /// use tflo_core::duration::IntoDuration;
    /// assert_eq!(1_u64.days(), std::time::Duration::from_secs(86400));
    /// ```
    fn days(self) -> Duration;

    /// Convert to microseconds.
    ///
    /// # Examples
    /// ```
    /// use tflo_core::duration::IntoDuration;
    /// assert_eq!(100_u64.us(), std::time::Duration::from_micros(100));
    /// ```
    fn us(self) -> Duration;

    /// Convert to nanoseconds.
    ///
    /// # Examples
    /// ```
    /// use tflo_core::duration::IntoDuration;
    /// assert_eq!(1000_u64.ns(), std::time::Duration::from_nanos(1000));
    /// ```
    fn ns(self) -> Duration;
}

impl IntoDuration for u64 {
    fn ms(self) -> Duration {
        Duration::from_millis(self)
    }

    fn secs(self) -> Duration {
        Duration::from_secs(self)
    }

    fn mins(self) -> Duration {
        Duration::from_secs(self * 60)
    }

    fn hours(self) -> Duration {
        Duration::from_secs(self * 3600)
    }

    fn days(self) -> Duration {
        Duration::from_secs(self * 86400)
    }

    fn us(self) -> Duration {
        Duration::from_micros(self)
    }

    fn ns(self) -> Duration {
        Duration::from_nanos(self)
    }
}

impl IntoDuration for u32 {
    fn ms(self) -> Duration {
        (u64::from(self)).ms()
    }

    fn secs(self) -> Duration {
        (u64::from(self)).secs()
    }

    fn mins(self) -> Duration {
        (u64::from(self)).mins()
    }

    fn hours(self) -> Duration {
        (u64::from(self)).hours()
    }

    fn days(self) -> Duration {
        (u64::from(self)).days()
    }

    fn us(self) -> Duration {
        (u64::from(self)).us()
    }

    fn ns(self) -> Duration {
        (u64::from(self)).ns()
    }
}

impl IntoDuration for i32 {
    fn ms(self) -> Duration {
        #[allow(clippy::cast_sign_loss)]
        (self.max(0) as u64).ms()
    }

    fn secs(self) -> Duration {
        #[allow(clippy::cast_sign_loss)]
        (self.max(0) as u64).secs()
    }

    fn mins(self) -> Duration {
        #[allow(clippy::cast_sign_loss)]
        (self.max(0) as u64).mins()
    }

    fn hours(self) -> Duration {
        #[allow(clippy::cast_sign_loss)]
        (self.max(0) as u64).hours()
    }

    fn days(self) -> Duration {
        #[allow(clippy::cast_sign_loss)]
        (self.max(0) as u64).days()
    }

    fn us(self) -> Duration {
        #[allow(clippy::cast_sign_loss)]
        (self.max(0) as u64).us()
    }

    fn ns(self) -> Duration {
        #[allow(clippy::cast_sign_loss)]
        (self.max(0) as u64).ns()
    }
}

impl IntoDuration for i64 {
    fn ms(self) -> Duration {
        #[allow(clippy::cast_sign_loss)]
        (self.max(0) as u64).ms()
    }

    fn secs(self) -> Duration {
        #[allow(clippy::cast_sign_loss)]
        (self.max(0) as u64).secs()
    }

    fn mins(self) -> Duration {
        #[allow(clippy::cast_sign_loss)]
        (self.max(0) as u64).mins()
    }

    fn hours(self) -> Duration {
        #[allow(clippy::cast_sign_loss)]
        (self.max(0) as u64).hours()
    }

    fn days(self) -> Duration {
        #[allow(clippy::cast_sign_loss)]
        (self.max(0) as u64).days()
    }

    fn us(self) -> Duration {
        #[allow(clippy::cast_sign_loss)]
        (self.max(0) as u64).us()
    }

    fn ns(self) -> Duration {
        #[allow(clippy::cast_sign_loss)]
        (self.max(0) as u64).ns()
    }
}

impl IntoDuration for usize {
    fn ms(self) -> Duration {
        #[allow(clippy::cast_possible_truncation)]
        (self as u64).ms()
    }

    fn secs(self) -> Duration {
        #[allow(clippy::cast_possible_truncation)]
        (self as u64).secs()
    }

    fn mins(self) -> Duration {
        #[allow(clippy::cast_possible_truncation)]
        (self as u64).mins()
    }

    fn hours(self) -> Duration {
        #[allow(clippy::cast_possible_truncation)]
        (self as u64).hours()
    }

    fn days(self) -> Duration {
        #[allow(clippy::cast_possible_truncation)]
        (self as u64).days()
    }

    fn us(self) -> Duration {
        #[allow(clippy::cast_possible_truncation)]
        (self as u64).us()
    }

    fn ns(self) -> Duration {
        #[allow(clippy::cast_possible_truncation)]
        (self as u64).ns()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_u64_conversions() {
        assert_eq!(500_u64.ms(), Duration::from_millis(500));
        assert_eq!(5_u64.secs(), Duration::from_secs(5));
        assert_eq!(2_u64.mins(), Duration::from_secs(120));
        assert_eq!(1_u64.hours(), Duration::from_secs(3600));
        assert_eq!(1_u64.days(), Duration::from_secs(86400));
        assert_eq!(100_u64.us(), Duration::from_micros(100));
        assert_eq!(1000_u64.ns(), Duration::from_nanos(1000));
    }

    #[test]
    fn test_i32_negative_clamps_to_zero() {
        assert_eq!((-5_i32).secs(), Duration::from_secs(0));
        assert_eq!((-1_i32).mins(), Duration::from_secs(0));
    }
}
