//! Error types for tflow.
//!
//! This module defines the error types used throughout the library.

use std::fmt;

/// Result type alias using [`TFloError`].
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

    /// The gap between two consecutive timestamps exceeded the configured
    /// `max_gap_ms` (when validation is enabled).
    TimestampGapExceeded {
        /// The previous timestamp.
        previous: i64,
        /// The current timestamp.
        current: i64,
        /// The configured maximum allowed gap, in milliseconds.
        max_gap: i64,
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
            Self::TimestampGapExceeded {
                previous,
                current,
                max_gap,
            } => {
                write!(
                    f,
                    "timestamp gap exceeded: previous={previous}, current={current}, \
                     gap={} > max_gap={max_gap}",
                    current.saturating_sub(*previous)
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
    /// A serialization/deserialization or codec step failed. Carries a static
    /// `context` tag plus the underlying error rendered as a string so the
    /// original diagnostic isn't lost.
    Decode {
        /// Short static label for what was being decoded/encoded.
        context: &'static str,
        /// `Display` rendering of the underlying error.
        source: String,
    },
    /// NaN value encountered.
    NaN,
    /// Infinite value encountered.
    Infinite,
    /// Attempted to advance the per-key event-time watermark backward.
    ///
    /// Returned by [`KeyedGraphState::advance_event_time_watermark`](crate::keyed::KeyedGraphState::advance_event_time_watermark)
    /// when the caller passes a value strictly less than the most recently
    /// advanced watermark. The watermark contract is monotonically
    /// non-decreasing per key; this error fails-fast on a violation
    /// rather than silently corrupting timer-fire ordering.
    NonMonotonicWatermark {
        /// The most recently advanced (or released) watermark for this key.
        last: i64,
        /// The (rejected) value the caller attempted to advance to.
        attempted: i64,
    },
    /// A snapshot's format version is not compatible with the running
    /// engine. Returned when restore encounters a snapshot whose
    /// `SnapshotMetadata.version` falls outside the engine's supported
    /// range. Operators planning a rolling upgrade should write
    /// migrations explicitly; this variant fails-fast on a silent
    /// version skew that would otherwise produce wrong outputs.
    IncompatibleSnapshotVersion {
        /// Snapshot format version the engine expected (its current).
        expected: u32,
        /// Snapshot format version the engine actually found in metadata.
        actual: u32,
        /// Short hint for the diagnostic, e.g. "snapshot v1 cannot be
        /// loaded by engine v2 without a migration".
        hint: &'static str,
    },
}

impl std::fmt::Display for ComputeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DivisionByZero => write!(f, "division by zero"),
            Self::InvalidInput { reason } => write!(f, "invalid input: {reason}"),
            Self::Decode { context, source } => write!(f, "{context}: {source}"),
            Self::NaN => write!(f, "NaN value"),
            Self::Infinite => write!(f, "infinite value"),
            Self::NonMonotonicWatermark { last, attempted } => write!(
                f,
                "non-monotonic event-time watermark advance: last={last}, attempted={attempted}"
            ),
            Self::IncompatibleSnapshotVersion { expected, actual, hint } => write!(
                f,
                "incompatible snapshot format version: expected={expected}, actual={actual} ({hint})"
            ),
        }
    }
}

/// Categorization of a [`ComputeError`] (or [`TFloError`]) for caller
/// retry policy. Backed by the [`ComputeError::kind`] accessor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorKind {
    /// The operation can be retried — the failure is transient. Examples:
    /// a network timeout, a temporarily-overloaded backend, a flaky I/O
    /// hop. Callers should back off and try again.
    Retriable,
    /// The operation cannot succeed by retrying — the failure is a
    /// permanent input/contract violation. Examples: a non-monotonic
    /// watermark, an incompatible snapshot version, a malformed config.
    /// Callers should surface the error to the operator.
    Permanent,
}

impl ComputeError {
    /// Classify this error for retry policy. See [`ErrorKind`].
    ///
    /// Used by Phase 3 callers (e.g. the [`Checkpointer`](crate::state::Checkpointer)
    /// circuit breaker) to decide whether a failure should reset the
    /// consecutive-failure counter (Retriable) or trip the breaker
    /// (Permanent).
    #[must_use]
    pub const fn kind(&self) -> ErrorKind {
        match self {
            // Permanent: contract violations the caller introduced; retry
            // would just reproduce the failure.
            Self::DivisionByZero
            | Self::InvalidInput { .. }
            | Self::Decode { .. }
            | Self::NaN
            | Self::Infinite
            | Self::NonMonotonicWatermark { .. }
            | Self::IncompatibleSnapshotVersion { .. } => ErrorKind::Permanent,
        }
    }
}

impl TFloError {
    /// Classify this error for retry policy. See [`ErrorKind`].
    #[must_use]
    pub const fn kind(&self) -> ErrorKind {
        match self {
            // All current variants are permanent (contract / config /
            // input violations). Future I/O- or network-shaped variants
            // should return `Retriable`.
            Self::MissingTimestamp
            | Self::OutOfOrderTimestamp { .. }
            | Self::TimestampGapExceeded { .. }
            | Self::DivisionByZero
            | Self::InvalidWindow { .. }
            | Self::CycleDetected
            | Self::NodeNotFound { .. }
            | Self::InsufficientData { .. }
            | Self::Configuration { .. }
            | Self::NaN
            | Self::Infinite
            | Self::NegativeValue { .. } => ErrorKind::Permanent,
            Self::Compute(e) => e.kind(),
        }
    }
}

impl std::error::Error for ComputeError {}
