//! Configuration types for policy loading.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Configuration for a policy bundle.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BundleConfig {
    /// Path to the bundle directory or file.
    pub path: String,

    /// Optional name override.
    #[serde(default)]
    pub name: Option<String>,

    /// Static data to load.
    #[serde(default)]
    pub data: HashMap<String, serde_json::Value>,
}

/// Configuration for the policy engine.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PolicyConfig {
    /// Policy bundles to load.
    #[serde(default)]
    pub bundles: Vec<BundleConfig>,

    /// Default static data.
    #[serde(default)]
    pub data: HashMap<String, serde_json::Value>,

    /// Query timeout in milliseconds.
    #[serde(default)]
    pub timeout_ms: Option<u64>,

    /// Whether to enable strict mode.
    #[serde(default)]
    pub strict: bool,
}

impl PolicyConfig {
    /// Create a new empty configuration.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a bundle path.
    #[must_use]
    pub fn with_bundle(mut self, path: impl Into<String>) -> Self {
        self.bundles.push(BundleConfig {
            path: path.into(),
            name: None,
            data: HashMap::new(),
        });
        self
    }

    /// Add static data.
    #[must_use]
    pub fn with_data(mut self, key: impl Into<String>, value: serde_json::Value) -> Self {
        let _ = self.data.insert(key.into(), value);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_builder() {
        let config = PolicyConfig::new()
            .with_bundle("policies/")
            .with_data("threshold", serde_json::json!(10.0));

        assert_eq!(config.bundles.len(), 1);
        assert!(config.data.contains_key("threshold"));
    }

    #[test]
    fn test_parse_config() {
        let yaml = r#"
bundles:
  - path: policies/main
    name: main
    data:
      version: "1.0"
      
data:
  admin_role: superuser
  
strict: true
"#;

        let config: PolicyConfig = serde_yaml::from_str(yaml).expect("should parse");
        assert_eq!(config.bundles.len(), 1);
        assert!(config.strict);
    }
}
