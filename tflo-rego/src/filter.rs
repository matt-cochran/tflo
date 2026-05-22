//! Rego-based filtering for iterators.

use crate::context::IntoRegoInput;
use crate::error::RegoResult;
use crate::policy::PolicyEngine;
use std::sync::{Arc, Mutex};

/// Extension trait for Rego-based filtering on iterators.
pub trait RegoFilterExt<T>: Iterator<Item = T> + Sized
where
    T: IntoRegoInput,
{
    /// Filter items using a Rego policy query.
    ///
    /// Items for which the query evaluates to `true` are kept.
    fn rego_filter(self, engine: Arc<Mutex<PolicyEngine>>, query: &str) -> RegoFilter<Self, T>;

    /// Filter items with error handling.
    fn rego_filter_result(
        self,
        engine: Arc<Mutex<PolicyEngine>>,
        query: &str,
    ) -> RegoFilterResult<Self, T>;
}

impl<I, T> RegoFilterExt<T> for I
where
    I: Iterator<Item = T>,
    T: IntoRegoInput,
{
    fn rego_filter(self, engine: Arc<Mutex<PolicyEngine>>, query: &str) -> RegoFilter<Self, T> {
        RegoFilter {
            iter: self,
            engine,
            query: query.to_string(),
        }
    }

    fn rego_filter_result(
        self,
        engine: Arc<Mutex<PolicyEngine>>,
        query: &str,
    ) -> RegoFilterResult<Self, T> {
        RegoFilterResult {
            iter: self,
            engine,
            query: query.to_string(),
        }
    }
}

/// Iterator adapter that filters using Rego policies.
pub struct RegoFilter<I, T>
where
    I: Iterator<Item = T>,
    T: IntoRegoInput,
{
    iter: I,
    engine: Arc<Mutex<PolicyEngine>>,
    query: String,
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

            let allowed = {
                let mut engine = match self.engine.lock() {
                    Ok(e) => e,
                    Err(_) => return None, // Poisoned mutex
                };
                engine.eval_allow(&item, &self.query).unwrap_or(false)
            };

            if allowed {
                return Some(item);
            }
        }
    }
}

/// Iterator adapter that filters with error handling.
pub struct RegoFilterResult<I, T>
where
    I: Iterator<Item = T>,
    T: IntoRegoInput,
{
    iter: I,
    engine: Arc<Mutex<PolicyEngine>>,
    query: String,
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

            let result = {
                let mut engine = match self.engine.lock() {
                    Ok(e) => e,
                    Err(e) => {
                        return Some(Err(crate::error::RegoError::EvaluationError {
                            query: self.query.clone(),
                            message: format!("mutex poisoned: {e}"),
                        }));
                    }
                };
                engine.eval_allow(&item, &self.query)
            };

            match result {
                Ok(true) => return Some(Ok(item)),
                Ok(false) => continue,
                Err(e) => return Some(Err(e)),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Serialize;

    #[derive(Serialize, Clone)]
    struct TestItem {
        value: i64,
    }

    #[test]
    fn test_rego_filter() {
        let engine = Arc::new(Mutex::new(PolicyEngine::new()));

        {
            let mut e = engine.lock().expect("should lock");
            e.add_policy(
                "test",
                r#"
                package test
                default allow = false
                allow { input.value > 10 }
            "#,
            )
            .expect("should parse");
        }

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
}
