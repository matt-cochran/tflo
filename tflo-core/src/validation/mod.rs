//! Validation options and utilities for temporal computations.
//!
//! This module provides tools for validating input data and computation
//! results, including:
//!
//! - Sorted timestamp validation
//! - Warmup period tracking
//! - NaN handling options

mod helpers;

pub use helpers::{
    require_finite, require_finite_opt, require_non_negative, require_not_infinite,
    require_not_nan, require_positive,
};

use crate::error::TFloError;

/// Options for validating temporal computations.
///
/// # Examples
///
/// ```rust
/// use tflo_core::validation::ValidationOptions;
///
/// let options = ValidationOptions::new()
///     .assert_sorted(true)
///     .min_warmup(10);
/// ```
#[derive(Debug, Clone)]
pub struct ValidationOptions {
    /// Whether to assert that timestamps are in sorted order.
    pub assert_sorted: bool,
    /// Minimum number of records before results are considered valid.
    pub min_warmup: usize,
    /// Whether to reject NaN input values.
    pub reject_nan: bool,
    /// Whether to reject infinite input values.
    pub reject_inf: bool,
    /// Whether to return errors for NaN values (stronger than `reject_nan`).
    pub error_on_nan: bool,
    /// Whether to return errors for infinite values (stronger than `reject_inf`).
    pub error_on_inf: bool,
    /// Whether to return errors for negative values in operations that don't allow them.
    pub error_on_negative: bool,
    /// Maximum allowed gap between timestamps (milliseconds).
    pub max_gap_ms: Option<i64>,
}

impl Default for ValidationOptions {
    fn default() -> Self {
        Self::new()
    }
}

impl ValidationOptions {
    /// Create default validation options.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            assert_sorted: false,
            min_warmup: 0,
            reject_nan: false,
            reject_inf: false,
            error_on_nan: false,
            error_on_inf: false,
            error_on_negative: false,
            max_gap_ms: None,
        }
    }

    /// Create strict validation options (all checks enabled).
    #[must_use]
    pub const fn strict() -> Self {
        Self {
            assert_sorted: true,
            min_warmup: 1,
            reject_nan: true,
            reject_inf: true,
            error_on_nan: true,
            error_on_inf: true,
            error_on_negative: true,
            max_gap_ms: None,
        }
    }

    /// Set whether to assert sorted timestamps.
    #[must_use]
    pub const fn assert_sorted(mut self, assert: bool) -> Self {
        self.assert_sorted = assert;
        self
    }

    /// Set the minimum warmup period.
    #[must_use]
    pub const fn min_warmup(mut self, warmup: usize) -> Self {
        self.min_warmup = warmup;
        self
    }

    /// Set whether to reject NaN values.
    #[must_use]
    pub const fn reject_nan(mut self, reject: bool) -> Self {
        self.reject_nan = reject;
        self
    }

    /// Set whether to reject infinite values.
    #[must_use]
    pub const fn reject_inf(mut self, reject: bool) -> Self {
        self.reject_inf = reject;
        self
    }

    /// Set whether to return errors for NaN values.
    ///
    /// When enabled, NaN values will cause `ComputeResult::Error(ComputeError::NaN)`
    /// instead of silently filtering or returning NaN.
    #[must_use]
    pub const fn error_on_nan(mut self, error: bool) -> Self {
        self.error_on_nan = error;
        self
    }

    /// Set whether to return errors for infinite values.
    ///
    /// When enabled, infinite values will cause `ComputeResult::Error(ComputeError::Infinite)`
    /// instead of silently filtering or returning Inf.
    #[must_use]
    pub const fn error_on_inf(mut self, error: bool) -> Self {
        self.error_on_inf = error;
        self
    }

    /// Set whether to return errors for negative values.
    ///
    /// When enabled, negative values in operations like `ln()` will cause
    /// `ComputeResult::Error(ComputeError::InvalidInput)`.
    #[must_use]
    pub const fn error_on_negative(mut self, error: bool) -> Self {
        self.error_on_negative = error;
        self
    }

    /// Set the maximum allowed gap between timestamps.
    #[must_use]
    pub const fn max_gap_ms(mut self, gap: i64) -> Self {
        self.max_gap_ms = Some(gap);
        self
    }
}

/// Validator for checking timestamp order.
#[derive(Debug, Default)]
pub struct TimestampValidator {
    last_ts: Option<i64>,
    violations: usize,
}

impl TimestampValidator {
    /// Create a new timestamp validator.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Check a timestamp and return whether it's valid (in order).
    pub fn check(&mut self, ts: i64) -> bool {
        let valid = self.last_ts.is_none_or(|last| ts >= last);
        if !valid {
            self.violations += 1;
        }
        self.last_ts = Some(ts);
        valid
    }

    /// Get the number of out-of-order violations.
    #[must_use]
    pub const fn violations(&self) -> usize {
        self.violations
    }

    /// Reset the validator.
    pub fn reset(&mut self) {
        self.last_ts = None;
        self.violations = 0;
    }
}

/// Warmup tracker for determining when computations are valid.
#[derive(Debug)]
pub struct WarmupTracker {
    records_seen: usize,
    min_required: usize,
    by_node: std::collections::HashMap<usize, usize>,
}

impl WarmupTracker {
    /// Create a new warmup tracker.
    #[must_use]
    pub fn new(min_required: usize) -> Self {
        Self {
            records_seen: 0,
            min_required,
            by_node: std::collections::HashMap::new(),
        }
    }

    /// Record that a new record has been processed.
    pub fn record(&mut self) {
        self.records_seen += 1;
    }

    /// Record warmup for a specific node.
    pub fn record_node(&mut self, node_id: usize) {
        *self.by_node.entry(node_id).or_insert(0) += 1;
    }

    /// Check if globally warmed up.
    #[must_use]
    pub const fn is_warmed_up(&self) -> bool {
        self.records_seen >= self.min_required
    }

    /// Check if a specific node is warmed up.
    #[must_use]
    pub fn is_node_warmed_up(&self, node_id: usize, required: usize) -> bool {
        self.by_node
            .get(&node_id)
            .is_some_and(|&count| count >= required)
    }

    /// Get the number of records seen.
    #[must_use]
    pub const fn records_seen(&self) -> usize {
        self.records_seen
    }

    /// Reset the tracker.
    pub fn reset(&mut self) {
        self.records_seen = 0;
        self.by_node.clear();
    }
}

/// Value validator for checking input values.
#[derive(Debug, Clone)]
pub struct ValueValidator {
    options: ValidationOptions,
    nan_count: usize,
    inf_count: usize,
}

impl ValueValidator {
    /// Create a new value validator with the given options.
    #[must_use]
    pub const fn new(options: ValidationOptions) -> Self {
        Self {
            options,
            nan_count: 0,
            inf_count: 0,
        }
    }

    /// Check a value and return whether it's valid.
    pub fn check(&mut self, value: f64) -> bool {
        if value.is_nan() {
            self.nan_count += 1;
            if self.options.reject_nan {
                return false;
            }
        }
        if value.is_infinite() {
            self.inf_count += 1;
            if self.options.reject_inf {
                return false;
            }
        }
        true
    }

    /// Check a value against every value-validation option.
    ///
    /// This is the full enforcement path used by
    /// [`validated()`](crate::iter_ext::TFlowIteratorExt::validated):
    ///
    /// - `Ok(true)` — the value passes.
    /// - `Ok(false)` — a `reject_*` option matched; the value should be
    ///   filtered out of the output.
    /// - `Err(..)` — an `error_on_*` option matched; the stream should fail.
    ///
    /// When both an `error_on_*` and the matching `reject_*` option are set,
    /// the error takes precedence (it is the stronger check).
    ///
    /// # Errors
    ///
    /// Returns [`TFloError::NaN`], [`TFloError::Infinite`], or
    /// [`TFloError::NegativeValue`] when the corresponding `error_on_*` option
    /// is enabled and the value matches.
    pub fn check_strict(&mut self, value: f64) -> Result<bool, TFloError> {
        if value.is_nan() {
            self.nan_count += 1;
            if self.options.error_on_nan {
                return Err(TFloError::NaN);
            }
            return Ok(!self.options.reject_nan);
        }
        if value.is_infinite() {
            self.inf_count += 1;
            if self.options.error_on_inf {
                return Err(TFloError::Infinite);
            }
            return Ok(!self.options.reject_inf);
        }
        if value < 0.0 && self.options.error_on_negative {
            return Err(TFloError::NegativeValue {
                reason: "validated() received a negative value (error_on_negative is enabled)",
            });
        }
        Ok(true)
    }

    /// Get the count of NaN values seen.
    #[must_use]
    pub const fn nan_count(&self) -> usize {
        self.nan_count
    }

    /// Get the count of infinite values seen.
    #[must_use]
    pub const fn inf_count(&self) -> usize {
        self.inf_count
    }

    /// Reset the validator.
    pub fn reset(&mut self) {
        self.nan_count = 0;
        self.inf_count = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_timestamp_validator() {
        let mut validator = TimestampValidator::new();

        assert!(validator.check(1000));
        assert!(validator.check(2000));
        assert!(validator.check(2000)); // Equal is OK
        assert!(!validator.check(1500)); // Out of order
        assert_eq!(validator.violations(), 1);
    }

    #[test]
    fn test_warmup_tracker() {
        let mut tracker = WarmupTracker::new(3);

        assert!(!tracker.is_warmed_up());
        tracker.record();
        tracker.record();
        assert!(!tracker.is_warmed_up());
        tracker.record();
        assert!(tracker.is_warmed_up());
    }

    #[test]
    fn test_value_validator() {
        let options = ValidationOptions::new().reject_nan(true).reject_inf(true);
        let mut validator = ValueValidator::new(options);

        assert!(validator.check(100.0));
        assert!(!validator.check(f64::NAN));
        assert!(!validator.check(f64::INFINITY));
        assert_eq!(validator.nan_count(), 1);
        assert_eq!(validator.inf_count(), 1);
    }

    #[test]
    fn test_validation_options_builder() {
        let options = ValidationOptions::new()
            .assert_sorted(true)
            .min_warmup(10)
            .reject_nan(true);

        assert!(options.assert_sorted);
        assert_eq!(options.min_warmup, 10);
        assert!(options.reject_nan);
        assert!(!options.reject_inf);
    }
}
