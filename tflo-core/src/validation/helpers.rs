//! Validation helpers for scalar values.
//!
//! This module provides helper functions for validating scalar values,
//! including require_finite checks using the Scalar trait.

use crate::error::ComputeError;
use crate::error::ComputeResult;
use crate::scalar::Scalar;

/// Require that a scalar value is finite (not NaN or infinite).
///
/// Returns `ComputeResult::Error(ComputeError::NaN)` if the value is NaN,
/// `ComputeResult::Error(ComputeError::Infinite)` if infinite, or
/// `ComputeResult::Value(value)` if finite.
///
/// Catches invalid values early, before they propagate through the graph.
///
/// # Example
///
/// ```ignore
/// let result = require_finite(100.0)?; // Ok(ComputeResult::Value(100.0))
/// let result = require_finite(f64::NAN)?; // Err(ComputeError::NaN)
/// ```
pub fn require_finite<T: Scalar>(value: T) -> ComputeResult<T> {
    if value.is_nan() {
        return ComputeResult::Error(ComputeError::NaN);
    }
    if value.is_infinite() {
        return ComputeResult::Error(ComputeError::Infinite);
    }
    if !value.is_finite() {
        // Double-check: if not finite and not NaN/inf, something is wrong
        return ComputeResult::Error(ComputeError::NaN);
    }
    ComputeResult::Value(value)
}

/// Require that a scalar value is finite, returning an Option.
///
/// Returns `None` if the value is NaN or infinite, `Some(value)` otherwise.
///
/// # Example
///
/// ```ignore
/// let result = require_finite_opt(100.0); // Some(100.0)
/// let result = require_finite_opt(f64::NAN); // None
/// ```
#[must_use]
pub fn require_finite_opt<T: Scalar>(value: T) -> Option<T> {
    if value.is_finite() {
        Some(value)
    } else {
        None
    }
}

/// Require that a scalar value is not NaN.
///
/// Returns `ComputeResult::Error(ComputeError::NaN)` if NaN, or
/// `ComputeResult::Value(value)` otherwise.
pub fn require_not_nan<T: Scalar>(value: T) -> ComputeResult<T> {
    if value.is_nan() {
        ComputeResult::Error(ComputeError::NaN)
    } else {
        ComputeResult::Value(value)
    }
}

/// Require that a scalar value is not infinite.
///
/// Returns `ComputeResult::Error(ComputeError::Infinite)` if infinite, or
/// `ComputeResult::Value(value)` otherwise.
pub fn require_not_infinite<T: Scalar>(value: T) -> ComputeResult<T> {
    if value.is_infinite() {
        ComputeResult::Error(ComputeError::Infinite)
    } else {
        ComputeResult::Value(value)
    }
}

/// Require that a scalar value is positive (greater than zero).
///
/// Returns `ComputeResult::Error(ComputeError::InvalidInput)` if <= 0,
/// or `ComputeResult::Value(value)` otherwise.
pub fn require_positive<T: Scalar>(value: T) -> ComputeResult<T> {
    if value <= T::zero() {
        ComputeResult::Error(ComputeError::InvalidInput {
            reason: "Value must be positive (greater than zero)",
        })
    } else {
        ComputeResult::Value(value)
    }
}

/// Require that a scalar value is non-negative (>= zero).
///
/// Returns `ComputeResult::Error(ComputeError::InvalidInput)` if < 0, or
/// `ComputeResult::Value(value)` otherwise.
pub fn require_non_negative<T: Scalar>(value: T) -> ComputeResult<T> {
    if value < T::zero() {
        ComputeResult::Error(ComputeError::InvalidInput {
            reason: "Value must be non-negative (>= zero)",
        })
    } else {
        ComputeResult::Value(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_require_finite() {
        assert!(matches!(
            require_finite(100.0),
            ComputeResult::Value(100.0)
        ));
        assert!(matches!(
            require_finite(f64::NAN),
            ComputeResult::Error(ComputeError::NaN)
        ));
        assert!(matches!(
            require_finite(f64::INFINITY),
            ComputeResult::Error(ComputeError::Infinite)
        ));
    }

    #[test]
    fn test_require_finite_opt() {
        assert_eq!(require_finite_opt(100.0), Some(100.0));
        assert_eq!(require_finite_opt(f64::NAN), None);
        assert_eq!(require_finite_opt(f64::INFINITY), None);
    }

    #[test]
    fn test_require_positive() {
        assert!(matches!(
            require_positive(1.0),
            ComputeResult::Value(1.0)
        ));
        assert!(matches!(
            require_positive(0.0),
            ComputeResult::Error(ComputeError::InvalidInput { .. })
        ));
        assert!(matches!(
            require_positive(-1.0),
            ComputeResult::Error(ComputeError::InvalidInput { .. })
        ));
    }
}

