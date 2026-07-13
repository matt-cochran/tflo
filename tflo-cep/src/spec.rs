//! Serializable CEL pattern specs — **patterns as data** (feature `cel`).
//!
//! A [`CelPatternSpec`] is the wire form of a temporal rule: pure data (CEL
//! strings + a window), no code. A server can author one, serialize it to JSON,
//! and push it to browsers; both tiers `compile` it into the *same* matching
//! behavior. The JSON shape is byte-identical to the TypeScript `CelPatternSpec`
//! in `@tflo/react/cel` (`name`, `when`, `then`, `notThen`, `within`), so one
//! document drives both engines.
//!
//! The `timestamp` and `emit` closures stay code (provided per tier at compile
//! time) — only the *matching logic* is data.

use crate::matched::Match;
use crate::pattern::{Pattern, PatternError};
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// A serializable CEL pattern: `when` opens the match, each `then` is a positive
/// sequential CEL step, an optional `not_then` is a negative terminal step, and
/// `within` (ms) bounds the last step. Mirrors the TypeScript `CelPatternSpec`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CelPatternSpec {
    /// Signal name (diagnostic / emitted name).
    pub name: String,
    /// Initial CEL predicate, e.g. `kind == "add_to_cart"`.
    pub when: String,
    /// Positive sequential CEL steps. May reference earlier captures via
    /// `first_<field>` / `prev_<field>` / `s{i}_<field>`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub then: Vec<String>,
    /// Optional **interior-negation guards**, positionally aligned with `then`:
    /// a non-empty `notBetween[i]` forbids its CEL between the running match and
    /// `then[i]` ("A then B with no C in between"). An empty string (or a
    /// missing index) means no guard for that step. Mirrors the TypeScript
    /// `CelPatternSpec.notBetween`.
    #[serde(default, skip_serializing_if = "Vec::is_empty", rename = "notBetween")]
    pub not_between: Vec<String>,
    /// Optional negative terminal CEL step (requires `within`).
    #[serde(default, skip_serializing_if = "Option::is_none", rename = "notThen")]
    pub not_then: Option<String>,
    /// Optional time bound (ms) applied to the last step.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub within: Option<i64>,
    /// Optional **rule version**. When a rule fires, the emitted derived event
    /// (signal / verdict / correction) carries this version, so the log records
    /// *which* version of the rule produced each result. Because emitted events
    /// are immutable, changing the rule preserves history for free and re-judging
    /// under a new version is just emitting a new derived event — no event-time
    /// effective-dating machinery needed. Defaults to `None` (unversioned).
    /// Mirrors the TypeScript `CelPatternSpec.version`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<u32>,
}

impl CelPatternSpec {
    /// Compile this spec into a runnable [`Pattern`] for event type `E`.
    ///
    /// `timestamp` extracts event-time; `emit` shapes the output `M`. Both are
    /// supplied per tier — only the matching logic comes from the (serialized)
    /// spec, so a spec compiled here matches identically to the same spec
    /// compiled in the browser.
    ///
    /// # Errors
    /// Returns [`PatternError`] if a CEL string is invalid or the step structure
    /// is illegal (e.g. a `not_then` without `within`).
    pub fn compile<E, M, Ts, Em>(
        &self,
        timestamp: Ts,
        emit: Em,
    ) -> Result<Pattern<E, M>, PatternError>
    where
        E: Clone + Serialize + 'static,
        M: 'static,
        Ts: Fn(&E) -> i64 + Send + Sync + 'static,
        Em: Fn(&Match<E>) -> M + Send + Sync + 'static,
    {
        let mut p = Pattern::<E, ()>::new(self.name.clone())
            .timestamp(timestamp)
            .when_cel(&self.when);
        for (i, t) in self.then.iter().enumerate() {
            match self.not_between.get(i) {
                Some(guard) if !guard.is_empty() => p = p.not_between_cel(guard).then_cel(t),
                _ => p = p.then_cel(t),
            }
        }
        if let Some(nt) = &self.not_then {
            p = p.not_then_cel(nt);
        }
        if let Some(ms) = self.within {
            p = p.within(Duration::from_millis(ms.max(0).unsigned_abs()));
        }
        p.emit(emit)
    }
}
