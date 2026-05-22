//! Configuration types for rule loading.

use crate::rule_engine::{Action, AlertPriority};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Configuration for a collection of rules.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RulesConfig {
    /// Optional variables that can be referenced in conditions.
    #[serde(default)]
    pub variables: HashMap<String, serde_json::Value>,

    /// The rules to evaluate.
    pub rules: Vec<RuleConfig>,
}

/// Configuration for a single rule.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuleConfig {
    /// The rule name (must be unique).
    pub name: String,

    /// Optional description.
    #[serde(default)]
    pub description: Option<String>,

    /// The CEL condition expression.
    pub condition: String,

    /// The action to take when the condition matches.
    pub action: ActionConfig,

    /// Optional priority (lower is higher priority).
    #[serde(default)]
    pub priority: Option<i32>,

    /// Optional tags for categorization.
    #[serde(default)]
    pub tags: Option<Vec<String>>,

    /// Whether the rule is enabled.
    #[serde(default = "default_true")]
    pub enabled: bool,
}

fn default_true() -> bool {
    true
}

/// Configuration for an action.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum ActionConfig {
    /// Log the match.
    Log,

    /// Send an alert.
    Alert {
        /// Alert priority.
        #[serde(default)]
        priority: AlertPriorityConfig,
        /// Notification targets.
        #[serde(default)]
        notify: Vec<String>,
    },

    /// Record the event.
    Record {
        /// Duration to record in seconds.
        #[serde(default)]
        duration_secs: Option<u32>,
    },

    /// Tag the item.
    Tag {
        /// The tag to apply.
        tag: String,
    },

    /// Ignore/suppress.
    Ignore,

    /// Custom action.
    Custom {
        /// Action name.
        name: String,
        /// Additional parameters.
        #[serde(default)]
        params: serde_json::Value,
    },
}

impl ActionConfig {
    /// Convert to the runtime Action type.
    #[must_use]
    pub fn into_action(self) -> Action {
        match self {
            Self::Log => Action::Log,
            Self::Alert { priority, notify } => Action::Alert {
                priority: priority.into_priority(),
                notify,
            },
            Self::Record { duration_secs } => Action::Record { duration_secs },
            Self::Tag { tag } => Action::Tag { tag },
            Self::Ignore => Action::Ignore,
            Self::Custom { name, params } => Action::Custom { name, params },
        }
    }
}

/// Priority configuration.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AlertPriorityConfig {
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

impl AlertPriorityConfig {
    /// Convert to runtime priority.
    #[must_use]
    pub fn into_priority(self) -> AlertPriority {
        match self {
            Self::Low => AlertPriority::Low,
            Self::Medium => AlertPriority::Medium,
            Self::High => AlertPriority::High,
            Self::Critical => AlertPriority::Critical,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_yaml() {
        let yaml = r#"
variables:
  threshold: 10.0
  
rules:
  - name: high_value
    description: "Detect high values"
    condition: "value > 10"
    action:
      type: alert
      priority: high
      notify:
        - ops@example.com
    priority: 1
    tags:
      - important
      
  - name: low_value
    condition: "value < 5"
    action:
      type: log
"#;

        let config: RulesConfig = serde_yaml::from_str(yaml).expect("should parse");

        assert_eq!(config.rules.len(), 2);
        assert_eq!(config.rules[0].name, "high_value");
        assert_eq!(config.rules[0].priority, Some(1));
        assert!(matches!(
            config.rules[0].action,
            ActionConfig::Alert {
                priority: AlertPriorityConfig::High,
                ..
            }
        ));
    }

    #[test]
    fn test_parse_json() {
        let json = r#"{
            "rules": [
                {
                    "name": "test",
                    "condition": "value > 0",
                    "action": { "type": "log" }
                }
            ]
        }"#;

        let config: RulesConfig = serde_json::from_str(json).expect("should parse");
        assert_eq!(config.rules.len(), 1);
    }
}
