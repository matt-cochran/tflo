//! Error types for tflow-cel.

use std::fmt;

/// Result type alias using [`CelError`].
pub type CelResult<T> = Result<T, CelError>;

/// Errors that can occur during CEL operations.
#[derive(Debug, Clone)]
pub enum CelError {
    /// Failed to compile a CEL expression.
    CompileError {
        /// The expression that failed to compile.
        expression: String,
        /// The error message.
        message: String,
    },

    /// Failed to evaluate a CEL expression.
    EvaluationError {
        /// The expression that failed.
        expression: String,
        /// The error message.
        message: String,
    },

    /// Failed to parse configuration.
    ConfigError {
        /// The error message.
        message: String,
    },

    /// A required variable was not found in the context.
    MissingVariable {
        /// The variable name.
        name: String,
    },

    /// Type conversion error.
    TypeError {
        /// Expected type.
        expected: String,
        /// Actual type.
        actual: String,
    },

    /// Failed to load rules file.
    IoError {
        /// The error message.
        message: String,
    },
}

impl fmt::Display for CelError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::CompileError {
                expression,
                message,
            } => {
                write!(
                    f,
                    "failed to compile CEL expression '{expression}': {message}"
                )
            }
            Self::EvaluationError {
                expression,
                message,
            } => {
                write!(
                    f,
                    "failed to evaluate CEL expression '{expression}': {message}"
                )
            }
            Self::ConfigError { message } => {
                write!(f, "configuration error: {message}")
            }
            Self::MissingVariable { name } => {
                write!(f, "missing variable in context: {name}")
            }
            Self::TypeError { expected, actual } => {
                write!(f, "type error: expected {expected}, got {actual}")
            }
            Self::IoError { message } => {
                write!(f, "I/O error: {message}")
            }
        }
    }
}

impl std::error::Error for CelError {}

impl From<std::io::Error> for CelError {
    fn from(err: std::io::Error) -> Self {
        Self::IoError {
            message: err.to_string(),
        }
    }
}
