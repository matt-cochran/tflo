//! CEL rule engine with hot reload support.

use crate::config::RulesConfig;
use crate::context::IntoCelContext;
use crate::error::{CelError, CelResult};
use cel_interpreter::Program;
#[cfg(not(target_arch = "wasm32"))]
use std::fs;
#[cfg(not(target_arch = "wasm32"))]
use std::path::Path;

/// A compiled rule ready for evaluation.
#[derive(Debug)]
pub struct CompiledRule {
    /// The rule name.
    pub name: String,
    /// The compiled CEL program.
    program: Program,
    /// The action to take when the rule matches.
    pub action: Action,
    /// Optional description.
    pub description: Option<String>,
    /// Optional priority (lower is higher priority).
    pub priority: Option<i32>,
    /// Optional tags for categorization.
    pub tags: Vec<String>,
}

impl CompiledRule {
    /// Create a new compiled rule.
    pub fn new(name: &str, condition: &str, action: Action) -> CelResult<Self> {
        let program = Program::compile(condition).map_err(|e| CelError::CompileError {
            expression: condition.to_string(),
            message: e.to_string(),
        })?;

        Ok(Self {
            name: name.to_string(),
            program,
            action,
            description: None,
            priority: None,
            tags: Vec::new(),
        })
    }

    /// Set the description.
    #[must_use]
    pub fn with_description(mut self, desc: impl Into<String>) -> Self {
        self.description = Some(desc.into());
        self
    }

    /// Set the priority.
    #[must_use]
    pub fn with_priority(mut self, priority: i32) -> Self {
        self.priority = Some(priority);
        self
    }

    /// Add tags.
    #[must_use]
    pub fn with_tags(mut self, tags: Vec<String>) -> Self {
        self.tags = tags;
        self
    }

    /// Evaluate the rule against a context.
    pub fn matches<T: IntoCelContext>(&self, item: &T) -> bool {
        let ctx = item.into_cel_context();
        matches!(
            self.program.execute(&ctx),
            Ok(cel_interpreter::Value::Bool(true))
        )
    }
}

/// Action to take when a rule matches.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    /// Log the match.
    Log,
    /// Send an alert.
    Alert {
        /// Alert priority.
        priority: AlertPriority,
        /// Optional notification targets.
        notify: Vec<String>,
    },
    /// Record the event.
    Record {
        /// Duration to record in seconds.
        duration_secs: Option<u32>,
    },
    /// Tag the item.
    Tag {
        /// The tag to apply.
        tag: String,
    },
    /// Ignore/suppress the item.
    Ignore,
    /// Custom action with a name.
    Custom {
        /// The action name.
        name: String,
        /// Additional parameters as JSON.
        params: serde_json::Value,
    },
}

/// Alert priority levels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AlertPriority {
    /// Low priority.
    Low,
    /// Medium priority.
    #[default]
    Medium,
    /// High priority.
    High,
    /// Critical priority.
    Critical,
}

/// Rule engine for evaluating multiple rules.
#[derive(Debug)]
pub struct RuleEngine {
    rules: Vec<CompiledRule>,
}

impl Default for RuleEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl RuleEngine {
    /// Create a new empty rule engine.
    #[must_use]
    pub fn new() -> Self {
        Self { rules: Vec::new() }
    }

    /// Load rules from a YAML string.
    pub fn from_yaml(yaml: &str) -> CelResult<Self> {
        let config: RulesConfig =
            serde_yaml::from_str(yaml).map_err(|e| CelError::ConfigError {
                message: e.to_string(),
            })?;
        Self::from_config(config)
    }

    /// Load rules from a JSON string.
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

    /// Add a rule to the engine.
    pub fn add_rule(&mut self, rule: CompiledRule) {
        self.rules.push(rule);
    }

    /// Evaluate all rules against an item.
    ///
    /// Returns all matching rules.
    pub fn evaluate<'a, T: IntoCelContext>(&'a self, item: &T) -> Vec<&'a CompiledRule> {
        self.rules
            .iter()
            .filter(|rule| rule.matches(item))
            .collect()
    }

    /// Evaluate and return only the first matching rule.
    pub fn evaluate_first<'a, T: IntoCelContext>(&'a self, item: &T) -> Option<&'a CompiledRule> {
        self.rules.iter().find(|rule| rule.matches(item))
    }

    /// Get the number of rules.
    #[must_use]
    pub fn rule_count(&self) -> usize {
        self.rules.len()
    }

    /// Get all rules.
    #[must_use]
    pub fn rules(&self) -> &[CompiledRule] {
        &self.rules
    }

    /// Reload rules from a file.
    ///
    /// Not available on wasm32 targets.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn reload<P: AsRef<Path>>(&mut self, path: P) -> CelResult<()> {
        let new_engine = Self::from_file(path)?;
        self.rules = new_engine.rules;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::ContextBuilder;
    use cel_interpreter::Context;

    struct TestData {
        value: i64,
        level: String,
    }

    impl IntoCelContext for TestData {
        fn into_cel_context(&self) -> Context<'static> {
            ContextBuilder::new()
                .with_int("value", self.value)
                .with_string("level", &self.level)
                .build()
        }
    }

    #[test]
    fn test_compiled_rule() {
        let rule =
            CompiledRule::new("test", "value > 10", Action::Log).expect("rule should compile");

        let data_match = TestData {
            value: 15,
            level: "high".into(),
        };
        let data_no_match = TestData {
            value: 5,
            level: "low".into(),
        };

        assert!(rule.matches(&data_match));
        assert!(!rule.matches(&data_no_match));
    }

    #[test]
    fn test_rule_engine_yaml() {
        let yaml = r#"
rules:
  - name: high_value
    condition: "value > 100"
    action:
      type: alert
  - name: low_value
    condition: "value < 10"
    action:
      type: log
"#;

        let engine = RuleEngine::from_yaml(yaml).expect("should parse");
        assert_eq!(engine.rule_count(), 2);

        let high = TestData {
            value: 150,
            level: "high".into(),
        };
        let matched = engine.evaluate(&high);
        assert_eq!(matched.len(), 1);
        assert_eq!(matched[0].name, "high_value");
    }

    #[test]
    fn test_evaluate_first() {
        let yaml = r#"
rules:
  - name: rule1
    condition: "value > 50"
    priority: 2
    action:
      type: log
  - name: rule2
    condition: "value > 50"
    priority: 1
    action:
      type: alert
"#;

        let engine = RuleEngine::from_yaml(yaml).expect("should parse");

        let data = TestData {
            value: 100,
            level: "x".into(),
        };
        let first = engine.evaluate_first(&data);
        assert!(first.is_some());
        assert_eq!(first.expect("should have match").name, "rule2"); // Priority 1 first
    }
}
