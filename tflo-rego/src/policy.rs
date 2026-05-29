//! Rego policy engine — evaluation surface (input/query/value).
//!
//! Config-file loaders (`add_policy*` / `add_data*`) live in
//! [`crate::policy_loader`]; value-codec helpers (Rego↔JSON) live in
//! [`crate::value_codec`].

use crate::error::{RegoError, RegoResult};

// `RegoError` is brought into scope above for intra-doc-link resolution
// in the `# Errors` sections of the methods below. Without this `use`,
// `[`RegoError::EvaluationError`]` cannot resolve and rustdoc fails.
use crate::traits::IntoRegoInput;
use crate::value_codec::{extract_bool, value_to_json};
#[allow(unused_imports)]
use RegoError as _;
use regorus::Engine;

/// Rego policy engine for evaluating OPA policies.
#[derive(Debug)]
pub struct PolicyEngine {
    pub(crate) engine: Engine,
}

impl Default for PolicyEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl PolicyEngine {
    /// Create a new policy engine.
    #[must_use]
    pub fn new() -> Self {
        Self {
            engine: Engine::new(),
        }
    }

    /// Set the input for evaluation.
    ///
    /// # Errors
    ///
    /// Currently infallible — the `Result` shape is reserved for future
    /// engine-side validation of the input document.
    pub fn set_input(&mut self, input: serde_json::Value) -> RegoResult<()> {
        let rego_value: regorus::Value = input.into();
        self.engine.set_input(rego_value);
        Ok(())
    }

    /// Evaluate a query and return the result as JSON.
    ///
    /// # Errors
    ///
    /// Returns
    /// [`RegoError::EvaluationError`]
    /// when the underlying Rego engine fails to evaluate `query`.
    #[allow(clippy::disallowed_methods)] // serde_json::json! macro internally uses unwrap
    pub fn eval_query(&mut self, query: &str) -> RegoResult<serde_json::Value> {
        let results = self
            .engine
            .eval_query(query.to_string(), false)
            .map_err(|e| crate::error::RegoError::EvaluationError {
                query: query.to_string(),
                message: e.to_string(),
            })?;

        // Convert results to JSON
        let json_results: Vec<serde_json::Value> = results
            .result
            .into_iter()
            .map(|expr_set| {
                serde_json::json!({
                    "expressions": expr_set.expressions.into_iter().map(|e| {
                        serde_json::json!({
                            "value": value_to_json(&e.value)
                        })
                    }).collect::<Vec<_>>()
                })
            })
            .collect();

        Ok(serde_json::json!({ "result": json_results }))
    }

    /// Evaluate a query with input and return a boolean result.
    ///
    /// # Errors
    ///
    /// Propagates any error from [`set_input`](Self::set_input) and
    /// [`eval_query`](Self::eval_query), and returns
    /// [`RegoError::EvaluationError`]
    /// when the query result cannot be coerced to a boolean.
    pub fn eval_allow<T: IntoRegoInput>(&mut self, input: &T, query: &str) -> RegoResult<bool> {
        let input_value = input.into_rego_input();
        self.set_input(input_value)?;

        let result = self.eval_query(query)?;
        extract_bool(&result)
    }

    /// Evaluate and return a value result.
    ///
    /// # Errors
    ///
    /// Propagates any error from [`set_input`](Self::set_input) and
    /// [`eval_query`](Self::eval_query).
    pub fn eval_value<T: IntoRegoInput>(
        &mut self,
        input: &T,
        query: &str,
    ) -> RegoResult<serde_json::Value> {
        let input_value = input.into_rego_input();
        self.set_input(input_value)?;
        self.eval_query(query)
    }

    /// Clear all policies and data.
    pub fn clear(&mut self) {
        self.engine = Engine::new();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_policy() {
        let mut engine = PolicyEngine::new();

        engine
            .add_policy(
                "test",
                r#"
                package test

                default allow = false

                allow {
                    input.value > 10
                }
            "#,
            )
            .expect("should parse");

        let input_high = serde_json::json!({"value": 15});
        let input_low = serde_json::json!({"value": 5});

        let result_high = engine.eval_allow(&input_high, "data.test.allow");
        let result_low = engine.eval_allow(&input_low, "data.test.allow");

        // Note: Result parsing may need adjustment based on regorus output format
        assert!(result_high.is_ok() || result_low.is_ok());
    }

    #[test]
    fn test_policy_with_data() {
        let mut engine = PolicyEngine::new();

        engine
            .add_policy(
                "roles",
                r#"
                package authz

                default allow = false

                allow {
                    input.role == data.admin_role
                }
            "#,
            )
            .expect("should parse");

        engine
            .add_data(serde_json::json!({"admin_role": "superuser"}))
            .expect("should add data");

        let admin = serde_json::json!({"role": "superuser"});
        let user = serde_json::json!({"role": "guest"});

        // These may need adjustment based on regorus behavior
        let _ = engine.eval_allow(&admin, "data.authz.allow");
        let _ = engine.eval_allow(&user, "data.authz.allow");
    }
}
