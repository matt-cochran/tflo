//! Rhai scope conversion traits and helpers.

use rhai::{Dynamic, Scope};
use std::collections::HashMap;
pub use crate::traits::{IntoRhaiDynamic, IntoRhaiScope};

/// Helper struct for building Rhai scopes.
#[derive(Debug, Default)]
pub struct ScopeBuilder {
    scope: Scope<'static>,
}

impl ScopeBuilder {
    /// Create a new scope builder.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a string variable.
    #[must_use]
    pub fn with_string(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        let _ = self.scope.push(name.into(), value.into());
        self
    }

    /// Add an integer variable.
    #[must_use]
    pub fn with_int(mut self, name: impl Into<String>, value: i64) -> Self {
        let _ = self.scope.push(name.into(), value);
        self
    }

    /// Add a float variable.
    #[must_use]
    pub fn with_float(mut self, name: impl Into<String>, value: f64) -> Self {
        let _ = self.scope.push(name.into(), value);
        self
    }

    /// Add a boolean variable.
    #[must_use]
    pub fn with_bool(mut self, name: impl Into<String>, value: bool) -> Self {
        let _ = self.scope.push(name.into(), value);
        self
    }

    /// Add a Dynamic value.
    #[must_use]
    pub fn with_dynamic(mut self, name: impl Into<String>, value: Dynamic) -> Self {
        let _ = self.scope.push_dynamic(name.into(), value);
        self
    }

    /// Build the scope.
    #[must_use]
    pub fn build(self) -> Scope<'static> {
        self.scope
    }
}

/// Implement `IntoRhaiScope` for HashMap-based data.
impl IntoRhaiScope for HashMap<String, serde_json::Value> {
    fn into_rhai_scope(&self) -> Scope<'static> {
        let mut scope = Scope::new();
        for (key, value) in self {
            match value {
                serde_json::Value::String(s) => {
                    let _ = scope.push(key.clone(), s.clone());
                }
                serde_json::Value::Number(n) => {
                    if let Some(i) = n.as_i64() {
                        let _ = scope.push(key.clone(), i);
                    } else if let Some(f) = n.as_f64() {
                        let _ = scope.push(key.clone(), f);
                    }
                }
                serde_json::Value::Bool(b) => {
                    let _ = scope.push(key.clone(), *b);
                }
                // Null, Array, Object are not representable as primitive
                // Rhai scope values; skip them. `serde_json::Value` is
                // marked `#[non_exhaustive]`-by-convention so use an
                // explicit `Null|Array|Object` to keep this match
                // honest if upstream ever adds a variant we should think
                // about.
                serde_json::Value::Null
                | serde_json::Value::Array(_)
                | serde_json::Value::Object(_) => {}
            }
        }
        scope
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scope_builder() {
        let scope = ScopeBuilder::new()
            .with_string("name", "test")
            .with_int("count", 42)
            .with_float("ratio", 1.5)
            .with_bool("active", true)
            .build();

        assert_eq!(scope.len(), 4);
    }
}
