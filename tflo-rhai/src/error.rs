//! Error types for tflow-rhai.

use std::fmt;

/// Result type alias using [`RhaiError`].
pub type RhaiResult<T> = Result<T, RhaiError>;

/// Errors that can occur during Rhai operations.
#[derive(Debug, Clone)]
pub enum RhaiError {
    /// Failed to compile a Rhai script.
    CompileError {
        /// The script that failed to compile.
        script: String,
        /// The error message.
        message: String,
    },

    /// Failed to evaluate a Rhai script.
    EvaluationError {
        /// The script that failed.
        script: String,
        /// The error message.
        message: String,
    },

    /// Type conversion error.
    TypeError {
        /// Expected type.
        expected: String,
        /// Actual type.
        actual: String,
    },

    /// Failed to load script file.
    IoError {
        /// The error message.
        message: String,
    },

    /// Script returned an error.
    ScriptError {
        /// The error message from the script.
        message: String,
    },
}

impl fmt::Display for RhaiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::CompileError { script, message } => {
                write!(f, "failed to compile Rhai script '{script}': {message}")
            }
            Self::EvaluationError { script, message } => {
                write!(f, "failed to evaluate Rhai script '{script}': {message}")
            }
            Self::TypeError { expected, actual } => {
                write!(f, "type error: expected {expected}, got {actual}")
            }
            Self::IoError { message } => {
                write!(f, "I/O error: {message}")
            }
            Self::ScriptError { message } => {
                write!(f, "script error: {message}")
            }
        }
    }
}

impl std::error::Error for RhaiError {}

impl From<std::io::Error> for RhaiError {
    fn from(err: std::io::Error) -> Self {
        Self::IoError {
            message: err.to_string(),
        }
    }
}

impl From<Box<rhai::EvalAltResult>> for RhaiError {
    fn from(err: Box<rhai::EvalAltResult>) -> Self {
        Self::EvaluationError {
            script: String::new(),
            message: err.to_string(),
        }
    }
}

impl From<rhai::ParseError> for RhaiError {
    fn from(err: rhai::ParseError) -> Self {
        Self::CompileError {
            script: String::new(),
            message: err.to_string(),
        }
    }
}
