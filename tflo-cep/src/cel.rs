//! Native CEL-string predicates for the [`Pattern`](crate::Pattern) builder.
//!
//! A CEL predicate is a **string** (serializable data), not a Rust closure, so
//! it can be authored once, shipped from a server, stored, and evaluated
//! *identically* on the client (`tflo-cep-wasm --features cel`) and here in the
//! native engine. This module is the native half of that cross-tier parity: it
//! compiles the same `cel-interpreter` programs and builds the **same** variable
//! context (top-level current-event fields plus `first_*` / `prev_* `/ `s{i}_*`
//! captures) as `tflo-cep-wasm`, so a given pattern + event stream yields
//! byte-identical signals on both tiers.
//!
//! Only compiled with `--features cel`.

use cel_interpreter::{Context, Program, Value};
use serde::Serialize;

/// Upper bound on a CEL predicate's source length. A predicate is a small
/// expression, not a program; the cap bounds the recursive-descent parser's
/// worst case. Mirrors `tflo-cep-wasm`.
const MAX_CEL_LEN: usize = 1024;

/// Upper bound on syntactic nesting (brackets + runs of prefix operators), which
/// drive parser recursion depth. Mirrors `tflo-cep-wasm`. Rejected *before* the
/// parser so a pathological expression can't overflow the stack.
const MAX_CEL_DEPTH: usize = 32;

/// Validate a CEL string is safe to parse: bounded length and bounded nesting.
/// Returns the reason string on rejection. Identical rule to `tflo-cep-wasm`.
fn validate(expr: &str) -> Result<(), String> {
    if expr.len() > MAX_CEL_LEN {
        return Err(format!(
            "CEL expression too long ({} bytes; max {MAX_CEL_LEN})",
            expr.len()
        ));
    }
    let (mut depth, mut max_depth, mut run, mut max_run) = (0usize, 0usize, 0usize, 0usize);
    for b in expr.bytes() {
        match b {
            b'(' | b'[' | b'{' => {
                depth = depth.saturating_add(1);
                max_depth = max_depth.max(depth);
            }
            b')' | b']' | b'}' => depth = depth.saturating_sub(1),
            b'!' | b'-' => {
                run = run.saturating_add(1);
                max_run = max_run.max(run);
            }
            b' ' | b'\t' | b'\n' | b'\r' => {}
            _ => run = 0,
        }
    }
    let nesting = max_depth.max(max_run);
    if nesting > MAX_CEL_DEPTH {
        return Err(format!(
            "CEL expression nests too deeply ({nesting} levels; max {MAX_CEL_DEPTH})"
        ));
    }
    Ok(())
}

/// Validate then compile a CEL string. Returns the reason string on failure.
///
/// # Errors
/// Returns `Err` if the expression fails the length/nesting guard or the
/// `cel-interpreter` parser rejects it.
pub fn compile(expr: &str) -> Result<Program, String> {
    validate(expr)?;
    Program::compile(expr).map_err(|e| format!("CEL compile error in `{expr}`: {e}"))
}

/// Add an event's scalar fields to `ctx` as `{prefix}{field}` variables
/// (empty prefix → top-level, e.g. `kind`). Nested objects/arrays/null are
/// skipped — the same scalar-only convention as `tflo-cel` / `tflo-cep-wasm`.
fn add_event_vars(ctx: &mut Context, value: &serde_json::Value, prefix: &str) {
    let serde_json::Value::Object(map) = value else {
        return;
    };
    for (k, v) in map {
        let name = if prefix.is_empty() {
            k.clone()
        } else {
            format!("{prefix}{k}")
        };
        match v {
            serde_json::Value::String(s) => drop(ctx.add_variable(&name, s.clone())),
            serde_json::Value::Number(n) => {
                if let Some(i) = n.as_i64() {
                    drop(ctx.add_variable(&name, i));
                } else if let Some(f) = n.as_f64() {
                    drop(ctx.add_variable(&name, f));
                }
            }
            serde_json::Value::Bool(b) => drop(ctx.add_variable(&name, *b)),
            serde_json::Value::Null
            | serde_json::Value::Array(_)
            | serde_json::Value::Object(_) => {}
        }
    }
}

/// Context-aware CEL evaluation with full `MatchContext` parity. The current
/// event's fields are top-level; events captured by EARLIER steps are exposed as
/// `first_<field>` (first capture), `prev_<field>` (immediately-preceding
/// capture), and `s0_<field>`, `s1_<field>`, … (by position) — so a predicate
/// can correlate across steps, e.g. `productId == first_productId`. Returns
/// `false` on any serialization or evaluation error (never panics).
pub fn eval_in_context<E: Serialize>(prog: &Program, event: &E, captures: &[(String, E)]) -> bool {
    let Ok(event_json) = serde_json::to_value(event) else {
        return false;
    };
    let mut ctx = Context::default();
    add_event_vars(&mut ctx, &event_json, "");
    // Serialize captures once; reuse for s{i}_ / first_ / prev_.
    let cap_json: Vec<serde_json::Value> = captures
        .iter()
        .map(|(_, e)| serde_json::to_value(e).unwrap_or(serde_json::Value::Null))
        .collect();
    for (i, cap) in cap_json.iter().enumerate() {
        add_event_vars(&mut ctx, cap, &format!("s{i}_"));
    }
    if let Some(first) = cap_json.first() {
        add_event_vars(&mut ctx, first, "first_");
    }
    if let Some(prev) = cap_json.last() {
        add_event_vars(&mut ctx, prev, "prev_");
    }
    matches!(prog.execute(&ctx), Ok(Value::Bool(true)))
}
