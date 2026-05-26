//! Rego input conversion traits and helpers.

use serde::Serialize;
use serde_json::Value;
pub use crate::traits::IntoRegoInput;

/// Blanket implementation for Serialize types.
impl<T: Serialize> IntoRegoInput for T {
    fn into_rego_input(&self) -> Value {
        serde_json::to_value(self).unwrap_or(Value::Null)
    }
}

/// Helper for building Rego input from key-value pairs.
#[derive(Debug, Default)]
pub struct InputBuilder {
    data: serde_json::Map<String, Value>,
}

impl InputBuilder {
    /// Create a new input builder.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a string field.
    #[must_use]
    pub fn with_string(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        let _ = self.data.insert(key.into(), Value::String(value.into()));
        self
    }

    /// Add an integer field.
    #[must_use]
    pub fn with_int(mut self, key: impl Into<String>, value: i64) -> Self {
        let _ = self.data.insert(key.into(), Value::Number(value.into()));
        self
    }

    /// Add a float field.
    #[must_use]
    pub fn with_float(mut self, key: impl Into<String>, value: f64) -> Self {
        if let Some(n) = serde_json::Number::from_f64(value) {
            let _ = self.data.insert(key.into(), Value::Number(n));
        }
        self
    }

    /// Add a boolean field.
    #[must_use]
    pub fn with_bool(mut self, key: impl Into<String>, value: bool) -> Self {
        let _ = self.data.insert(key.into(), Value::Bool(value));
        self
    }

    /// Add a nested object.
    #[must_use]
    pub fn with_object(mut self, key: impl Into<String>, value: Value) -> Self {
        let _ = self.data.insert(key.into(), value);
        self
    }

    /// Build the input value.
    #[must_use]
    pub fn build(self) -> Value {
        Value::Object(self.data)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_input_builder() {
        let input = InputBuilder::new()
            .with_string("name", "test")
            .with_int("count", 42)
            .with_float("ratio", 1.5)
            .with_bool("active", true)
            .build();

        assert_eq!(input["name"], "test");
        assert_eq!(input["count"], 42);
        assert!(input["active"].as_bool().unwrap_or(false));
    }
}
