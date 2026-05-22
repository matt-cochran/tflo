//! Golden vector validation logic.

/// Validation result for a golden vector test.
#[derive(Debug, Clone)]
pub struct ValidationResult {
    /// Whether the test passed
    pub passed: bool,
    /// Number of samples compared
    pub samples_compared: usize,
    /// Number of samples that matched
    pub samples_matched: usize,
    /// Maximum absolute difference found
    pub max_diff: f64,
    /// Mean absolute difference
    pub mean_diff: f64,
    /// Detailed differences (first 10 mismatches)
    pub mismatches: Vec<Mismatch>,
}

/// Details about a single mismatch.
#[derive(Debug, Clone)]
pub struct Mismatch {
    /// Index in the output array
    pub index: usize,
    /// Expected value
    pub expected: f64,
    /// Actual value
    pub actual: f64,
    /// Absolute difference
    pub diff: f64,
}

/// Validates computed results against expected golden vector output (single-output).
pub fn validate(
    expected: &[Option<f64>],
    actual: &[Option<f64>],
    tolerance: f64,
) -> ValidationResult {
    let mut samples_compared = 0;
    let mut samples_matched = 0;
    let mut max_diff = 0.0;
    let mut sum_diff = 0.0;
    let mut mismatches = Vec::new();

    let min_len = expected.len().min(actual.len());

    for i in 0..min_len {
        match (expected.get(i), actual.get(i)) {
            (Some(Some(expected_val)), Some(Some(actual_val))) => {
                samples_compared += 1;
                
                // Handle NaN: NaN == NaN is false, but we want to treat them as matching
                let both_nan = expected_val.is_nan() && actual_val.is_nan();
                let diff = if both_nan {
                    0.0
                } else {
                    (expected_val - actual_val).abs()
                };
                
                sum_diff += diff;
                max_diff = f64::max(max_diff, diff);

                if diff <= tolerance || both_nan {
                    samples_matched += 1;
                } else if mismatches.len() < 10 {
                    mismatches.push(Mismatch {
                        index: i,
                        expected: *expected_val,
                        actual: *actual_val,
                        diff,
                    });
                }
            }
            (Some(None), Some(None)) => {
                // Both None (warmup) - this is a match
                samples_compared += 1;
                samples_matched += 1;
            }
            (Some(None), Some(Some(actual_val))) => {
                // Expected None (from JSON null), actual is Some(value)
                // If actual is NaN, this is a match (JSON can't represent NaN, so it becomes null)
                samples_compared += 1;
                if actual_val.is_nan() {
                    samples_matched += 1;
                } else if mismatches.len() < 10 {
                    mismatches.push(Mismatch {
                        index: i,
                        expected: 0.0, // None represented as 0 for display
                        actual: *actual_val,
                        diff: f64::INFINITY,
                    });
                }
            }
            (Some(Some(_)), Some(None)) => {
                // Expected Some(value), actual is None - this is a mismatch
                samples_compared += 1;
                if mismatches.len() < 10 {
                    mismatches.push(Mismatch {
                        index: i,
                        expected: expected[i].unwrap_or(0.0),
                        actual: 0.0, // None represented as 0 for display
                        diff: f64::INFINITY,
                    });
                }
            }
            _ => {
                // Length mismatch - already handled by min_len
            }
        }
    }

    let mean_diff = if samples_compared > 0 {
        sum_diff / samples_compared as f64
    } else {
        0.0
    };

    let passed = samples_compared > 0 && samples_matched == samples_compared;

    ValidationResult {
        passed,
        samples_compared,
        samples_matched,
        max_diff,
        mean_diff,
        mismatches,
    }
}

/// Validates computed multi-output results against expected golden vector output.
pub fn validate_multi_output(
    expected: &[Vec<Option<f64>>],
    actual: &[Vec<Option<f64>>],
    tolerance: f64,
) -> ValidationResult {
    if expected.len() != actual.len() {
        return ValidationResult {
            passed: false,
            samples_compared: 0,
            samples_matched: 0,
            max_diff: f64::INFINITY,
            mean_diff: f64::INFINITY,
            mismatches: vec![Mismatch {
                index: 0,
                expected: 0.0,
                actual: 0.0,
                diff: f64::INFINITY,
            }],
        };
    }

    let num_outputs = expected.len();
    let mut all_samples_compared = 0;
    let mut all_samples_matched = 0;
    let mut max_diff = 0.0;
    let mut sum_diff = 0.0;
    let mut mismatches = Vec::new();

    for output_idx in 0..num_outputs {
        let result = validate(&expected[output_idx], &actual[output_idx], tolerance);
        all_samples_compared += result.samples_compared;
        all_samples_matched += result.samples_matched;
        max_diff = f64::max(max_diff, result.max_diff);
        sum_diff += result.mean_diff * result.samples_compared as f64;
        
        // Add mismatches with output index context
        for mismatch in result.mismatches {
            if mismatches.len() < 10 {
                mismatches.push(mismatch);
            }
        }
    }

    let mean_diff = if all_samples_compared > 0 {
        sum_diff / all_samples_compared as f64
    } else {
        0.0
    };

    let passed = all_samples_compared > 0 && all_samples_matched == all_samples_compared;

    ValidationResult {
        passed,
        samples_compared: all_samples_compared,
        samples_matched: all_samples_matched,
        max_diff,
        mean_diff,
        mismatches,
    }
}

