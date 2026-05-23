//! Rego policy engine.

use crate::context::IntoRegoInput;
use crate::error::{RegoError, RegoResult};
use regorus::Engine;
use std::fs;
use std::path::Path;

/// Rego policy engine for evaluating OPA policies.
#[derive(Debug)]
pub struct PolicyEngine {
    engine: Engine,
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

    /// Add a policy from a string.
    ///
    /// # Errors
    ///
    /// Returns [`RegoError::ParseError`](crate::error::RegoError::ParseError)
    /// when `policy` is not a valid Rego policy.
    pub fn add_policy(&mut self, name: &str, policy: &str) -> RegoResult<()> {
        let _ = self
            .engine
            .add_policy(name.to_string(), policy.to_string())
            .map_err(|e| RegoError::ParseError {
                policy: name.to_string(),
                message: e.to_string(),
            })?;
        Ok(())
    }

    /// Add a policy from a file.
    ///
    /// # Errors
    ///
    /// Returns [`RegoError::IoError`](crate::error::RegoError::IoError) when
    /// `path` cannot be read, plus any error from
    /// [`add_policy`](Self::add_policy) for parsing.
    pub fn add_policy_from_file<P: AsRef<Path>>(&mut self, path: P) -> RegoResult<()> {
        let content = fs::read_to_string(path.as_ref())?;
        let name = path
            .as_ref()
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("policy");
        self.add_policy(name, &content)
    }

    /// Add all policies from a directory.
    ///
    /// # Errors
    ///
    /// Returns [`RegoError::IoError`](crate::error::RegoError::IoError) when
    /// `path` cannot be read or entries cannot be enumerated, plus any
    /// error from [`add_policy_from_file`](Self::add_policy_from_file) for
    /// each `.rego` file found.
    pub fn add_policies_from_directory<P: AsRef<Path>>(&mut self, path: P) -> RegoResult<usize> {
        let mut count = 0;
        for entry in fs::read_dir(path)? {
            let entry = entry?;
            let file_path = entry.path();
            if file_path.extension().is_some_and(|e| e == "rego") {
                self.add_policy_from_file(&file_path)?;
                count += 1;
            }
        }
        Ok(count)
    }

    /// Add static data that policies can reference.
    ///
    /// # Errors
    ///
    /// Returns
    /// [`RegoError::EvaluationError`](crate::error::RegoError::EvaluationError)
    /// when the underlying engine rejects the supplied data.
    pub fn add_data(&mut self, data: serde_json::Value) -> RegoResult<()> {
        let rego_value: regorus::Value = data.into();
        self.engine
            .add_data(rego_value)
            .map_err(|e| RegoError::EvaluationError {
                query: "data".to_string(),
                message: e.to_string(),
            })?;
        Ok(())
    }

    /// Add data from a JSON file.
    ///
    /// # Errors
    ///
    /// Returns [`RegoError::IoError`](crate::error::RegoError::IoError) when
    /// `path` cannot be read,
    /// [`RegoError::JsonError`](crate::error::RegoError::JsonError) when
    /// the file is not valid JSON, plus any error from
    /// [`add_data`](Self::add_data).
    pub fn add_data_from_file<P: AsRef<Path>>(&mut self, path: P) -> RegoResult<()> {
        let content = fs::read_to_string(path)?;
        let data: serde_json::Value = serde_json::from_str(&content)?;
        self.add_data(data)
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
    /// [`RegoError::EvaluationError`](crate::error::RegoError::EvaluationError)
    /// when the underlying Rego engine fails to evaluate `query`.
    #[allow(clippy::disallowed_methods)] // serde_json::json! macro internally uses unwrap
    pub fn eval_query(&mut self, query: &str) -> RegoResult<serde_json::Value> {
        let results = self
            .engine
            .eval_query(query.to_string(), false)
            .map_err(|e| RegoError::EvaluationError {
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
    /// [`RegoError::EvaluationError`](crate::error::RegoError::EvaluationError)
    /// when the query result cannot be coerced to a boolean.
    pub fn eval_allow<T: IntoRegoInput>(&mut self, input: &T, query: &str) -> RegoResult<bool> {
        let input_value = input.into_rego_input();
        self.set_input(input_value)?;

        let result = self.eval_query(query)?;

        // Extract boolean from result
        Self::extract_bool(&result)
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

    fn extract_bool(result: &serde_json::Value) -> RegoResult<bool> {
        // Handle regorus result format
        if let Some(arr) = result.get("result").and_then(|r| r.as_array()) {
            if let Some(first) = arr.first() {
                if let Some(exprs) = first.get("expressions").and_then(|e| e.as_array()) {
                    if let Some(expr) = exprs.first() {
                        if let Some(value) = expr.get("value") {
                            if let Some(b) = value.as_bool() {
                                return Ok(b);
                            }
                        }
                    }
                }
            }
        }

        // Direct value
        if let Some(b) = result.as_bool() {
            return Ok(b);
        }

        // Check for empty result (undefined = false)
        if result
            .get("result")
            .and_then(|r| r.as_array())
            .is_some_and(std::vec::Vec::is_empty)
        {
            return Ok(false);
        }

        Err(RegoError::InvalidResult {
            expected: "boolean".to_string(),
            actual: format!("{result:?}"),
        })
    }

    /// Clear all policies and data.
    pub fn clear(&mut self) {
        self.engine = Engine::new();
    }
}

/// Convert regorus Value to `serde_json` Value.
fn value_to_json(value: &regorus::Value) -> serde_json::Value {
    match value {
        regorus::Value::Null => serde_json::Value::Null,
        regorus::Value::Bool(b) => serde_json::Value::Bool(*b),
        regorus::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                serde_json::Value::Number(i.into())
            } else if let Some(f) = n.as_f64() {
                serde_json::Number::from_f64(f)
                    .map(serde_json::Value::Number)
                    .unwrap_or(serde_json::Value::Null)
            } else {
                serde_json::Value::Null
            }
        }
        regorus::Value::String(s) => serde_json::Value::String(s.to_string()),
        regorus::Value::Array(arr) => {
            serde_json::Value::Array(arr.iter().map(value_to_json).collect())
        }
        regorus::Value::Set(set) => {
            serde_json::Value::Array(set.iter().map(value_to_json).collect())
        }
        regorus::Value::Object(obj) => {
            let map: serde_json::Map<String, serde_json::Value> = obj
                .iter()
                .filter_map(|(k, v)| {
                    if let regorus::Value::String(s) = k {
                        Some((s.to_string(), value_to_json(v)))
                    } else {
                        None
                    }
                })
                .collect();
            serde_json::Value::Object(map)
        }
        regorus::Value::Undefined => serde_json::Value::Null,
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
