//! Rhai-based transformation for iterators.

use crate::error::{RhaiError, RhaiResult};
use crate::options::RhaiOptions;
use crate::traits::IntoRhaiScope;
use rhai::{AST, Dynamic, Engine};
use std::sync::Arc;

/// Extension trait for Rhai-based transformation on iterators.
pub trait RhaiMapExt<T>: Iterator<Item = T> + Sized
where
    T: IntoRhaiScope,
{
    /// Transform items using a Rhai expression.
    ///
    /// PANICS on compile error; use [`rhai_map_result`](Self::rhai_map_result)
    /// — Result-returning variant — to surface compile errors instead.
    ///
    /// # Panics
    ///
    /// Panics if the Rhai expression fails to compile.
    #[deprecated(
        since = "0.2.0",
        note = "use rhai_map_result(...) — Result-returning variant — to surface compile errors instead of panicking; the panicking constructor is also DoS-able because it builds a Rhai engine with no resource caps"
    )]
    fn rhai_map(self, expr: &str) -> RhaiMap<Self, T> {
        RhaiMap::new(self, expr)
    }

    /// Transform items using a Rhai expression with a custom engine.
    #[deprecated(
        since = "0.2.0",
        note = "use rhai_map_result_with_engine(...) — Result-returning variant — to surface compile errors instead of panicking"
    )]
    fn rhai_map_with_engine(self, engine: Arc<Engine>, expr: &str) -> RhaiMap<Self, T> {
        RhaiMap::with_engine(self, engine, expr)
    }

    /// Transform items, returning errors.
    ///
    /// Engine is built from [`RhaiOptions::default`] which applies
    /// conservative resource caps (`max_operations`, `max_call_levels`)
    /// so adversarial scripts cannot `DoS` the host. Use
    /// [`rhai_map_result_with_options`](Self::rhai_map_result_with_options)
    /// to pick a different budget, or
    /// [`rhai_map_result_with_engine`](Self::rhai_map_result_with_engine)
    /// to share a pre-configured engine.
    ///
    /// # Errors
    ///
    /// Returns
    /// [`RhaiError::CompileError`]
    /// when `expr` fails to compile as a Rhai expression.
    fn rhai_map_result(self, expr: &str) -> RhaiResult<RhaiMapResult<Self, T>> {
        RhaiMapResult::new(self, expr)
    }

    /// Transform items with custom Rhai resource budgets.
    ///
    /// # Errors
    ///
    /// Returns [`RhaiError::CompileError`] when `expr` fails to compile.
    fn rhai_map_result_with_options(
        self,
        expr: &str,
        options: RhaiOptions,
    ) -> RhaiResult<RhaiMapResult<Self, T>> {
        RhaiMapResult::with_options(self, expr, options)
    }

    /// Transform items using a caller-supplied Rhai engine.
    ///
    /// # Errors
    ///
    /// Returns [`RhaiError::CompileError`] when `expr` fails to compile.
    fn rhai_map_result_with_engine(
        self,
        engine: Arc<Engine>,
        expr: &str,
    ) -> RhaiResult<RhaiMapResult<Self, T>> {
        RhaiMapResult::with_engine(self, engine, expr)
    }
}

impl<I, T> RhaiMapExt<T> for I
where
    I: Iterator<Item = T>,
    T: IntoRhaiScope,
{
}

/// Iterator adapter that transforms using Rhai expressions.
pub struct RhaiMap<I, T>
where
    I: Iterator<Item = T>,
    T: IntoRhaiScope,
{
    iter: I,
    engine: Arc<Engine>,
    ast: AST,
}

impl<I, T> std::fmt::Debug for RhaiMap<I, T>
where
    I: Iterator<Item = T> + std::fmt::Debug,
    T: IntoRhaiScope,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RhaiMap").field("iter", &self.iter).finish()
    }
}

impl<I, T> RhaiMap<I, T>
where
    I: Iterator<Item = T>,
    T: IntoRhaiScope,
{
    fn new(iter: I, expr: &str) -> Self {
        let engine = Arc::new(Engine::new());
        Self::with_engine(iter, engine, expr)
    }

    /// # Panics
    ///
    /// Panics if `expr` is not a valid Rhai expression.
    #[allow(clippy::panic)]
    fn with_engine(iter: I, engine: Arc<Engine>, expr: &str) -> Self {
        let ast = engine
            .compile(expr)
            .unwrap_or_else(|e| panic!("failed to compile Rhai expression '{expr}': {e}"));
        Self { iter, engine, ast }
    }
}

impl<I, T> Iterator for RhaiMap<I, T>
where
    I: Iterator<Item = T>,
    T: IntoRhaiScope,
{
    type Item = Dynamic;

    fn next(&mut self) -> Option<Self::Item> {
        let item = self.iter.next()?;
        let mut scope = item.into_rhai_scope();

        match self
            .engine
            .eval_ast_with_scope::<Dynamic>(&mut scope, &self.ast)
        {
            Ok(result) => Some(result),
            Err(_) => Some(Dynamic::UNIT), // Return unit on error
        }
    }
}

/// Iterator adapter that transforms with error handling.
pub struct RhaiMapResult<I, T>
where
    I: Iterator<Item = T>,
    T: IntoRhaiScope,
{
    iter: I,
    engine: Arc<Engine>,
    ast: AST,
    expression: String,
}

impl<I, T> std::fmt::Debug for RhaiMapResult<I, T>
where
    I: Iterator<Item = T> + std::fmt::Debug,
    T: IntoRhaiScope,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RhaiMapResult")
            .field("iter", &self.iter)
            .field("expression", &self.expression)
            .finish()
    }
}

impl<I, T> RhaiMapResult<I, T>
where
    I: Iterator<Item = T>,
    T: IntoRhaiScope,
{
    fn new(iter: I, expr: &str) -> RhaiResult<Self> {
        Self::with_options(iter, expr, RhaiOptions::default())
    }

    fn with_options(iter: I, expr: &str, options: RhaiOptions) -> RhaiResult<Self> {
        let engine = Arc::new(options.build_engine());
        Self::with_engine(iter, engine, expr)
    }

    fn with_engine(iter: I, engine: Arc<Engine>, expr: &str) -> RhaiResult<Self> {
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
}

impl<I, T> Iterator for RhaiMapResult<I, T>
where
    I: Iterator<Item = T>,
    T: IntoRhaiScope,
{
    type Item = RhaiResult<Dynamic>;

    fn next(&mut self) -> Option<Self::Item> {
        let item = self.iter.next()?;
        let mut scope = item.into_rhai_scope();

        Some(
            self.engine
                .eval_ast_with_scope::<Dynamic>(&mut scope, &self.ast)
                .map_err(|e| RhaiError::EvaluationError {
                    script: self.expression.clone(),
                    message: e.to_string(),
                }),
        )
    }
}

/// Extension trait for transforming and keeping original items.
pub trait RhaiEnrichExt<T>: Iterator<Item = T> + Sized
where
    T: IntoRhaiScope + Clone,
{
    /// Transform items and keep both original and result.
    ///
    /// PANICS on compile error and silently coerces evaluation errors to
    /// `Dynamic::UNIT`. Use [`rhai_enrich_result`](Self::rhai_enrich_result)
    /// — the `Result`-returning variant — to surface failures.
    #[deprecated(
        since = "0.2.0",
        note = "use rhai_enrich_result(...) — Result-returning variant — to surface compile and evaluation errors instead of panicking / silently substituting Dynamic::UNIT; the panicking constructor is also DoS-able because it builds a Rhai engine with no resource caps"
    )]
    #[allow(deprecated)]
    fn rhai_enrich(self, expr: &str) -> RhaiEnrich<Self, T> {
        RhaiEnrich::new(self, expr)
    }

    /// Transform items and keep both original and result, propagating
    /// evaluation errors as `Result`.
    ///
    /// Engine is built from [`RhaiOptions::default`] which applies
    /// conservative resource caps (`max_operations`, `max_call_levels`)
    /// so adversarial scripts cannot `DoS` the host.
    ///
    /// # Errors
    ///
    /// Returns
    /// [`RhaiError::CompileError`]
    /// when `expr` fails to compile as a Rhai expression.
    fn rhai_enrich_result(self, expr: &str) -> RhaiResult<RhaiEnrichResult<Self, T>> {
        RhaiEnrichResult::new(self, expr)
    }

    /// Transform items with custom Rhai resource budgets.
    ///
    /// # Errors
    ///
    /// Returns [`RhaiError::CompileError`] when `expr` fails to compile.
    fn rhai_enrich_result_with_options(
        self,
        expr: &str,
        options: RhaiOptions,
    ) -> RhaiResult<RhaiEnrichResult<Self, T>> {
        RhaiEnrichResult::with_options(self, expr, options)
    }
}

impl<I, T> RhaiEnrichExt<T> for I
where
    I: Iterator<Item = T>,
    T: IntoRhaiScope + Clone,
{
}

/// Iterator adapter that enriches items with computed values.
pub struct RhaiEnrich<I, T>
where
    I: Iterator<Item = T>,
    T: IntoRhaiScope + Clone,
{
    iter: I,
    engine: Arc<Engine>,
    ast: AST,
}

impl<I, T> std::fmt::Debug for RhaiEnrich<I, T>
where
    I: Iterator<Item = T> + std::fmt::Debug,
    T: IntoRhaiScope + Clone,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RhaiEnrich")
            .field("iter", &self.iter)
            .finish()
    }
}

impl<I, T> RhaiEnrich<I, T>
where
    I: Iterator<Item = T>,
    T: IntoRhaiScope + Clone,
{
    /// # Panics
    ///
    /// Panics if `expr` is not a valid Rhai expression.
    #[allow(clippy::panic)]
    fn new(iter: I, expr: &str) -> Self {
        let engine = Arc::new(Engine::new());
        let ast = engine
            .compile(expr)
            .unwrap_or_else(|e| panic!("failed to compile Rhai expression '{expr}': {e}"));
        Self { iter, engine, ast }
    }
}

impl<I, T> Iterator for RhaiEnrich<I, T>
where
    I: Iterator<Item = T>,
    T: IntoRhaiScope + Clone,
{
    type Item = (T, Dynamic);

    fn next(&mut self) -> Option<Self::Item> {
        let item = self.iter.next()?;
        let mut scope = item.into_rhai_scope();

        let result = self
            .engine
            .eval_ast_with_scope::<Dynamic>(&mut scope, &self.ast)
            .unwrap_or(Dynamic::UNIT);

        Some((item, result))
    }
}

/// Iterator adapter that enriches items with computed values, surfacing
/// compile and evaluation errors as `Result` instead of panicking /
/// silently substituting [`Dynamic::UNIT`].
pub struct RhaiEnrichResult<I, T>
where
    I: Iterator<Item = T>,
    T: IntoRhaiScope + Clone,
{
    iter: I,
    engine: Arc<Engine>,
    ast: AST,
    expression: String,
}

impl<I, T> std::fmt::Debug for RhaiEnrichResult<I, T>
where
    I: Iterator<Item = T> + std::fmt::Debug,
    T: IntoRhaiScope + Clone,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RhaiEnrichResult")
            .field("iter", &self.iter)
            .field("expression", &self.expression)
            .finish()
    }
}

impl<I, T> RhaiEnrichResult<I, T>
where
    I: Iterator<Item = T>,
    T: IntoRhaiScope + Clone,
{
    fn new(iter: I, expr: &str) -> RhaiResult<Self> {
        Self::with_options(iter, expr, RhaiOptions::default())
    }

    fn with_options(iter: I, expr: &str, options: RhaiOptions) -> RhaiResult<Self> {
        let engine = Arc::new(options.build_engine());
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
}

impl<I, T> Iterator for RhaiEnrichResult<I, T>
where
    I: Iterator<Item = T>,
    T: IntoRhaiScope + Clone,
{
    type Item = RhaiResult<(T, Dynamic)>;

    fn next(&mut self) -> Option<Self::Item> {
        let item = self.iter.next()?;
        let mut scope = item.into_rhai_scope();

        match self
            .engine
            .eval_ast_with_scope::<Dynamic>(&mut scope, &self.ast)
        {
            Ok(result) => Some(Ok((item, result))),
            Err(e) => Some(Err(RhaiError::EvaluationError {
                script: self.expression.clone(),
                message: e.to_string(),
            })),
        }
    }
}

#[cfg(test)]
#[allow(clippy::indexing_slicing)]
mod tests {
    use super::*;
    use rhai::Scope;

    #[derive(Clone, Debug)]
    struct TestItem {
        x: i64,
        y: i64,
    }

    impl IntoRhaiScope for TestItem {
        fn into_rhai_scope(&self) -> Scope<'static> {
            let mut scope = Scope::new();
            let _ = scope.push("x", self.x);
            let _ = scope.push("y", self.y);
            scope
        }
    }

    #[test]
    #[allow(deprecated)]
    fn test_rhai_map() {
        let items = vec![TestItem { x: 1, y: 2 }, TestItem { x: 3, y: 4 }];

        let results: Vec<Dynamic> = items.into_iter().rhai_map("x + y").collect();

        assert_eq!(results.len(), 2);
        assert_eq!(results[0].as_int().expect("should be int"), 3);
        assert_eq!(results[1].as_int().expect("should be int"), 7);
    }

    #[test]
    #[allow(deprecated)]
    fn test_rhai_enrich() {
        let items = vec![TestItem { x: 1, y: 2 }, TestItem { x: 3, y: 4 }];

        let results: Vec<(TestItem, Dynamic)> = items.into_iter().rhai_enrich("x * y").collect();

        assert_eq!(results.len(), 2);
        assert_eq!(results[0].0.x, 1);
        assert_eq!(results[0].1.as_int().expect("should be int"), 2);
        assert_eq!(results[1].0.x, 3);
        assert_eq!(results[1].1.as_int().expect("should be int"), 12);
    }

    #[test]
    fn rhai_map_propagates_eval_error() {
        let items = vec![TestItem { x: 1, y: 2 }];
        let mut it = items
            .into_iter()
            .rhai_map_result("throw \"boom\"")
            .expect("compile ok");
        match it.next() {
            Some(Err(RhaiError::EvaluationError { .. })) => {}
            other => panic!("expected Some(Err(EvaluationError)), got {other:?}"),
        }
    }

    #[test]
    fn rhai_enrich_result_propagates_eval_error() {
        let items = vec![TestItem { x: 1, y: 2 }];
        let mut it = items
            .into_iter()
            .rhai_enrich_result("throw \"boom\"")
            .expect("compile ok");
        match it.next() {
            Some(Err(RhaiError::EvaluationError { .. })) => {}
            other => panic!("expected Some(Err(EvaluationError)), got {other:?}"),
        }
    }

    #[test]
    fn rhai_enrich_result_yields_pair_on_success() {
        let items = vec![TestItem { x: 3, y: 4 }];
        let mut it = items
            .into_iter()
            .rhai_enrich_result("x * y")
            .expect("compile ok");
        let (orig, result) = it.next().expect("at least one item").expect("eval ok");
        assert_eq!(orig.x, 3);
        assert_eq!(result.as_int().expect("int"), 12);
    }
}
