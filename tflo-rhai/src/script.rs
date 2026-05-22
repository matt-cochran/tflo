//! Rhai script engine with caching.

use crate::context::IntoRhaiScope;
use crate::error::{RhaiError, RhaiResult};
use rhai::{AST, Dynamic, Engine};
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::sync::Arc;

/// Rhai script engine with caching and custom functions.
#[derive(Debug)]
pub struct ScriptEngine {
    engine: Arc<Engine>,
    scripts: HashMap<String, AST>,
}

impl Default for ScriptEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl ScriptEngine {
    /// Create a new script engine.
    #[must_use]
    pub fn new() -> Self {
        Self {
            engine: Arc::new(Engine::new()),
            scripts: HashMap::new(),
        }
    }

    /// Create a script engine with a custom Rhai engine.
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

    /// Compile and cache a script.
    pub fn compile(&mut self, name: &str, script: &str) -> RhaiResult<()> {
        let ast = self
            .engine
            .compile(script)
            .map_err(|e| RhaiError::CompileError {
                script: script.to_string(),
                message: e.to_string(),
            })?;
        let _ = self.scripts.insert(name.to_string(), ast);
        Ok(())
    }

    /// Load and compile a script from a file.
    pub fn load_file<P: AsRef<Path>>(&mut self, name: &str, path: P) -> RhaiResult<()> {
        let content = fs::read_to_string(path)?;
        self.compile(name, &content)
    }

    /// Load all scripts from a directory.
    pub fn load_directory<P: AsRef<Path>>(&mut self, path: P) -> RhaiResult<usize> {
        let mut count = 0;
        for entry in fs::read_dir(path)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().is_some_and(|e| e == "rhai") {
                if let Some(name) = path.file_stem().and_then(|s| s.to_str()) {
                    self.load_file(name, &path)?;
                    count += 1;
                }
            }
        }
        Ok(count)
    }

    /// Evaluate a cached script.
    pub fn eval<T: IntoRhaiScope, R: Clone + 'static>(
        &self,
        script_name: &str,
        context: &T,
    ) -> RhaiResult<R> {
        let ast = self
            .scripts
            .get(script_name)
            .ok_or_else(|| RhaiError::ScriptError {
                message: format!("script not found: {script_name}"),
            })?;

        let mut scope = context.into_rhai_scope();
        self.engine
            .eval_ast_with_scope::<R>(&mut scope, ast)
            .map_err(|e| RhaiError::EvaluationError {
                script: script_name.to_string(),
                message: e.to_string(),
            })
    }

    /// Evaluate a cached script and return Dynamic.
    pub fn eval_dynamic<T: IntoRhaiScope>(
        &self,
        script_name: &str,
        context: &T,
    ) -> RhaiResult<Dynamic> {
        let ast = self
            .scripts
            .get(script_name)
            .ok_or_else(|| RhaiError::ScriptError {
                message: format!("script not found: {script_name}"),
            })?;

        let mut scope = context.into_rhai_scope();
        self.engine
            .eval_ast_with_scope::<Dynamic>(&mut scope, ast)
            .map_err(|e| RhaiError::EvaluationError {
                script: script_name.to_string(),
                message: e.to_string(),
            })
    }

    /// Evaluate an expression directly.
    pub fn eval_expression<T: IntoRhaiScope, R: Clone + 'static>(
        &self,
        expr: &str,
        context: &T,
    ) -> RhaiResult<R> {
        let mut scope = context.into_rhai_scope();
        self.engine
            .eval_with_scope::<R>(&mut scope, expr)
            .map_err(|e| RhaiError::EvaluationError {
                script: expr.to_string(),
                message: e.to_string(),
            })
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
