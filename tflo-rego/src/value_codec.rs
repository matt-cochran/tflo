//! Value-codec helpers — Rego↔JSON marshalling.
//!
//! Kept separate so the conversion surface evolves independently of the
//! policy engine's lifecycle and evaluation surface.

use crate::error::{RegoError, RegoResult};

/// Convert a regorus `Value` into a `serde_json::Value`.
pub(crate) fn value_to_json(value: &regorus::Value) -> serde_json::Value {
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

/// Extract a boolean from a regorus query result.
///
/// Handles the regorus result envelope shape
/// `{ "result": [ { "expressions": [ { "value": <bool> } ] } ] }`, falls
/// back to a direct boolean, and treats an empty result set as `false`
/// (Rego's "undefined" semantics).
///
/// # Errors
///
/// Returns [`RegoError::InvalidResult`] when the value cannot be coerced
/// to a boolean.
pub(crate) fn extract_bool(result: &serde_json::Value) -> RegoResult<bool> {
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
