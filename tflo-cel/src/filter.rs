//! CEL-based filtering for iterators.

use crate::error::{CelError, CelResult};
use crate::traits::IntoCelContext;
use cel_interpreter::Program;

/// Extension trait for CEL-based filtering on iterators.
///
/// # Examples
///
/// ```ignore
/// use tflo_cel::prelude::*;
///
/// let filtered: Vec<Detection> = detections.into_iter()
///     .cel_filter("snr > 10.0 && freq_mhz > 100.0")
///     .collect();
/// ```
pub trait CelFilterExt<T>: Iterator<Item = T> + Sized
where
    T: IntoCelContext,
{
    /// Filter items using a CEL expression.
    ///
    /// Items for which the expression evaluates to `true` are kept.
    ///
    /// # Panics
    ///
    /// Panics if the CEL expression fails to compile.
    fn cel_filter(self, expr: &str) -> CelFilter<Self, T> {
        CelFilter::new(self, expr)
    }

    /// Filter items using a CEL expression, returning errors.
    ///
    /// Unlike `cel_filter`, this returns a `Result` for each item,
    /// allowing the caller to handle evaluation errors.
    ///
    /// # Errors
    ///
    /// Returns [`CelError::CompileError`](crate::error::CelError::CompileError)
    /// when `expr` fails to compile as a CEL program.
    fn cel_filter_result(self, expr: &str) -> CelResult<CelFilterResult<Self, T>> {
        CelFilterResult::new(self, expr)
    }
}

impl<I, T> CelFilterExt<T> for I
where
    I: Iterator<Item = T>,
    T: IntoCelContext,
{
}

/// Iterator adapter that filters using CEL expressions.
pub struct CelFilter<I, T>
where
    I: Iterator<Item = T>,
    T: IntoCelContext,
{
    iter: I,
    program: Program,
}

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

impl<I, T> CelFilter<I, T>
where
    I: Iterator<Item = T>,
    T: IntoCelContext,
{
    /// # Panics
    ///
    /// Panics if `expr` is not a valid CEL expression. The fallible
    /// [`CelFilterResult`] path returns the compile error instead.
    #[allow(clippy::panic)]
    fn new(iter: I, expr: &str) -> Self {
        let program = Program::compile(expr)
            .unwrap_or_else(|e| panic!("failed to compile CEL expression '{expr}': {e}"));
        Self { iter, program }
    }
}

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
pub struct CelFilterResult<I, T>
where
    I: Iterator<Item = T>,
    T: IntoCelContext,
{
    iter: I,
    program: Program,
    expression: String,
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
            .finish()
    }
}

impl<I, T> CelFilterResult<I, T>
where
    I: Iterator<Item = T>,
    T: IntoCelContext,
{
    fn new(iter: I, expr: &str) -> CelResult<Self> {
        let program = Program::compile(expr).map_err(|e| CelError::CompileError {
            expression: expr.to_string(),
            message: e.to_string(),
        })?;
        Ok(Self {
            iter,
            program,
            expression: expr.to_string(),
        })
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

            match self.program.execute(&ctx) {
                Ok(cel_interpreter::Value::Bool(true)) => return Some(Ok(item)),
                Ok(cel_interpreter::Value::Bool(false)) => continue,
                Ok(v) => {
                    return Some(Err(CelError::TypeError {
                        expected: "bool".to_string(),
                        actual: format!("{v:?}"),
                    }));
                }
                Err(e) => {
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
    use super::*;
    use cel_interpreter::Context;

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
}
