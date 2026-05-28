//! Config-file loaders for [`PolicyEngine`] — Rego files, JSON data, and
//! directory ingestion.

use crate::error::{RegoError, RegoResult};
use crate::policy::PolicyEngine;
use std::fs;
use std::path::Path;

impl PolicyEngine {
    /// Add a policy from a string.
    ///
    /// # Errors
    ///
    /// Returns [`RegoError::ParseError`]
    /// when `policy` is not a valid Rego policy.
    pub fn add_policy(&mut self, name: &str, policy: &str) -> RegoResult<()> {
        let _ = self
            .engine
            .add_policy(name.to_string(), policy.to_string())
            .map_err(|e| RegoError::ParseError {
                policy: name.to_string(),
                message: e.to_string(),
            })?;
        Ok(())
    }

    /// Add a policy from a file.
    ///
    /// # Errors
    ///
    /// Returns [`RegoError::IoError`] when
    /// `path` cannot be read, plus any error from
    /// [`add_policy`](Self::add_policy) for parsing.
    pub fn add_policy_from_file<P: AsRef<Path>>(&mut self, path: P) -> RegoResult<()> {
        let content = fs::read_to_string(path.as_ref())?;
        let name = path
            .as_ref()
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("policy");
        self.add_policy(name, &content)
    }

    /// Add all policies from a directory.
    ///
    /// # Errors
    ///
    /// Returns [`RegoError::IoError`] when
    /// `path` cannot be read or entries cannot be enumerated, plus any
    /// error from [`add_policy_from_file`](Self::add_policy_from_file) for
    /// each `.rego` file found.
    pub fn add_policies_from_directory<P: AsRef<Path>>(&mut self, path: P) -> RegoResult<usize> {
        let mut count = 0;
        for entry in fs::read_dir(path)? {
            let entry = entry?;
            let file_path = entry.path();
            if file_path.extension().is_some_and(|e| e == "rego") {
                self.add_policy_from_file(&file_path)?;
                // SAFETY: bounded by directory entry count; no overflow
                // possible at any realistic filesystem size.
                #[allow(clippy::arithmetic_side_effects)]
                {
                    count += 1;
                }
            }
        }
        Ok(count)
    }

    /// Add static data that policies can reference.
    ///
    /// # Errors
    ///
    /// Returns
    /// [`RegoError::EvaluationError`]
    /// when the underlying engine rejects the supplied data.
    pub fn add_data(&mut self, data: serde_json::Value) -> RegoResult<()> {
        let rego_value: regorus::Value = data.into();
        self.engine
            .add_data(rego_value)
            .map_err(|e| RegoError::EvaluationError {
                query: "data".to_string(),
                message: e.to_string(),
            })?;
        Ok(())
    }

    /// Add data from a JSON file.
    ///
    /// # Errors
    ///
    /// Returns [`RegoError::IoError`] when
    /// `path` cannot be read,
    /// [`RegoError::SerializationError`] when
    /// the file is not valid JSON, plus any error from
    /// [`add_data`](Self::add_data).
    pub fn add_data_from_file<P: AsRef<Path>>(&mut self, path: P) -> RegoResult<()> {
        let content = fs::read_to_string(path)?;
        let data: serde_json::Value = serde_json::from_str(&content)?;
        self.add_data(data)
    }
}
