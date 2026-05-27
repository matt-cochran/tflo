//! Rhai-based filtering for iterators.

use crate::error::{RhaiError, RhaiResult};
use crate::options::RhaiOptions;
use crate::traits::IntoRhaiScope;
use rhai::{AST, Engine, ParseError};
use std::sync::Arc;

/// Extension trait for Rhai-based filtering on iterators.
pub trait RhaiFilterExt<T>: Iterator<Item = T> + Sized
where
    T: IntoRhaiScope,
{
    /// Filter items using a Rhai expression.
    ///
    /// Items for which the expression evaluates to `true` are kept.
    ///
    /// # Panics
    ///
    /// PANICS on compile error; use
    /// [`rhai_filter_result`](Self::rhai_filter_result) instead in
    /// production. Evaluation errors are also silently coerced to
    /// `false`, dropping matching items without surfacing the failure.
    #[deprecated(
        since = "0.2.0",
        note = "use rhai_filter_result(...) — Result-returning variant — to surface evaluation errors instead of silently treating them as false"
    )]
    #[allow(deprecated)]
    fn rhai_filter(self, expr: &str) -> RhaiFilter<Self, T> {
        RhaiFilter::new(self, expr)
    }

    /// Filter items using a Rhai expression with a custom engine.
    ///
    /// # Panics
    ///
    /// PANICS on compile error; use
    /// [`rhai_filter_result_with_engine`](Self::rhai_filter_result_with_engine)
    /// instead in production. Evaluation errors are silently coerced
    /// to `false`.
    #[deprecated(
        since = "0.2.0",
        note = "use rhai_filter_result_with_engine(...) — Result-returning variant — to surface evaluation errors instead of silently treating them as false"
    )]
    #[allow(deprecated)]
    fn rhai_filter_with_engine(self, engine: Arc<Engine>, expr: &str) -> RhaiFilter<Self, T> {
        RhaiFilter::with_engine(self, engine, expr)
    }

    /// Filter items using a Rhai expression, returning errors.
    ///
    /// Engine is built from [`RhaiOptions::default`] which applies
    /// conservative DoS-mitigation caps (max operations, call depth).
    /// Use [`rhai_filter_result_with_options`](Self::rhai_filter_result_with_options)
    /// or [`rhai_filter_result_with_engine`](Self::rhai_filter_result_with_engine)
    /// to override.
    ///
    /// # Errors
    ///
    /// Returns
    /// [`RhaiError::CompileError`](crate::error::RhaiError::CompileError)
    /// when `expr` fails to compile as a Rhai expression.
    fn rhai_filter_result(self, expr: &str) -> RhaiResult<RhaiFilterResult<Self, T>> {
        RhaiFilterResult::new(self, expr)
    }

    /// Filter items using a Rhai expression, returning errors, with
    /// custom engine resource budgets.
    ///
    /// # Errors
    ///
    /// Returns the underlying [`rhai::ParseError`] when `expr` fails
    /// to compile.
    fn rhai_filter_result_with_options(
        self,
        expr: &str,
        options: RhaiOptions,
    ) -> Result<RhaiFilterResult<Self, T>, ParseError> {
        RhaiFilterResult::with_options(self, expr, options)
    }

    /// Filter items using a Rhai expression, returning errors, sharing
    /// a caller-supplied engine.
    ///
    /// # Errors
    ///
    /// Returns the underlying [`rhai::ParseError`] when `expr` fails
    /// to compile.
    fn rhai_filter_result_with_engine(
        self,
        engine: Arc<Engine>,
        expr: &str,
    ) -> Result<RhaiFilterResult<Self, T>, ParseError> {
        RhaiFilterResult::with_engine(self, engine, expr)
    }
}

impl<I, T> RhaiFilterExt<T> for I
where
    I: Iterator<Item = T>,
    T: IntoRhaiScope,
{
}

/// Iterator adapter that filters using Rhai expressions.
///
/// Evaluation errors are silently treated as `false`. Prefer
/// [`RhaiFilterResult`] in production code.
#[deprecated(
    since = "0.2.0",
    note = "use rhai_filter_result(...) — Result-returning variant — to surface evaluation errors instead of silently treating them as false"
)]
pub struct RhaiFilter<I, T>
where
    I: Iterator<Item = T>,
    T: IntoRhaiScope,
{
    iter: I,
    engine: Arc<Engine>,
    ast: AST,
}

#[allow(deprecated)]
impl<I, T> std::fmt::Debug for RhaiFilter<I, T>
where
    I: Iterator<Item = T> + std::fmt::Debug,
    T: IntoRhaiScope,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RhaiFilter")
            .field("iter", &self.iter)
            .finish()
    }
}

#[allow(deprecated)]
impl<I, T> RhaiFilter<I, T>
where
    I: Iterator<Item = T>,
    T: IntoRhaiScope,
{
    #[deprecated(
        since = "0.2.0",
        note = "use RhaiFilterResult::new — surfaces compile errors as a Result instead of panicking"
    )]
    fn new(iter: I, expr: &str) -> Self {
        let engine = Arc::new(Engine::new());
        Self::with_engine(iter, engine, expr)
    }

    /// # Panics
    ///
    /// PANICS on compile error; use [`RhaiFilterResult`] instead in
    /// production. The fallible
    /// [`RhaiFilterResult::with_engine`] path returns the compile
    /// error as a `Result`.
    #[allow(clippy::panic)]
    #[deprecated(
        since = "0.2.0",
        note = "use RhaiFilterResult::with_engine — PANICS on compile error; the Result-returning variant surfaces it instead"
    )]
    fn with_engine(iter: I, engine: Arc<Engine>, expr: &str) -> Self {
        let ast = engine
            .compile(expr)
            .unwrap_or_else(|e| panic!("failed to compile Rhai expression '{expr}': {e}"));
        Self { iter, engine, ast }
    }
}

#[allow(deprecated)]
impl<I, T> Iterator for RhaiFilter<I, T>
where
    I: Iterator<Item = T>,
    T: IntoRhaiScope,
{
    type Item = T;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let item = self.iter.next()?;
            let mut scope = item.into_rhai_scope();

            match self
                .engine
                .eval_ast_with_scope::<bool>(&mut scope, &self.ast)
            {
                Ok(true) => return Some(item),
                Ok(false) => {}
                Err(_) => {} // Evaluation error treated as false
            }
        }
    }
}

/// Iterator adapter that filters with error handling.
///
/// Evaluation errors are surfaced as `Some(Err(_))` so callers can
/// react instead of silently dropping the item.
pub struct RhaiFilterResult<I, T>
where
    I: Iterator<Item = T>,
    T: IntoRhaiScope,
{
    iter: I,
    engine: Arc<Engine>,
    ast: AST,
    expression: String,
}

impl<I, T> std::fmt::Debug for RhaiFilterResult<I, T>
where
    I: Iterator<Item = T> + std::fmt::Debug,
    T: IntoRhaiScope,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RhaiFilterResult")
            .field("iter", &self.iter)
            .field("expression", &self.expression)
            .finish()
    }
}

impl<I, T> RhaiFilterResult<I, T>
where
    I: Iterator<Item = T>,
    T: IntoRhaiScope,
{
    fn new(iter: I, expr: &str) -> RhaiResult<Self> {
        let mut engine = Engine::new();
        RhaiOptions::default().apply_to(&mut engine);
        let engine = Arc::new(engine);
        let ast = engine.compile(expr).map_err(|e| RhaiError::CompileError {
            script: expr.to_string(),
            message: e.to_string(),
        })?;
        Ok(Self {
            iter,
            engine,
            ast,
            expression: expr.to_string(),
        })
    }

    fn with_options(iter: I, expr: &str, options: RhaiOptions) -> Result<Self, ParseError> {
        let engine = Arc::new(options.build_engine());
        let ast = engine.compile(expr)?;
        Ok(Self {
            iter,
            engine,
            ast,
            expression: expr.to_string(),
        })
    }

    fn with_engine(iter: I, engine: Arc<Engine>, expr: &str) -> Result<Self, ParseError> {
        let ast = engine.compile(expr)?;
        Ok(Self {
            iter,
            engine,
            ast,
            expression: expr.to_string(),
        })
    }
}

impl<I, T> Iterator for RhaiFilterResult<I, T>
where
    I: Iterator<Item = T>,
    T: IntoRhaiScope,
{
    type Item = RhaiResult<T>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let item = self.iter.next()?;
            let mut scope = item.into_rhai_scope();

            match self
                .engine
                .eval_ast_with_scope::<bool>(&mut scope, &self.ast)
            {
                Ok(true) => return Some(Ok(item)),
                Ok(false) => {}
                Err(e) => {
                    return Some(Err(RhaiError::EvaluationError {
                        script: self.expression.clone(),
                        message: e.to_string(),
                    }));
                }
            }
        }
    }
}

#[cfg(test)]
#[allow(deprecated)]
mod tests {
    use super::*;
    use rhai::Scope;

    #[derive(Debug)]
    struct TestItem {
        value: i64,
        active: bool,
    }

    impl IntoRhaiScope for TestItem {
        fn into_rhai_scope(&self) -> Scope<'static> {
            let mut scope = Scope::new();
            let _ = scope.push("value", self.value);
            let _ = scope.push("active", self.active);
            scope
        }
    }

    #[test]
    fn test_rhai_filter() {
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

        let filtered: Vec<TestItem> = items.into_iter().rhai_filter("value > 10").collect();

        assert_eq!(filtered.len(), 2);
        assert_eq!(filtered[0].value, 15);
        assert_eq!(filtered[1].value, 25);
    }

    #[test]
    fn test_rhai_filter_boolean() {
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

        let filtered: Vec<TestItem> = items.into_iter().rhai_filter("active").collect();

        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].value, 5);
    }

    #[test]
    fn test_rhai_filter_combined() {
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
            .rhai_filter("value > 10 && active")
            .collect();

        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].value, 15);
    }

    /// RHAI-001 regression: `RhaiFilterResult` MUST surface evaluation
    /// errors as `Some(Err(_))` rather than silently dropping items.
    /// The original `RhaiFilter` path coerces eval errors to `false`,
    /// which is the anti-pattern this canonical path replaces.
    #[test]
    fn rhai_filter_propagates_eval_error() {
        let items = vec![TestItem {
            value: 1,
            active: true,
        }];

        let mut iter = items
            .into_iter()
            .rhai_filter_result(r#"throw "boom""#)
            .expect("compile succeeds");

        match iter.next() {
            Some(Err(RhaiError::EvaluationError { message, .. })) => {
                assert!(
                    message.contains("boom"),
                    "expected evaluation error to mention the thrown message, got: {message}"
                );
            }
            other => panic!("expected Some(Err(EvaluationError)), got {other:?}"),
        }
        assert!(
            iter.next().is_none(),
            "iterator should be exhausted after the single item produced an error"
        );
    }

    /// RHAI-002 regression: the default `RhaiOptions::max_operations`
    /// cap MUST terminate an unbounded loop with an evaluation error
    /// when an even tighter budget is supplied.
    #[test]
    fn rhai_max_operations_caps_execution() {
        let items = vec![TestItem {
            value: 0,
            active: true,
        }];

        let opts = RhaiOptions {
            max_operations: Some(1_000),
            ..RhaiOptions::default()
        };

        let mut iter = items
            .into_iter()
            .rhai_filter_result_with_options(
                "let x = 0; while x < 1_000_000 { x += 1; } true",
                opts,
            )
            .expect("compile succeeds");

        match iter.next() {
            Some(Err(RhaiError::EvaluationError { .. })) => {}
            other => panic!("expected Some(Err(EvaluationError)) from operations cap, got {other:?}"),
        }
    }

    /// RHAI-003 regression: the `max_call_levels` cap MUST terminate
    /// unbounded recursion with an evaluation error.
    #[test]
    fn rhai_max_call_levels_caps_recursion() {
        let items = vec![TestItem {
            value: 0,
            active: true,
        }];

        let opts = RhaiOptions {
            max_call_levels: Some(8),
            ..RhaiOptions::default()
        };

        let mut iter = items
            .into_iter()
            .rhai_filter_result_with_options(
                "fn r(n) { if n == 0 { 1 } else { r(n - 1) } } r(100) > 0",
                opts,
            )
            .expect("compile succeeds");

        match iter.next() {
            Some(Err(RhaiError::EvaluationError { .. })) => {}
            other => panic!("expected Some(Err(EvaluationError)) from call-depth cap, got {other:?}"),
        }
    }
}
