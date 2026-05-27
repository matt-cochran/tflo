//! CEL rule engine — in-memory rule storage + evaluation.
//!
//! Config-file loaders (`from_yaml` / `from_json` / `from_file` / `from_str`
//! / `from_config` / `reload`) live in [`crate::rule_loader`].

use crate::error::{CelError, CelResult};
use crate::traits::IntoCelContext;
use cel_interpreter::Program;

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
    ///
    /// # Errors
    ///
    /// Returns [`CelError::CompileError`](crate::error::CelError::CompileError)
    /// when `condition` is not a valid CEL expression.
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
    pub const fn with_priority(mut self, priority: i32) -> Self {
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
    pub(crate) rules: Vec<CompiledRule>,
}

impl Default for RuleEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl RuleEngine {
    /// Create a new empty rule engine.
    #[must_use]
    pub const fn new() -> Self {
        Self { rules: Vec::new() }
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
    pub const fn rule_count(&self) -> usize {
        self.rules.len()
    }

    /// Get all rules.
    #[must_use]
    pub fn rules(&self) -> &[CompiledRule] {
        &self.rules
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
