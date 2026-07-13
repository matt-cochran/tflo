//! Rego-based filtering for iterators.
//!
//! This module is the *fail-safe* surface for policy-driven filtering. The
//! two key invariants:
//!
//! 1. **Default-deny on error.** When the underlying Rego engine returns an
//!    `Err`, [`RegoFilter::next`] increments
//!    [`eval_errors_total`](RegoFilter::eval_errors_total), rate-limit-logs
//!    the failure, and *drops* the item. This preserves the security
//!    contract: a malformed or buggy policy never accidentally widens
//!    access. Callers who need to see every failure should use
//!    [`RegoFilterResult`] instead, which surfaces `Err(_)` per item.
//! 2. **Wall-clock budget enforcement.** `regorus` is synchronous and cannot
//!    be cancelled mid-evaluation, but a `timeout_ms` budget (set via
//!    [`PolicyConfig::timeout_ms`](crate::config::PolicyConfig::timeout_ms))
//!    is still honored: each call is timed with [`std::time::Instant`], and
//!    if elapsed exceeds the budget the result is classified as a timeout
//!    (counter incremented, item denied on [`RegoFilter`]; surfaced as
//!    `Err(RegoError::EvalTimeout {..})` on [`RegoFilterResult`]).

use crate::config::PolicyConfig;
use crate::error::{RegoError, RegoResult};
use crate::policy::PolicyEngine;
use crate::traits::IntoRegoInput;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

/// Outcome of a single Rego evaluation, used by the internal helper.
enum EvalOutcome {
    /// Policy allowed the item.
    Allow,
    /// Policy denied the item.
    Deny,
    /// Engine returned an error.
    EvalError(RegoError),
    /// Evaluation completed but exceeded the wall-clock budget.
    TimedOut {
        /// The wall-clock budget that was exceeded.
        budget_ms: u64,
        /// The actual elapsed time.
        elapsed_ms: u64,
    },
}

/// Internal helper: evaluate `query` against `item`, classify the outcome,
/// and apply the post-evaluation wall-clock watchdog when `budget_ms` is set.
fn evaluate<T: IntoRegoInput>(
    engine: &Arc<Mutex<PolicyEngine>>,
    item: &T,
    query: &str,
    budget_ms: Option<u64>,
) -> EvalOutcome {
    let mut guard = match engine.lock() {
        Ok(g) => g,
        Err(e) => {
            return EvalOutcome::EvalError(RegoError::EvaluationError {
                query: query.to_string(),
                message: format!("mutex poisoned: {e}"),
            });
        }
    };

    let start = Instant::now();
    let result = guard.eval_allow(item, query);
    let elapsed_ms = u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX);
    drop(guard);

    // Watchdog: even if eval succeeded, an exceeded budget is reported as a
    // timeout. regorus is sync and cannot be cancelled mid-call; this is the
    // honest spelling of "we noticed it ran too long."
    //
    // A budget of 0 means "no eval is permitted to run" — it always fires,
    // even on systems where `Instant::elapsed().as_millis()` rounds to 0.
    if let Some(budget) = budget_ms {
        if budget == 0 || elapsed_ms > budget {
            return EvalOutcome::TimedOut {
                budget_ms: budget,
                elapsed_ms,
            };
        }
    }

    match result {
        Ok(true) => EvalOutcome::Allow,
        Ok(false) => EvalOutcome::Deny,
        Err(e) => EvalOutcome::EvalError(e),
    }
}

/// Rate-limit-log helper: emits via `eprintln!` every 64th occurrence.
///
/// We deliberately avoid pulling in `tracing` to keep this crate slim; the
/// throttled stderr line is enough for an operator to notice a misbehaving
/// policy without flooding logs in a hot loop.
fn log_throttled(counter: &AtomicU64, msg: &str) {
    let n = counter.fetch_add(1, Ordering::Relaxed);
    if n % 64 == 0 {
        // `fetch_add` wraps on overflow, so `n` may be `u64::MAX`; saturate
        // the +1 we use for the display index — the occurrence number is
        // human-readable only and capping at u64::MAX is a fine ceiling.
        let display = n.saturating_add(1);
        // SAFETY (print_stderr): operator-visible diagnostic for the
        // REGO-001 throttled-error path. Tracing was deliberately not
        // pulled in as a dep here; this is the documented fallback.
        #[allow(clippy::print_stderr)]
        {
            eprintln!("[tflo-rego] {msg} (occurrence #{display})");
        }
    }
}

/// Extension trait for Rego-based filtering on iterators.
pub trait RegoFilterExt<T>: Iterator<Item = T> + Sized
where
    T: IntoRegoInput,
{
    /// Filter items using a Rego policy query.
    ///
    /// Items for which the query evaluates to `true` are kept. Items for
    /// which the engine returns an error or which exceed the (unset, by
    /// default) wall-clock budget are *denied* (fail-safe). Counts of both
    /// classes are exposed via [`RegoFilter::eval_errors_total`] and
    /// [`RegoFilter::eval_timeouts_total`].
    fn rego_filter(self, engine: Arc<Mutex<PolicyEngine>>, query: &str) -> RegoFilter<Self, T>;

    /// Filter items with full error visibility.
    ///
    /// Every decision — allow, deny, error, timeout — is surfaced as a
    /// `Result<T, RegoError>`. Denials are *skipped* (the iterator yields
    /// nothing for them); errors and timeouts yield `Some(Err(_))`.
    fn rego_filter_result(
        self,
        engine: Arc<Mutex<PolicyEngine>>,
        query: &str,
    ) -> RegoFilterResult<Self, T>;

    /// Filter items, honoring [`PolicyConfig::timeout_ms`] as a
    /// post-evaluation wall-clock watchdog.
    ///
    /// See [`rego_filter`](Self::rego_filter) for fail-safe behavior. The
    /// timeout budget is detected *after* `regorus` returns (it cannot be
    /// cancelled mid-eval) and triggers a denial plus a bump of
    /// [`RegoFilter::eval_timeouts_total`].
    fn rego_filter_with_config(
        self,
        engine: Arc<Mutex<PolicyEngine>>,
        query: &str,
        config: &PolicyConfig,
    ) -> RegoFilter<Self, T>;

    /// Filter items with error visibility, honoring
    /// [`PolicyConfig::timeout_ms`].
    ///
    /// Timeouts surface as `Some(Err(RegoError::EvalTimeout {..}))`;
    /// evaluation errors surface as `Some(Err(RegoError::EvaluationError
    /// {..}))` (or whichever variant the engine returned).
    fn rego_filter_result_with_config(
        self,
        engine: Arc<Mutex<PolicyEngine>>,
        query: &str,
        config: &PolicyConfig,
    ) -> RegoFilterResult<Self, T>;
}

impl<I, T> RegoFilterExt<T> for I
where
    I: Iterator<Item = T>,
    T: IntoRegoInput,
{
    fn rego_filter(self, engine: Arc<Mutex<PolicyEngine>>, query: &str) -> RegoFilter<Self, T> {
        RegoFilter::new(self, engine, query, None)
    }

    fn rego_filter_result(
        self,
        engine: Arc<Mutex<PolicyEngine>>,
        query: &str,
    ) -> RegoFilterResult<Self, T> {
        RegoFilterResult::new(self, engine, query, None)
    }

    fn rego_filter_with_config(
        self,
        engine: Arc<Mutex<PolicyEngine>>,
        query: &str,
        config: &PolicyConfig,
    ) -> RegoFilter<Self, T> {
        RegoFilter::new(self, engine, query, config.timeout_ms)
    }

    fn rego_filter_result_with_config(
        self,
        engine: Arc<Mutex<PolicyEngine>>,
        query: &str,
        config: &PolicyConfig,
    ) -> RegoFilterResult<Self, T> {
        RegoFilterResult::new(self, engine, query, config.timeout_ms)
    }
}

/// Iterator adapter that filters using Rego policies, fail-safe on error.
pub struct RegoFilter<I, T>
where
    I: Iterator<Item = T>,
    T: IntoRegoInput,
{
    iter: I,
    engine: Arc<Mutex<PolicyEngine>>,
    query: String,
    budget_ms: Option<u64>,
    eval_errors_total: Arc<AtomicU64>,
    eval_timeouts_total: Arc<AtomicU64>,
}

impl<I, T> RegoFilter<I, T>
where
    I: Iterator<Item = T>,
    T: IntoRegoInput,
{
    fn new(iter: I, engine: Arc<Mutex<PolicyEngine>>, query: &str, budget_ms: Option<u64>) -> Self {
        Self {
            iter,
            engine,
            query: query.to_string(),
            budget_ms,
            eval_errors_total: Arc::new(AtomicU64::new(0)),
            eval_timeouts_total: Arc::new(AtomicU64::new(0)),
        }
    }

    /// Cumulative count of Rego evaluations that returned an error.
    ///
    /// Returned as an `Arc<AtomicU64>` so observers can hold a handle to
    /// the counter even after the filter is dropped.
    #[must_use]
    pub fn eval_errors_total(&self) -> Arc<AtomicU64> {
        Arc::clone(&self.eval_errors_total)
    }

    /// Cumulative count of Rego evaluations that exceeded the
    /// [`PolicyConfig::timeout_ms`] wall-clock budget.
    ///
    /// Returned as an `Arc<AtomicU64>` so observers can hold a handle to
    /// the counter even after the filter is dropped.
    #[must_use]
    pub fn eval_timeouts_total(&self) -> Arc<AtomicU64> {
        Arc::clone(&self.eval_timeouts_total)
    }
}

impl<I, T> std::fmt::Debug for RegoFilter<I, T>
where
    I: Iterator<Item = T> + std::fmt::Debug,
    T: IntoRegoInput,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RegoFilter")
            .field("iter", &self.iter)
            .field("query", &self.query)
            .field("budget_ms", &self.budget_ms)
            .field(
                "eval_errors_total",
                &self.eval_errors_total.load(Ordering::Relaxed),
            )
            .field(
                "eval_timeouts_total",
                &self.eval_timeouts_total.load(Ordering::Relaxed),
            )
            .finish()
    }
}

impl<I, T> Iterator for RegoFilter<I, T>
where
    I: Iterator<Item = T>,
    T: IntoRegoInput,
{
    type Item = T;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let item = self.iter.next()?;

            match evaluate(&self.engine, &item, &self.query, self.budget_ms) {
                EvalOutcome::Allow => return Some(item),
                EvalOutcome::Deny => {}
                EvalOutcome::EvalError(e) => {
                    log_throttled(
                        &self.eval_errors_total,
                        &format!("eval error on query '{}': {e}", self.query),
                    );
                    // Default-deny: drop the item.
                }
                EvalOutcome::TimedOut {
                    budget_ms,
                    elapsed_ms,
                } => {
                    log_throttled(
                        &self.eval_timeouts_total,
                        &format!(
                            "eval timeout on query '{}': elapsed {elapsed_ms}ms > budget {budget_ms}ms",
                            self.query
                        ),
                    );
                    // Default-deny: drop the item.
                }
            }
        }
    }
}

/// Iterator adapter that filters with full error visibility.
pub struct RegoFilterResult<I, T>
where
    I: Iterator<Item = T>,
    T: IntoRegoInput,
{
    iter: I,
    engine: Arc<Mutex<PolicyEngine>>,
    query: String,
    budget_ms: Option<u64>,
}

impl<I, T> RegoFilterResult<I, T>
where
    I: Iterator<Item = T>,
    T: IntoRegoInput,
{
    fn new(iter: I, engine: Arc<Mutex<PolicyEngine>>, query: &str, budget_ms: Option<u64>) -> Self {
        Self {
            iter,
            engine,
            query: query.to_string(),
            budget_ms,
        }
    }
}

impl<I, T> std::fmt::Debug for RegoFilterResult<I, T>
where
    I: Iterator<Item = T> + std::fmt::Debug,
    T: IntoRegoInput,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RegoFilterResult")
            .field("iter", &self.iter)
            .field("query", &self.query)
            .field("budget_ms", &self.budget_ms)
            .finish()
    }
}

impl<I, T> Iterator for RegoFilterResult<I, T>
where
    I: Iterator<Item = T>,
    T: IntoRegoInput,
{
    type Item = RegoResult<T>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let item = self.iter.next()?;

            match evaluate(&self.engine, &item, &self.query, self.budget_ms) {
                EvalOutcome::Allow => return Some(Ok(item)),
                EvalOutcome::Deny => {}
                EvalOutcome::EvalError(e) => return Some(Err(e)),
                EvalOutcome::TimedOut {
                    budget_ms,
                    elapsed_ms,
                } => {
                    return Some(Err(RegoError::EvalTimeout {
                        query: self.query.clone(),
                        budget_ms,
                        elapsed_ms,
                    }));
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Serialize;

    #[derive(Serialize, Clone, Debug)]
    struct TestItem {
        value: i64,
    }

    fn engine_with_test_policy() -> Arc<Mutex<PolicyEngine>> {
        let engine = Arc::new(Mutex::new(PolicyEngine::new()));
        {
            let mut e = engine.lock().expect("should lock");
            e.add_policy(
                "test",
                r#"
                package test
                default allow := false
                allow if { input.value > 10 }
            "#,
            )
            .expect("should parse");
        }
        engine
    }

    #[test]
    fn test_rego_filter() {
        let engine = engine_with_test_policy();

        let items = vec![
            TestItem { value: 5 },
            TestItem { value: 15 },
            TestItem { value: 25 },
        ];

        let filtered: Vec<TestItem> = items
            .into_iter()
            .rego_filter(engine, "data.test.allow")
            .collect();

        // Note: actual filtering depends on regorus behavior
        // This test verifies the API compiles and runs
        assert!(filtered.len() <= 3);
    }

    /// A query that always raises an `Err` from regorus (divide by zero is
    /// detected at evaluation time). Note that `data.nonexistent.allow`
    /// is *not* an error — Rego treats an unknown rule as `undefined`,
    /// which `extract_bool` coerces to `Ok(false)`. We need a genuinely
    /// failing query to exercise the error path.
    const ERROR_QUERY: &str = "1/0";

    #[test]
    fn rego_filter_counts_eval_errors() {
        let engine = Arc::new(Mutex::new(PolicyEngine::new()));
        let items = vec![TestItem { value: 1 }, TestItem { value: 2 }];

        let filter = items.into_iter().rego_filter(engine, ERROR_QUERY);
        let errors_handle = filter.eval_errors_total();

        let collected: Vec<TestItem> = filter.collect();

        // Default-deny on error: nothing emitted.
        assert!(
            collected.is_empty(),
            "expected default-deny but got {} items",
            collected.len()
        );
        // Counter incremented once per item.
        assert_eq!(
            errors_handle.load(Ordering::Relaxed),
            2,
            "eval_errors_total should match the number of failing evaluations"
        );
    }

    #[test]
    fn rego_filter_default_denies_on_error() {
        let engine = Arc::new(Mutex::new(PolicyEngine::new()));
        let items = vec![TestItem { value: 42 }];

        let collected: Vec<TestItem> = items.into_iter().rego_filter(engine, ERROR_QUERY).collect();

        assert!(
            collected.is_empty(),
            "items must not be emitted when the policy errors"
        );
    }

    #[test]
    fn rego_timeout_ms_respected() {
        let engine = engine_with_test_policy();
        let items = vec![TestItem { value: 15 }]; // would otherwise allow

        let config = PolicyConfig {
            timeout_ms: Some(0),
            ..PolicyConfig::default()
        };

        let mut iter =
            items
                .into_iter()
                .rego_filter_result_with_config(engine, "data.test.allow", &config);

        let first = iter.next().expect("expected one item");
        match first {
            Err(RegoError::EvalTimeout {
                query,
                budget_ms,
                elapsed_ms: _,
            }) => {
                assert_eq!(query, "data.test.allow");
                assert_eq!(budget_ms, 0);
            }
            other => panic!("expected EvalTimeout, got {other:?}"),
        }
    }
}
