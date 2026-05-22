//! Rhai-based filtering for iterators.

use crate::context::IntoRhaiScope;
use crate::error::{RhaiError, RhaiResult};
use rhai::{AST, Engine};
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
    /// Panics if the Rhai expression fails to compile.
    fn rhai_filter(self, expr: &str) -> RhaiFilter<Self, T> {
        RhaiFilter::new(self, expr)
    }

    /// Filter items using a Rhai expression with a custom engine.
    fn rhai_filter_with_engine(self, engine: Arc<Engine>, expr: &str) -> RhaiFilter<Self, T> {
        RhaiFilter::with_engine(self, engine, expr)
    }

    /// Filter items using a Rhai expression, returning errors.
    fn rhai_filter_result(self, expr: &str) -> RhaiResult<RhaiFilterResult<Self, T>> {
        RhaiFilterResult::new(self, expr)
    }
}

impl<I, T> RhaiFilterExt<T> for I
where
    I: Iterator<Item = T>,
    T: IntoRhaiScope,
{
}

/// Iterator adapter that filters using Rhai expressions.
pub struct RhaiFilter<I, T>
where
    I: Iterator<Item = T>,
    T: IntoRhaiScope,
{
    iter: I,
    engine: Arc<Engine>,
    ast: AST,
}

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

impl<I, T> RhaiFilter<I, T>
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
    /// Panics if `expr` is not a valid Rhai expression. The fallible
    /// [`RhaiFilterResult`] path returns the compile error instead.
    #[allow(clippy::panic)]
    fn with_engine(iter: I, engine: Arc<Engine>, expr: &str) -> Self {
        let ast = engine
            .compile(expr)
            .unwrap_or_else(|e| panic!("failed to compile Rhai expression '{expr}': {e}"));
        Self { iter, engine, ast }
    }
}

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
                Ok(false) => continue,
                Err(_) => continue, // Evaluation error treated as false
            }
        }
    }
}

/// Iterator adapter that filters with error handling.
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
                Ok(false) => continue,
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
mod tests {
    use super::*;
    use rhai::Scope;

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
}
