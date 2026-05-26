//! Execution surface for [`ScriptEngine`] — compile, load, and evaluate Rhai scripts.

use crate::error::{RhaiError, RhaiResult};
use crate::script::ScriptEngine;
use crate::traits::IntoRhaiScope;
use rhai::Dynamic;
use std::fs;
use std::path::Path;

impl ScriptEngine {
    /// Compile and cache a script.
    ///
    /// # Errors
    ///
    /// Returns
    /// [`RhaiError::CompileError`](crate::error::RhaiError::CompileError)
    /// when `script` is not valid Rhai source.
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
    ///
    /// # Errors
    ///
    /// Returns [`RhaiError::IoError`](crate::error::RhaiError::IoError) when
    /// `path` cannot be read, plus any error from
    /// [`compile`](Self::compile).
    pub fn load_file<P: AsRef<Path>>(&mut self, name: &str, path: P) -> RhaiResult<()> {
        let content = fs::read_to_string(path)?;
        self.compile(name, &content)
    }

    /// Load all scripts from a directory.
    ///
    /// # Errors
    ///
    /// Returns [`RhaiError::IoError`](crate::error::RhaiError::IoError) when
    /// `path` cannot be read or entries cannot be enumerated, plus any
    /// error from [`load_file`](Self::load_file) for each `.rhai` file
    /// found.
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
    ///
    /// # Errors
    ///
    /// Returns
    /// [`RhaiError::ScriptError`](crate::error::RhaiError::ScriptError)
    /// when `script_name` has not been compiled, and
    /// [`RhaiError::EvaluationError`](crate::error::RhaiError::EvaluationError)
    /// when execution fails (type mismatch, runtime error, etc.).
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
    ///
    /// # Errors
    ///
    /// Returns
    /// [`RhaiError::ScriptError`](crate::error::RhaiError::ScriptError)
    /// when `script_name` has not been compiled, and
    /// [`RhaiError::EvaluationError`](crate::error::RhaiError::EvaluationError)
    /// when execution fails.
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
    ///
    /// # Errors
    ///
    /// Returns
    /// [`RhaiError::EvaluationError`](crate::error::RhaiError::EvaluationError)
    /// when `expr` fails to compile or execute (type mismatch, runtime
    /// error, etc.).
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
}
