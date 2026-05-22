//! Golden vector data structures and loading.

use serde::{Deserialize, Serialize};
use std::path::Path;

/// Golden vector test case.
///
/// Contains input data, expected output, and metadata for validating
/// indicator implementations.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GoldenVector {
    /// Metadata about the vector (source, version, license, etc.)
    pub metadata: VectorMetadata,
    /// Indicator parameters
    pub params: serde_json::Value,
    /// Input time series data
    pub input: Vec<f64>,
    /// Expected output (null values indicate warmup period)
    ///
    /// For single-output indicators: JSON array of numbers/null
    /// For multi-output indicators: JSON array of arrays (e.g., MACD has 3 outputs)
    pub expected_output: serde_json::Value,
    /// Number of samples needed for warmup
    pub warmup_samples: usize,
}

/// Metadata about a golden vector.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VectorMetadata {
    /// Indicator name (e.g., "rsi_tv_count", "ema_talib_count")
    pub indicator: String,
    /// Source of reference data ("tradingview", "talib", etc.)
    pub source: String,
    /// Version of the source tool/library
    pub version: String,
    /// License for the reference data
    pub license: String,
    /// Provenance information (how the data was generated)
    pub provenance: String,
}

impl GoldenVector {
    /// Load a golden vector from a JSON file.
    pub fn load<P: AsRef<Path>>(path: P) -> Result<Self, super::GoldenError> {
        let content = std::fs::read_to_string(path)?;
        let vector: GoldenVector = serde_json::from_str(&content)?;
        Ok(vector)
    }

    /// Save a golden vector to a JSON file.
    pub fn save<P: AsRef<Path>>(&self, path: P) -> Result<(), super::GoldenError> {
        let content = serde_json::to_string_pretty(self)?;
        std::fs::write(path, content)?;
        Ok(())
    }

    /// Validate that input and expected_output have compatible lengths.
    pub fn validate_structure(&self) -> Result<(), super::GoldenError> {
        // Allow empty expected_output during generation
        if let Some(arr) = self.expected_output.as_array() {
            if arr.is_empty() {
                return Ok(());
            }
            // Check if it's a multi-output empty array
            if !arr.is_empty() && arr[0].is_array() {
                if let Some(first_output) = arr[0].as_array() {
                    if first_output.is_empty() {
                        return Ok(());
                    }
                }
            }
        }

        let output_len = self.expected_output_len()?;
        if self.input.len() != output_len {
            return Err(super::GoldenError::Validation(format!(
                "Input length {} does not match expected_output length {}",
                self.input.len(),
                output_len
            )));
        }
        Ok(())
    }

    /// Get the length of expected_output (for single-output indicators).
    pub fn expected_output_len(&self) -> Result<usize, super::GoldenError> {
        if let Some(arr) = self.expected_output.as_array() {
            // Check if it's a multi-output (array of arrays) or single-output (array of values)
            if arr.is_empty() {
                return Ok(0);
            }
            if arr[0].is_array() {
                // Multi-output: length is the length of the first output array
                if let Some(first_output) = arr[0].as_array() {
                    return Ok(first_output.len());
                }
            } else {
                // Single-output: length is the array length
                return Ok(arr.len());
            }
        }
        // If not an array, return 0 (for empty/null during generation)
        Ok(0)
    }

    /// Extract single-output expected values.
    pub fn expected_output_single(&self) -> Result<Vec<Option<f64>>, super::GoldenError> {
        if let Some(arr) = self.expected_output.as_array() {
            let mut result = Vec::new();
            for val in arr {
                if val.is_null() {
                    result.push(None);
                } else if let Some(num) = val.as_f64() {
                    result.push(Some(num));
                } else {
                    return Err(super::GoldenError::Validation(
                        "expected_output contains non-numeric, non-null values".to_string(),
                    ));
                }
            }
            Ok(result)
        } else {
            Err(super::GoldenError::Validation(
                "expected_output must be a JSON array for single-output indicators".to_string(),
            ))
        }
    }

    /// Extract multi-output expected values.
    pub fn expected_output_multi(&self) -> Result<Vec<Vec<Option<f64>>>, super::GoldenError> {
        if let Some(arr) = self.expected_output.as_array() {
            if arr.is_empty() {
                return Ok(Vec::new());
            }
            let num_outputs = arr.len();
            let mut results: Vec<Vec<Option<f64>>> = vec![Vec::new(); num_outputs];

            for output_idx in 0..num_outputs {
                if let Some(output_arr) = arr[output_idx].as_array() {
                    for val in output_arr {
                        if val.is_null() {
                            results[output_idx].push(None);
                        } else if let Some(num) = val.as_f64() {
                            results[output_idx].push(Some(num));
                        } else {
                            return Err(super::GoldenError::Validation(
                                "expected_output contains non-numeric, non-null values".to_string(),
                            ));
                        }
                    }
                } else {
                    return Err(super::GoldenError::Validation(
                        "expected_output must be an array of arrays for multi-output indicators"
                            .to_string(),
                    ));
                }
            }
            Ok(results)
        } else {
            Err(super::GoldenError::Validation(
                "expected_output must be a JSON array for multi-output indicators".to_string(),
            ))
        }
    }

    /// Check if this is a multi-output indicator.
    pub fn is_multi_output(&self) -> bool {
        if let Some(arr) = self.expected_output.as_array() {
            !arr.is_empty() && arr[0].is_array()
        } else {
            false
        }
    }
}
