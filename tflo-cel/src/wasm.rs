//! WebAssembly bridge for tflo-cel.
//!
//! This module provides JSON-in/JSON-out CEL rule evaluation entry points
//! for wasm builds. It follows the same pattern as `tflo_core::wasm`.
//!
//! # Design
//!
//! - **Opaque bridge**: RuleEngine and compiled rule types are never exposed
//!   across FFI. All interaction is via JSON strings.
//! - **Serde-driven**: Inputs and outputs use `serde_json` for maximum
//!   compatibility with TypeScript consumers.
//! - **No `#[wasm_bindgen]`**: Final `#[wasm_bindgen]` exports live in
//!   the thin wrapper crate (see `tflo-wasm/src/lib.rs`).

#![cfg(target_arch = "wasm32")]

use crate::context::IntoCelContext;
use crate::rule_engine::RuleEngine;
use cel_interpreter::Context;
use serde::Deserialize;
use serde::Serialize;

/// A single item to evaluate rules against.
#[derive(Debug, Clone, Deserialize)]
pub struct RuleItem {
    /// The item's unique identifier.
    pub id: String,
    /// Arbitrary fields for CEL evaluation.
    #[serde(flatten)]
    pub fields: serde_json::Map<String, serde_json::Value>,
}

/// Result of rule evaluation.
#[derive(Debug, Clone, Serialize)]
pub struct EvaluationResult {
    /// The item's unique identifier.
    pub item_id: String,
    /// Names of matched rules.
    pub matched_rules: Vec<String>,
}

impl IntoCelContext for RuleItem {
    fn into_cel_context(&self) -> Context<'static> {
        let mut ctx = Context::default();
        for (key, value) in &self.fields {
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
                _ => {
                    // Skip null, arrays, objects — cel-interpreter 0.8 doesn't
                    // support them directly via add_variable
                }
            }
        }
        ctx
    }
}

/// Evaluate CEL rules against a list of items.
///
/// # Arguments
/// * `rules_json` — JSON string of rules (as accepted by `RuleEngine::from_json`).
/// * `items_json` — JSON array of `RuleItem` objects.
///
/// # Returns
/// JSON array of `EvaluationResult` objects, one per item.
pub fn evaluate_rules(rules_json: &str, items_json: &str) -> String {
    let engine: RuleEngine = match RuleEngine::from_json(rules_json) {
        Ok(e) => e,
        Err(e) => return format!("{{\"error\": \"{e}\"}}"),
    };

    let items: Vec<RuleItem> = match serde_json::from_str(items_json) {
        Ok(i) => i,
        Err(e) => return format!("{{\"error\": \"invalid items: {e}\"}}"),
    };

    let results: Vec<EvaluationResult> = items
        .into_iter()
        .map(|item| {
            let matched_rules: Vec<String> = engine
                .evaluate(&item)
                .into_iter()
                .map(|r| r.name.clone())
                .collect();
            EvaluationResult {
                item_id: item.id,
                matched_rules,
            }
        })
        .collect();

    serde_json::to_string(&results).unwrap_or_else(|_| "[]".to_string())
}

/// Evaluate CEL rules from YAML format against a list of items.
///
/// # Arguments
/// * `rules_yaml` — YAML string of rules.
/// * `items_json` — JSON array of `RuleItem` objects.
///
/// # Returns
/// JSON array of `EvaluationResult` objects.
pub fn evaluate_rules_from_yaml(rules_yaml: &str, items_json: &str) -> String {
    let engine: RuleEngine = match RuleEngine::from_yaml(rules_yaml) {
        Ok(e) => e,
        Err(e) => return format!("{{\"error\": \"{e}\"}}"),
    };

    let items: Vec<RuleItem> = match serde_json::from_str(items_json) {
        Ok(i) => i,
        Err(e) => return format!("{{\"error\": \"invalid items: {e}\"}}"),
    };

    let results: Vec<EvaluationResult> = items
        .into_iter()
        .map(|item| {
            let matched_rules: Vec<String> = engine
                .evaluate(&item)
                .into_iter()
                .map(|r| r.name.clone())
                .collect();
            EvaluationResult {
                item_id: item.id,
                matched_rules,
            }
        })
        .collect();

    serde_json::to_string(&results).unwrap_or_else(|_| "[]".to_string())
}
