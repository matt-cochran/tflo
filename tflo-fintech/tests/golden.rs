#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
// The golden-vector scaffolding carries diagnostic fields/methods that not
// every test path reads.
#![allow(dead_code)]
//! Golden vector integration tests for `tflo-core`.
//!
//! Validates `tflo-core` graph implementations against TA-Lib reference outputs.
//! Tests are under `tests/golden/` as a module tree using `#[path]` declarations.

#[path = "golden/runner.rs"]
mod runner;
#[path = "golden/tests.rs"]
mod tests;
#[path = "golden/validator.rs"]
mod validator;
#[path = "golden/vector.rs"]
mod vector;

// Re-export everything the moved modules need
use runner::GoldenRunner;
use validator::{validate, validate_multi_output};
use vector::GoldenVector;

/// Error types for golden vector operations
#[derive(Debug, thiserror::Error)]
pub enum GoldenError {
    /// IO error.
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    /// JSON parse error.
    #[error("JSON parse error: {0}")]
    Json(#[from] serde_json::Error),
    /// CSV parse error.
    #[error("CSV parse error: {0}")]
    Csv(#[from] csv::Error),
    /// Validation error.
    #[error("Validation error: {0}")]
    Validation(String),
    /// Unsupported indicator type.
    #[error("Unsupported indicator: {0}")]
    UnsupportedIndicator(String),
}
