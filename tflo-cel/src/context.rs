//! CEL context conversion traits and helpers.

use cel_interpreter::{Context, Value};
use std::collections::HashMap;

/// Trait for types that can be converted to a CEL evaluation context.
///
/// Implement this trait for your domain types to enable CEL filtering
/// and rule evaluation.
///
/// # Examples
///
/// ```rust
/// use tflo_cel::context::IntoCelContext;
/// use cel_interpreter::Context;
///
/// struct Detection {
///     ts: i64,
///     freq_hz: u64,
///     power_dbm: f64,
///     snr_db: f64,
///     is_signal: bool,
/// }
///
/// impl IntoCelContext for Detection {
///     fn into_cel_context(&self) -> Context<'static> {
///         let mut ctx = Context::default();
///         ctx.add_variable("ts", self.ts).unwrap();
///         ctx.add_variable("freq_hz", self.freq_hz as i64).unwrap();
///         ctx.add_variable("freq_mhz", self.freq_hz as f64 / 1e6).unwrap();
///         ctx.add_variable("power", self.power_dbm).unwrap();
///         ctx.add_variable("snr", self.snr_db).unwrap();
///         ctx.add_variable("is_signal", self.is_signal).unwrap();
///         ctx
///     }
/// }
/// ```
pub trait IntoCelContext {
    /// Convert this value into a CEL context for evaluation.
    fn into_cel_context(&self) -> Context<'static>;
}

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
            match value {
                Value::String(s) => {
                    let _ = ctx.add_variable(&name, s.to_string());
                }
                Value::Int(i) => {
                    let _ = ctx.add_variable(&name, i);
                }
                Value::UInt(u) => {
                    let _ = ctx.add_variable(&name, u as i64);
                }
                Value::Float(f) => {
                    let _ = ctx.add_variable(&name, f);
                }
                Value::Bool(b) => {
                    let _ = ctx.add_variable(&name, b);
                }
                _ => {}
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
            match value {
                serde_json::Value::String(s) => {
                    let _ = ctx.add_variable(key, s.clone());
                }
                serde_json::Value::Number(n) => {
                    if let Some(i) = n.as_i64() {
                        let _ = ctx.add_variable(key, i);
                    } else if let Some(f) = n.as_f64() {
                        let _ = ctx.add_variable(key, f);
                    }
                }
                serde_json::Value::Bool(b) => {
                    let _ = ctx.add_variable(key, *b);
                }
                _ => {}
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
            .with_float("ratio", 3.14)
            .with_bool("active", true)
            .build();

        // Context is built - we can't easily verify internals
        // but we can check it doesn't panic
        drop(ctx);
    }
}
