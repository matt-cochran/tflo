//! CEL context conversion traits and helpers.

pub use crate::traits::IntoCelContext;
use cel_interpreter::{Context, Value};
use std::collections::HashMap;

/// Helper struct for building CEL contexts from key-value pairs.
#[derive(Debug, Default)]
pub struct ContextBuilder {
    variables: HashMap<String, Value>,
}

impl ContextBuilder {
    /// Create a new context builder.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a string variable.
    #[must_use]
    pub fn with_string(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        let _ = self
            .variables
            .insert(name.into(), Value::String(value.into().into()));
        self
    }

    /// Add an integer variable.
    #[must_use]
    pub fn with_int(mut self, name: impl Into<String>, value: i64) -> Self {
        let _ = self.variables.insert(name.into(), Value::Int(value));
        self
    }

    /// Add an unsigned integer variable.
    #[must_use]
    pub fn with_uint(mut self, name: impl Into<String>, value: u64) -> Self {
        let _ = self.variables.insert(name.into(), Value::UInt(value));
        self
    }

    /// Add a float variable.
    #[must_use]
    pub fn with_float(mut self, name: impl Into<String>, value: f64) -> Self {
        let _ = self.variables.insert(name.into(), Value::Float(value));
        self
    }

    /// Add a boolean variable.
    #[must_use]
    pub fn with_bool(mut self, name: impl Into<String>, value: bool) -> Self {
        let _ = self.variables.insert(name.into(), Value::Bool(value));
        self
    }

    /// Build the CEL context.
    #[must_use]
    pub fn build(self) -> Context<'static> {
        let mut ctx = Context::default();
        for (name, value) in self.variables {
            // CEL context expects references, but we need to work around lifetime issues
            // by using the add_variable method which copies values
            // Conversions from primitive types are infallible; the Result
            // is only there for fallible custom types we don't pass.
            match value {
                Value::String(s) => {
                    drop(ctx.add_variable(&name, s.to_string()));
                }
                Value::Int(i) => {
                    drop(ctx.add_variable(&name, i));
                }
                Value::UInt(u) => {
                    drop(ctx.add_variable(&name, u as i64));
                }
                Value::Float(f) => {
                    drop(ctx.add_variable(&name, f));
                }
                Value::Bool(b) => {
                    drop(ctx.add_variable(&name, b));
                }
                // List/Map/Function/Bytes/Null are not representable as
                // primitive CEL scope values via this builder; skip them.
                // Listed explicitly so a future cel-interpreter variant
                // addition surfaces in CI. (Duration/Timestamp variants only
                // exist under cel-interpreter's `chrono` feature, which we
                // disable for size — re-enabling it will reintroduce them here
                // as a non-exhaustive-match error, by design.)
                Value::List(_)
                | Value::Map(_)
                | Value::Function(_, _)
                | Value::Bytes(_)
                | Value::Null => {}
            }
        }
        ctx
    }
}

/// Implement `IntoCelContext` for HashMap-based contexts.
impl IntoCelContext for HashMap<String, serde_json::Value> {
    fn into_cel_context(&self) -> Context<'static> {
        let mut ctx = Context::default();
        for (key, value) in self {
            // Conversions from primitive types are infallible; the Result
            // is only there for fallible custom types we don't pass.
            match value {
                serde_json::Value::String(s) => {
                    drop(ctx.add_variable(key, s.clone()));
                }
                serde_json::Value::Number(n) => {
                    if let Some(i) = n.as_i64() {
                        drop(ctx.add_variable(key, i));
                    } else if let Some(f) = n.as_f64() {
                        drop(ctx.add_variable(key, f));
                    }
                }
                serde_json::Value::Bool(b) => {
                    drop(ctx.add_variable(key, *b));
                }
                // Null/Array/Object don't map to primitive CEL values;
                // listed explicitly so a future serde_json variant
                // would surface in CI.
                serde_json::Value::Null
                | serde_json::Value::Array(_)
                | serde_json::Value::Object(_) => {}
            }
        }
        ctx
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_context_builder() {
        let ctx = ContextBuilder::new()
            .with_string("name", "test")
            .with_int("count", 42)
            .with_float("ratio", 1.5)
            .with_bool("active", true)
            .build();

        // Context is built - we can't easily verify internals
        // but we can check it doesn't panic
        drop(ctx);
    }
}
