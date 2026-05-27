//! CEL-based filtering for iterators.
//!
//! Two iterator adapters are provided:
//!
//! * [`CelFilterResult`] — the **canonical** API. It propagates CEL compile
//!   and evaluation errors via `Iterator<Item = Result<T, CelError>>`, never
//!   panics, exposes observable counters for evaluation errors and
//!   `max_eval_time` overruns, and accepts a [`CelOptions`] watchdog budget.
//! * [`CelFilter`] — a legacy, panic-on-compile-error convenience adapter
//!   kept for backwards compatibility. **Deprecated**; new call sites should
//!   prefer [`CelFilterExt::cel_filter_result`].
//!
//! See [`CelOptions`] for execution-time budgeting and watchdog semantics.

use crate::error::{CelError, CelResult};
use crate::traits::IntoCelContext;
use cel_interpreter::Program;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

/// Tunable options applied to CEL filter execution.
///
/// `cel-interpreter` 0.8 does not expose a step limit or a way to interrupt
/// a synchronous evaluation. The watchdog therefore measures *observed*
/// duration after each `Program::execute` call and increments a counter when
/// the budget is exceeded. It cannot actually interrupt the evaluation; it
/// makes overruns *observable* so operators can detect runaway rules.
///
/// Default: no time budget.
#[derive(Debug, Clone, Default)]
pub struct CelOptions {
    /// Soft per-call budget for a single CEL evaluation. When set, each
    /// item's evaluation is timed; an overrun increments
    /// [`CelFilterResult::eval_timeouts`] but the result is still returned.
    pub max_eval_time: Option<Duration>,
}

impl CelOptions {
    /// Construct empty options (no budget).
    #[must_use]
    pub const fn new() -> Self {
        Self { max_eval_time: None }
    }

    /// Set the per-call evaluation budget.
    #[must_use]
    pub const fn with_max_eval_time(mut self, d: Duration) -> Self {
        self.max_eval_time = Some(d);
        self
    }
}

/// Extension trait for CEL-based filtering on iterators.
///
/// # Examples
///
/// ```ignore
/// use tflo_cel::prelude::*;
///
/// let filtered: Vec<Detection> = detections.into_iter()
///     .cel_filter_result("snr > 10.0 && freq_mhz > 100.0")?
///     .filter_map(Result::ok)
///     .collect();
/// ```
pub trait CelFilterExt<T>: Iterator<Item = T> + Sized
where
    T: IntoCelContext,
{
    /// Filter items using a CEL expression (panic on compile failure).
    ///
    /// Items for which the expression evaluates to `true` are kept.
    /// Evaluation errors and non-boolean results are **silently discarded**.
    ///
    /// # Panics
    ///
    /// Panics if the CEL expression fails to compile.
    ///
    /// # Migration
    ///
    /// This variant is preserved for backwards compatibility but is
    /// deprecated for production use. Prefer
    /// [`cel_filter_result`](Self::cel_filter_result), which surfaces both
    /// compile and evaluation errors, exposes counters, and accepts a
    /// [`CelOptions`] watchdog.
    #[deprecated(
        note = "panics on compile failure and swallows evaluation errors; \
                use `cel_filter_result` (Result-returning) instead in production"
    )]
    #[allow(deprecated)]
    fn cel_filter(self, expr: &str) -> CelFilter<Self, T> {
        CelFilter::new(self, expr)
    }

    /// Filter items using a CEL expression, returning errors.
    ///
    /// Unlike [`cel_filter`](Self::cel_filter), this returns a
    /// `Result` for each item, allowing the caller to handle evaluation
    /// errors. Compile errors are surfaced at construction time.
    ///
    /// # Errors
    ///
    /// Returns [`CelError::CompileError`](crate::error::CelError::CompileError)
    /// when `expr` fails to compile as a CEL program.
    fn cel_filter_result(self, expr: &str) -> CelResult<CelFilterResult<Self, T>> {
        CelFilterResult::new(self, expr, CelOptions::default())
    }

    /// Filter items using a CEL expression with execution options.
    ///
    /// Identical to [`cel_filter_result`](Self::cel_filter_result), but
    /// accepts a [`CelOptions`] for watchdog budgeting.
    ///
    /// # Errors
    ///
    /// Returns [`CelError::CompileError`](crate::error::CelError::CompileError)
    /// when `expr` fails to compile as a CEL program.
    fn cel_filter_result_with_options(
        self,
        expr: &str,
        options: CelOptions,
    ) -> CelResult<CelFilterResult<Self, T>> {
        CelFilterResult::new(self, expr, options)
    }
}

impl<I, T> CelFilterExt<T> for I
where
    I: Iterator<Item = T>,
    T: IntoCelContext,
{
}

/// Iterator adapter that filters using CEL expressions.
///
/// **Deprecated**: silently discards evaluation errors. Use
/// [`CelFilterResult`] instead.
#[deprecated(
    note = "swallows evaluation errors silently; use `CelFilterResult` instead in production"
)]
pub struct CelFilter<I, T>
where
    I: Iterator<Item = T>,
    T: IntoCelContext,
{
    iter: I,
    program: Program,
}

#[allow(deprecated)]
impl<I, T> std::fmt::Debug for CelFilter<I, T>
where
    I: Iterator<Item = T> + std::fmt::Debug,
    T: IntoCelContext,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CelFilter")
            .field("iter", &self.iter)
            .finish()
    }
}

#[allow(deprecated)]
impl<I, T> CelFilter<I, T>
where
    I: Iterator<Item = T>,
    T: IntoCelContext,
{
    /// # Panics
    ///
    /// PANICS on compile error; use [`CelFilterResult`] (Result-returning)
    /// instead in production.
    #[allow(clippy::panic)]
    fn new(iter: I, expr: &str) -> Self {
        let program = Program::compile(expr)
            .unwrap_or_else(|e| panic!("failed to compile CEL expression '{expr}': {e}"));
        Self { iter, program }
    }
}

#[allow(deprecated)]
impl<I, T> Iterator for CelFilter<I, T>
where
    I: Iterator<Item = T>,
    T: IntoCelContext,
{
    type Item = T;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let item = self.iter.next()?;
            let ctx = item.into_cel_context();

            match self.program.execute(&ctx) {
                Ok(cel_interpreter::Value::Bool(true)) => return Some(item),
                Ok(cel_interpreter::Value::Bool(false)) => continue,
                Ok(_) => continue,  // Non-boolean result treated as false
                Err(_) => continue, // Evaluation error treated as false
            }
        }
    }
}

/// Iterator adapter that filters with error handling.
///
/// Yields `Result<T, CelError>` for each underlying item:
///
/// * `Ok(item)` — expression evaluated to `true`.
/// * `Err(CelError::EvaluationError { .. })` — runtime evaluation failure.
/// * `Err(CelError::TypeError { .. })` — expression returned a non-boolean.
///
/// Items for which the expression evaluates to `false` are silently
/// skipped (filter semantics).
///
/// Exposes two observable counters incremented during iteration:
///
/// * [`Self::eval_errors`] — count of evaluation errors yielded.
/// * [`Self::eval_timeouts`] — count of evaluations that exceeded
///   `options.max_eval_time` (best-effort, post-hoc).
pub struct CelFilterResult<I, T>
where
    I: Iterator<Item = T>,
    T: IntoCelContext,
{
    iter: I,
    program: Program,
    expression: String,
    options: CelOptions,
    eval_errors: Arc<AtomicU64>,
    eval_timeouts: Arc<AtomicU64>,
}

impl<I, T> std::fmt::Debug for CelFilterResult<I, T>
where
    I: Iterator<Item = T> + std::fmt::Debug,
    T: IntoCelContext,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CelFilterResult")
            .field("iter", &self.iter)
            .field("expression", &self.expression)
            .field("options", &self.options)
            .field("eval_errors", &self.eval_errors.load(Ordering::Relaxed))
            .field("eval_timeouts", &self.eval_timeouts.load(Ordering::Relaxed))
            .finish()
    }
}

impl<I, T> CelFilterResult<I, T>
where
    I: Iterator<Item = T>,
    T: IntoCelContext,
{
    fn new(iter: I, expr: &str, options: CelOptions) -> CelResult<Self> {
        let program = Program::compile(expr).map_err(|e| CelError::CompileError {
            expression: expr.to_string(),
            message: e.to_string(),
        })?;
        Ok(Self {
            iter,
            program,
            expression: expr.to_string(),
            options,
            eval_errors: Arc::new(AtomicU64::new(0)),
            eval_timeouts: Arc::new(AtomicU64::new(0)),
        })
    }

    /// Count of evaluation errors observed so far. Cloneable handle
    /// suitable for external monitoring.
    #[must_use]
    pub fn eval_errors_handle(&self) -> Arc<AtomicU64> {
        Arc::clone(&self.eval_errors)
    }

    /// Count of evaluation errors observed so far.
    #[must_use]
    pub fn eval_errors(&self) -> u64 {
        self.eval_errors.load(Ordering::Relaxed)
    }

    /// Count of evaluations that exceeded `options.max_eval_time` so far.
    /// Cloneable handle suitable for external monitoring.
    #[must_use]
    pub fn eval_timeouts_handle(&self) -> Arc<AtomicU64> {
        Arc::clone(&self.eval_timeouts)
    }

    /// Count of evaluations that exceeded `options.max_eval_time` so far.
    #[must_use]
    pub fn eval_timeouts(&self) -> u64 {
        self.eval_timeouts.load(Ordering::Relaxed)
    }

    /// Current options.
    #[must_use]
    pub const fn options(&self) -> &CelOptions {
        &self.options
    }
}

impl<I, T> Iterator for CelFilterResult<I, T>
where
    I: Iterator<Item = T>,
    T: IntoCelContext,
{
    type Item = CelResult<T>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let item = self.iter.next()?;
            let ctx = item.into_cel_context();

            let started = self.options.max_eval_time.map(|_| Instant::now());
            let result = self.program.execute(&ctx);
            if let (Some(budget), Some(start)) = (self.options.max_eval_time, started) {
                if start.elapsed() > budget {
                    self.eval_timeouts.fetch_add(1, Ordering::Relaxed);
                }
            }

            match result {
                Ok(cel_interpreter::Value::Bool(true)) => return Some(Ok(item)),
                Ok(cel_interpreter::Value::Bool(false)) => continue,
                Ok(v) => {
                    self.eval_errors.fetch_add(1, Ordering::Relaxed);
                    return Some(Err(CelError::TypeError {
                        expected: "bool".to_string(),
                        actual: format!("{v:?}"),
                    }));
                }
                Err(e) => {
                    self.eval_errors.fetch_add(1, Ordering::Relaxed);
                    return Some(Err(CelError::EvaluationError {
                        expression: self.expression.clone(),
                        message: e.to_string(),
                    }));
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(deprecated)]
    use super::*;
    use cel_interpreter::Context;

    #[derive(Debug)]
    struct TestItem {
        value: i64,
        active: bool,
    }

    impl IntoCelContext for TestItem {
        fn into_cel_context(&self) -> Context<'static> {
            let mut ctx = Context::default();
            let _ = ctx.add_variable("value", self.value);
            let _ = ctx.add_variable("active", self.active);
            ctx
        }
    }

    #[test]
    fn test_cel_filter() {
        let items = vec![
            TestItem {
                value: 5,
                active: true,
            },
            TestItem {
                value: 15,
                active: true,
            },
            TestItem {
                value: 25,
                active: false,
            },
        ];

        let filtered: Vec<TestItem> = items.into_iter().cel_filter("value > 10").collect();

        assert_eq!(filtered.len(), 2);
        assert_eq!(filtered[0].value, 15);
        assert_eq!(filtered[1].value, 25);
    }

    #[test]
    fn test_cel_filter_boolean() {
        let items = vec![
            TestItem {
                value: 5,
                active: true,
            },
            TestItem {
                value: 15,
                active: false,
            },
        ];

        let filtered: Vec<TestItem> = items.into_iter().cel_filter("active").collect();

        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].value, 5);
    }

    #[test]
    fn test_cel_filter_combined() {
        let items = vec![
            TestItem {
                value: 5,
                active: true,
            },
            TestItem {
                value: 15,
                active: true,
            },
            TestItem {
                value: 25,
                active: false,
            },
        ];

        let filtered: Vec<TestItem> = items
            .into_iter()
            .cel_filter("value > 10 && active")
            .collect();

        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].value, 15);
    }

    #[test]
    fn cel_filter_propagates_eval_error() {
        // Reference an undeclared variable: this compiles but errors at
        // runtime with `ExecutionError::UndeclaredReference`.
        let items = vec![TestItem {
            value: 1,
            active: true,
        }];

        let mut iter = items
            .into_iter()
            .cel_filter_result("nonexistent_var > 0")
            .expect("compile should succeed");

        let first = iter.next();
        match first {
            Some(Err(CelError::EvaluationError { .. })) => {}
            other => panic!("expected Some(Err(EvaluationError)), got {other:?}"),
        }
    }

    #[test]
    fn cel_filter_eval_error_counter_increments() {
        let items = vec![
            TestItem {
                value: 1,
                active: true,
            },
            TestItem {
                value: 2,
                active: false,
            },
        ];

        let iter = items
            .into_iter()
            .cel_filter_result("nonexistent_var > 0")
            .expect("compile should succeed");

        let counter = iter.eval_errors_handle();
        let results: Vec<_> = iter.collect();

        // Two items, each yields one Err.
        assert_eq!(results.len(), 2);
        assert!(results.iter().all(std::result::Result::is_err));
        assert_eq!(counter.load(Ordering::Relaxed), 2);
    }

    #[test]
    fn cel_filter_type_error_counter_increments() {
        // Non-bool return type increments the same counter.
        let items = vec![TestItem {
            value: 7,
            active: true,
        }];

        let iter = items
            .into_iter()
            .cel_filter_result("value + 1")
            .expect("compile should succeed");
        let counter = iter.eval_errors_handle();
        let results: Vec<_> = iter.collect();

        assert_eq!(results.len(), 1);
        assert!(matches!(
            results.first(),
            Some(Err(CelError::TypeError { .. }))
        ));
        assert_eq!(counter.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn cel_options_max_eval_time_records_overrun() {
        // Use a zero budget so any non-trivial evaluation counts as an
        // overrun. This makes the test robust to fast machines.
        let items = vec![
            TestItem {
                value: 1,
                active: true,
            },
            TestItem {
                value: 2,
                active: true,
            },
            TestItem {
                value: 3,
                active: true,
            },
        ];

        let options = CelOptions::new().with_max_eval_time(Duration::from_nanos(0));
        let iter = items
            .into_iter()
            .cel_filter_result_with_options("value > 0", options)
            .expect("compile should succeed");

        let overruns = iter.eval_timeouts_handle();
        let kept: Vec<_> = iter.filter_map(std::result::Result::ok).collect();

        assert_eq!(kept.len(), 3);
        // Every eval should have observed a non-zero elapsed > 0ns.
        assert_eq!(overruns.load(Ordering::Relaxed), 3);
    }

    #[test]
    fn cel_options_no_budget_no_overruns() {
        let items = vec![TestItem {
            value: 1,
            active: true,
        }];

        let iter = items
            .into_iter()
            .cel_filter_result("value > 0")
            .expect("compile should succeed");
        let overruns = iter.eval_timeouts_handle();
        let _: Vec<_> = iter.collect();

        assert_eq!(overruns.load(Ordering::Relaxed), 0);
    }
}
