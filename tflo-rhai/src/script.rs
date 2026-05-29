//! Rhai script engine with caching.

use crate::options::RhaiOptions;
use rhai::{AST, Engine};
use std::collections::HashMap;
use std::sync::Arc;

/// Rhai script engine with caching and custom functions.
///
/// Compile/eval/load methods live in [`crate::script_exec`].
#[derive(Debug)]
pub struct ScriptEngine {
    pub(crate) engine: Arc<Engine>,
    pub(crate) scripts: HashMap<String, AST>,
}

impl Default for ScriptEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl ScriptEngine {
    /// Create a new script engine with conservative DoS-mitigation
    /// resource caps applied via [`RhaiOptions::default`].
    #[must_use]
    pub fn new() -> Self {
        Self::with_options(&RhaiOptions::default())
    }

    /// Create a script engine with the given Rhai resource budgets.
    #[must_use]
    pub fn with_options(options: &RhaiOptions) -> Self {
        Self {
            engine: Arc::new(options.build_engine()),
            scripts: HashMap::new(),
        }
    }

    /// Create a script engine with a caller-supplied Rhai engine. The
    /// caller owns the engine's configuration — no [`RhaiOptions`] are
    /// applied here.
    #[must_use]
    pub fn with_engine(engine: Engine) -> Self {
        Self {
            engine: Arc::new(engine),
            scripts: HashMap::new(),
        }
    }

    /// Get the underlying Rhai engine.
    #[must_use]
    pub fn engine(&self) -> &Engine {
        &self.engine
    }

    /// Check if a script is loaded.
    #[must_use]
    pub fn has_script(&self, name: &str) -> bool {
        self.scripts.contains_key(name)
    }

    /// Get the number of cached scripts.
    #[must_use]
    pub fn script_count(&self) -> usize {
        self.scripts.len()
    }

    /// Remove a cached script.
    pub fn remove(&mut self, name: &str) -> bool {
        self.scripts.remove(name).is_some()
    }

    /// Clear all cached scripts.
    pub fn clear(&mut self) {
        self.scripts.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::RhaiResult;
    use crate::traits::IntoRhaiScope;
    use rhai::Scope;

    struct TestContext {
        x: i64,
        y: i64,
    }

    impl IntoRhaiScope for TestContext {
        fn into_rhai_scope(&self) -> Scope<'static> {
            let mut scope = Scope::new();
            let _ = scope.push("x", self.x);
            let _ = scope.push("y", self.y);
            scope
        }
    }

    #[test]
    fn test_script_engine() {
        let mut engine = ScriptEngine::new();

        engine.compile("add", "x + y").expect("should compile");
        engine.compile("mul", "x * y").expect("should compile");

        let ctx = TestContext { x: 3, y: 4 };

        let sum: i64 = engine.eval("add", &ctx).expect("should eval");
        assert_eq!(sum, 7);

        let product: i64 = engine.eval("mul", &ctx).expect("should eval");
        assert_eq!(product, 12);
    }

    #[test]
    fn test_eval_expression() {
        let engine = ScriptEngine::new();
        let ctx = TestContext { x: 10, y: 5 };

        let result: i64 = engine.eval_expression("x - y", &ctx).expect("should eval");
        assert_eq!(result, 5);
    }

    #[test]
    fn test_missing_script() {
        let engine = ScriptEngine::new();
        let ctx = TestContext { x: 1, y: 2 };

        let result: RhaiResult<i64> = engine.eval("nonexistent", &ctx);
        assert!(result.is_err());
    }
}
