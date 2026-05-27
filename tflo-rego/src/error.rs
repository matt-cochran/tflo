//! Error types for tflow-rego.

use std::fmt;

/// Result type alias using [`RegoError`].
pub type RegoResult<T> = Result<T, RegoError>;

/// Errors that can occur during Rego operations.
#[derive(Debug, Clone)]
pub enum RegoError {
    /// Failed to parse a Rego policy.
    ParseError {
        /// The policy that failed to parse.
        policy: String,
        /// The error message.
        message: String,
    },

    /// Failed to evaluate a Rego query.
    EvaluationError {
        /// The query that failed.
        query: String,
        /// The error message.
        message: String,
    },

    /// Failed to serialize input to JSON.
    SerializationError {
        /// The error message.
        message: String,
    },

    /// Failed to load policy file.
    IoError {
        /// The error message.
        message: String,
    },

    /// Policy not found.
    PolicyNotFound {
        /// The policy name.
        name: String,
    },

    /// Invalid query result.
    InvalidResult {
        /// The expected type.
        expected: String,
        /// The actual result.
        actual: String,
    },

    /// Evaluation exceeded the configured wall-clock budget.
    ///
    /// `regorus` is synchronous and cannot be interrupted mid-evaluation,
    /// so this variant is reported by a post-evaluation watchdog: the
    /// evaluation already completed, but the elapsed time exceeded the
    /// caller's `budget_ms`.
    EvalTimeout {
        /// The query that exceeded the budget.
        query: String,
        /// The configured wall-clock budget, in milliseconds.
        budget_ms: u64,
        /// The actual elapsed time, in milliseconds.
        elapsed_ms: u64,
    },
}

impl fmt::Display for RegoError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ParseError { policy, message } => {
                write!(f, "failed to parse Rego policy '{policy}': {message}")
            }
            Self::EvaluationError { query, message } => {
                write!(f, "failed to evaluate Rego query '{query}': {message}")
            }
            Self::SerializationError { message } => {
                write!(f, "serialization error: {message}")
            }
            Self::IoError { message } => {
                write!(f, "I/O error: {message}")
            }
            Self::PolicyNotFound { name } => {
                write!(f, "policy not found: {name}")
            }
            Self::InvalidResult { expected, actual } => {
                write!(f, "invalid result: expected {expected}, got {actual}")
            }
            Self::EvalTimeout {
                query,
                budget_ms,
                elapsed_ms,
            } => {
                write!(
                    f,
                    "Rego query '{query}' exceeded {budget_ms}ms budget (elapsed {elapsed_ms}ms)"
                )
            }
        }
    }
}

impl std::error::Error for RegoError {}

impl From<std::io::Error> for RegoError {
    fn from(err: std::io::Error) -> Self {
        Self::IoError {
            message: err.to_string(),
        }
    }
}

impl From<serde_json::Error> for RegoError {
    fn from(err: serde_json::Error) -> Self {
        Self::SerializationError {
            message: err.to_string(),
        }
    }
}
