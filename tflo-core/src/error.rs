//! Error types for tflow.
//!
//! This module defines the error types used throughout the library.

use std::fmt;

/// Result type alias using [`TemporalError`].
pub type TFloResult<T> = Result<T, TFloError>;

/// Errors that can occur during temporal computation.
#[derive(Debug, Clone, PartialEq)]
pub enum TFloError {
    /// Timestamp extractor was not configured but is required.
    MissingTimestamp,

    /// Timestamps are not in sorted order (when validation is enabled).
    OutOfOrderTimestamp {
        /// The previous timestamp.
        previous: i64,
        /// The current (out-of-order) timestamp.
        current: i64,
    },

    /// Division by zero in a computation.
    DivisionByZero,

    /// Invalid window configuration.
    InvalidWindow {
        /// Description of the issue.
        message: String,
    },

    /// Computation graph contains a cycle (should not happen with current API).
    CycleDetected,

    /// A required node was not found during compilation.
    NodeNotFound {
        /// The node ID that was not found.
        node_id: usize,
    },

    /// Insufficient data for the requested operation.
    InsufficientData {
        /// Number of samples required.
        required: usize,
        /// Number of samples available.
        available: usize,
    },

    /// Configuration error.
    Configuration {
        /// Description of the configuration issue.
        message: String,
    },

    /// NaN value encountered (when validation is enabled).
    NaN,

    /// Infinite value encountered (when validation is enabled).
    Infinite,

    /// Negative value encountered where not allowed (e.g., log of negative).
    NegativeValue {
        /// Description of why negative is invalid.
        reason: &'static str,
    },

    /// Computation error from a node execution.
    Compute(ComputeError),
}

impl fmt::Display for TFloError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingTimestamp => {
                write!(f, "timestamp extractor not configured (call t.timestamp())")
            }
            Self::OutOfOrderTimestamp { previous, current } => {
                write!(
                    f,
                    "out-of-order timestamp: previous={previous}, current={current}"
                )
            }
            Self::DivisionByZero => write!(f, "division by zero in computation"),
            Self::InvalidWindow { message } => write!(f, "invalid window: {message}"),
            Self::CycleDetected => write!(f, "cycle detected in computation graph"),
            Self::NodeNotFound { node_id } => write!(f, "node not found: {node_id}"),
            Self::InsufficientData {
                required,
                available,
            } => {
                write!(
                    f,
                    "insufficient data: required {required}, available {available}"
                )
            }
            Self::Configuration { message } => write!(f, "configuration error: {message}"),
            Self::NaN => write!(f, "NaN value encountered"),
            Self::Infinite => write!(f, "infinite value encountered"),
            Self::NegativeValue { reason } => write!(f, "negative value not allowed: {reason}"),
            Self::Compute(e) => write!(f, "computation error: {e}"),
        }
    }
}

impl std::error::Error for TFloError {}

/// Result type for computation steps that may be warming up or error.
///
/// This enum provides explicit handling of warmup periods and errors,
/// replacing the previous `Option<T>` pattern where warmup was silently
/// filtered out.
///
/// # Examples
///
/// ```rust
/// use tflo_core::error::ComputeResult;
///
/// let result: ComputeResult<i32> = ComputeResult::WarmingUp { remaining: 1 };
/// match result {
///     ComputeResult::Value(_v) => {}
///     ComputeResult::WarmingUp { remaining } => assert_eq!(remaining, 1),
///     ComputeResult::Error(_e) => {}
/// }
/// ```
#[derive(Debug, Clone, PartialEq)]
pub enum ComputeResult<T> {
    /// Computation succeeded with a value.
    Value(T),
    /// Computation is still warming up (insufficient data).
    WarmingUp {
        /// Number of records still needed before valid output.
        remaining: usize,
    },
    /// Computation failed with an error.
    Error(ComputeError),
}

/// Error type for computation failures.
///
/// This is a simplified error type focused on computation-specific failures.
/// For more general errors, see [`TFloError`].
#[derive(Debug, Clone, PartialEq)]
pub enum ComputeError {
    /// Division by zero occurred.
    DivisionByZero,
    /// Invalid input value (NaN, Inf, negative, etc.).
    InvalidInput {
        /// Reason why the input is invalid.
        reason: &'static str,
    },
    /// NaN value encountered.
    NaN,
    /// Infinite value encountered.
    Infinite,
}

impl std::fmt::Display for ComputeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DivisionByZero => write!(f, "division by zero"),
            Self::InvalidInput { reason } => write!(f, "invalid input: {reason}"),
            Self::NaN => write!(f, "NaN value"),
            Self::Infinite => write!(f, "infinite value"),
        }
    }
}

impl std::error::Error for ComputeError {}
