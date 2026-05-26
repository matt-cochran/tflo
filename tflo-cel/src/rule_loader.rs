//! Config-file loaders for [`RuleEngine`] — YAML/JSON parsing, file I/O, hot reload.

use crate::config::RulesConfig;
use crate::error::{CelError, CelResult};
use crate::rule_engine::{CompiledRule, RuleEngine};
#[cfg(not(target_arch = "wasm32"))]
use std::fs;
#[cfg(not(target_arch = "wasm32"))]
use std::path::Path;

impl RuleEngine {
    /// Load rules from a YAML string.
    ///
    /// # Errors
    ///
    /// Returns [`CelError::ConfigError`](crate::error::CelError::ConfigError)
    /// when `yaml` cannot be deserialized into a [`RulesConfig`], and
    /// [`CelError::CompileError`](crate::error::CelError::CompileError) when
    /// any rule's condition is not a valid CEL expression.
    pub fn from_yaml(yaml: &str) -> CelResult<Self> {
        let config: RulesConfig =
            serde_yaml::from_str(yaml).map_err(|e| CelError::ConfigError {
                message: e.to_string(),
            })?;
        Self::from_config(config)
    }

    /// Load rules from a JSON string.
    ///
    /// # Errors
    ///
    /// Returns [`CelError::ConfigError`](crate::error::CelError::ConfigError)
    /// when `json` cannot be deserialized into a [`RulesConfig`], and
    /// [`CelError::CompileError`](crate::error::CelError::CompileError) when
    /// any rule's condition is not a valid CEL expression.
    pub fn from_json(json: &str) -> CelResult<Self> {
        let config: RulesConfig =
            serde_json::from_str(json).map_err(|e| CelError::ConfigError {
                message: e.to_string(),
            })?;
        Self::from_config(config)
    }

    /// Load rules from a file.
    ///
    /// Not available on wasm32 targets — use [`from_str()`](Self::from_str)
    /// with file contents loaded from JavaScript.
    ///
    /// # Errors
    ///
    /// Returns [`CelError::IoError`](crate::error::CelError::IoError) when
    /// `path` cannot be read, plus any error from
    /// [`from_yaml`](Self::from_yaml) /
    /// [`from_json`](Self::from_json) for parsing.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn from_file<P: AsRef<Path>>(path: P) -> CelResult<Self> {
        let content = fs::read_to_string(path.as_ref())?;
        let path_str = path.as_ref().to_string_lossy();

        if path_str.ends_with(".yaml") || path_str.ends_with(".yml") {
            Self::from_yaml(&content)
        } else if path_str.ends_with(".json") {
            Self::from_json(&content)
        } else {
            // Try YAML first, then JSON
            Self::from_yaml(&content).or_else(|_| Self::from_json(&content))
        }
    }

    /// Load rules from a string with explicit format.
    ///
    /// This is the wasm-compatible alternative to [`from_file()`](Self::from_file).
    /// Use it when you have already loaded the file contents as a string.
    ///
    /// # Arguments
    /// * `content` — The raw string content.
    /// * `format` — Either `"yaml"` or `"json"`.
    ///
    /// # Example
    ///
    /// ```rust
    /// use tflo_cel::rule_engine::RuleEngine;
    ///
    /// let yaml = r#"
    /// rules:
    ///   - name: high_value
    ///     condition: "value > 100"
    ///     action:
    ///       type: log
    /// "#;
    /// let engine = RuleEngine::from_str(yaml, "yaml").expect("valid yaml");
    /// assert_eq!(engine.rule_count(), 1);
    /// ```
    ///
    /// # Errors
    ///
    /// Returns [`CelError::ConfigError`](crate::error::CelError::ConfigError)
    /// when `format` is not `"yaml"`/`"yml"`/`"json"`, plus any error from
    /// [`from_yaml`](Self::from_yaml) /
    /// [`from_json`](Self::from_json) for parsing.
    pub fn from_str(content: &str, format: &str) -> CelResult<Self> {
        match format {
            "yaml" | "yml" => Self::from_yaml(content),
            "json" => Self::from_json(content),
            _ => Err(CelError::ConfigError {
                message: format!("unsupported format: '{format}'. Use 'yaml' or 'json'."),
            }),
        }
    }

    /// Create from a config struct.
    ///
    /// # Errors
    ///
    /// Returns [`CelError::CompileError`](crate::error::CelError::CompileError)
    /// when any rule in `config` has a condition that is not a valid CEL
    /// expression.
    pub fn from_config(config: RulesConfig) -> CelResult<Self> {
        let mut rules = Vec::with_capacity(config.rules.len());

        for rule_config in config.rules {
            let action = rule_config.action.into_action();
            let mut rule = CompiledRule::new(&rule_config.name, &rule_config.condition, action)?;

            if let Some(desc) = rule_config.description {
                rule = rule.with_description(desc);
            }
            if let Some(priority) = rule_config.priority {
                rule = rule.with_priority(priority);
            }
            if let Some(tags) = rule_config.tags {
                rule = rule.with_tags(tags);
            }

            rules.push(rule);
        }

        // Sort by priority if present
        rules.sort_by(|a, b| {
            let pa = a.priority.unwrap_or(i32::MAX);
            let pb = b.priority.unwrap_or(i32::MAX);
            pa.cmp(&pb)
        });

        Ok(Self { rules })
    }

    /// Reload rules from a file.
    ///
    /// Not available on wasm32 targets.
    ///
    /// # Errors
    ///
    /// Propagates any error from [`from_file`](Self::from_file). The current
    /// rule set is left unchanged when reloading fails.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn reload<P: AsRef<Path>>(&mut self, path: P) -> CelResult<()> {
        let new_engine = Self::from_file(path)?;
        self.rules = new_engine.rules;
        Ok(())
    }
}
