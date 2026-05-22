//! Rhai scope conversion traits and helpers.

use rhai::{Dynamic, Scope};
use std::collections::HashMap;

/// Trait for types that can be converted to a Rhai scope.
///
/// Implement this trait for your domain types to enable Rhai scripting.
///
/// # Examples
///
/// ```rust
/// use tflo_rhai::context::IntoRhaiScope;
/// use rhai::Scope;
///
/// struct Detection {
///     ts: i64,
///     freq_hz: u64,
///     power_dbm: f64,
///     snr_db: f64,
/// }
///
/// impl IntoRhaiScope for Detection {
///     fn into_rhai_scope(&self) -> Scope<'static> {
///         let mut scope = Scope::new();
///         scope.push("ts", self.ts);
///         scope.push("freq_hz", self.freq_hz as i64);
///         scope.push("freq_mhz", self.freq_hz as f64 / 1e6);
///         scope.push("power", self.power_dbm);
///         scope.push("snr", self.snr_db);
///         scope
///     }
/// }
/// ```
pub trait IntoRhaiScope {
    /// Convert this value into a Rhai scope for evaluation.
    fn into_rhai_scope(&self) -> Scope<'static>;
}

/// Trait for types that can be converted to a Rhai Dynamic value.
pub trait IntoRhaiDynamic {
    /// Convert this value into a Rhai Dynamic.
    fn into_rhai_dynamic(&self) -> Dynamic;
}

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
                _ => {}
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
            .with_float("ratio", 3.14)
            .with_bool("active", true)
            .build();

        assert_eq!(scope.len(), 4);
    }
}
