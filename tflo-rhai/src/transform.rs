//! Rhai-based transformation for iterators.

use crate::context::IntoRhaiScope;
use crate::error::{RhaiError, RhaiResult};
use rhai::{AST, Dynamic, Engine};
use std::sync::Arc;

/// Extension trait for Rhai-based transformation on iterators.
pub trait RhaiMapExt<T>: Iterator<Item = T> + Sized
where
    T: IntoRhaiScope,
{
    /// Transform items using a Rhai expression.
    ///
    /// The expression should return a Dynamic value.
    ///
    /// # Panics
    ///
    /// Panics if the Rhai expression fails to compile.
    fn rhai_map(self, expr: &str) -> RhaiMap<Self, T> {
        RhaiMap::new(self, expr)
    }

    /// Transform items using a Rhai expression with a custom engine.
    fn rhai_map_with_engine(self, engine: Arc<Engine>, expr: &str) -> RhaiMap<Self, T> {
        RhaiMap::with_engine(self, engine, expr)
    }

    /// Transform items, returning errors.
    ///
    /// # Errors
    ///
    /// Returns
    /// [`RhaiError::CompileError`](crate::error::RhaiError::CompileError)
    /// when `expr` fails to compile as a Rhai expression.
    fn rhai_map_result(self, expr: &str) -> RhaiResult<RhaiMapResult<Self, T>> {
        RhaiMapResult::new(self, expr)
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
        let engine = Arc::new(Engine::new());
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
    fn rhai_enrich(self, expr: &str) -> RhaiEnrich<Self, T> {
        RhaiEnrich::new(self, expr)
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

#[cfg(test)]
mod tests {
    use super::*;
    use rhai::Scope;

    #[derive(Clone)]
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
    fn test_rhai_map() {
        let items = vec![TestItem { x: 1, y: 2 }, TestItem { x: 3, y: 4 }];

        let results: Vec<Dynamic> = items.into_iter().rhai_map("x + y").collect();

        assert_eq!(results.len(), 2);
        assert_eq!(results[0].as_int().expect("should be int"), 3);
        assert_eq!(results[1].as_int().expect("should be int"), 7);
    }

    #[test]
    fn test_rhai_enrich() {
        let items = vec![TestItem { x: 1, y: 2 }, TestItem { x: 3, y: 4 }];

        let results: Vec<(TestItem, Dynamic)> = items.into_iter().rhai_enrich("x * y").collect();

        assert_eq!(results.len(), 2);
        assert_eq!(results[0].0.x, 1);
        assert_eq!(results[0].1.as_int().expect("should be int"), 2);
        assert_eq!(results[1].0.x, 3);
        assert_eq!(results[1].1.as_int().expect("should be int"), 12);
    }
}
