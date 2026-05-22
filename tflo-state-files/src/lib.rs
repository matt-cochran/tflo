//! File-based state store backend for tflo checkpoints.
//!
//! This crate provides a `StateStore` implementation that persists
//! state snapshots to the local filesystem, one file per key.
//!
//! # Example
//!
//! ```no_run
//! use tflo_state_files::FileStateStore;
//! use tflo_core::keyed::{StateSnapshot, SnapshotMetadata, StateStore};
//!
//! # fn main() -> Result<(), String> {
//!
//! let store = FileStateStore::new("/path/to/checkpoints")?;
//!
//! let snapshot = StateSnapshot {
//!     data: vec![1, 2, 3],
//!     metadata: SnapshotMetadata {
//!         key: Some(b"my_key".to_vec()),
//!         timestamp_ms: 1000,
//!         version: 1,
//!     },
//! };
//!
//! store.save(b"my_key", &snapshot)?;
//! let _loaded = store.load(b"my_key")?;
//! # Ok(())
//! # }
//! ```

#![warn(missing_docs)]
#![warn(missing_debug_implementations)]
#![deny(unsafe_code)]

use std::fs;
use std::path::{Path, PathBuf};
use tflo_core::keyed::{StateSnapshot, StateStore};

/// File-based state store implementation.
///
/// Stores each key's snapshot as a separate file in a directory.
/// File names are hex-encoded keys for safe filesystem usage.
#[derive(Debug, Clone)]
pub struct FileStateStore {
    base_dir: PathBuf,
}

impl FileStateStore {
    /// Create a new file-based state store.
    ///
    /// The directory will be created if it doesn't exist.
    ///
    /// # Errors
    ///
    /// Returns an error if the directory cannot be created or accessed.
    pub fn new<P: AsRef<Path>>(base_dir: P) -> Result<Self, String> {
        let base_dir = base_dir.as_ref().to_path_buf();
        fs::create_dir_all(&base_dir).map_err(|e| format!("Failed to create directory: {e}"))?;
        Ok(Self { base_dir })
    }

    fn key_to_path(&self, key: &[u8]) -> PathBuf {
        let hex_key = hex::encode(key);
        self.base_dir.join(format!("{hex_key}.snapshot"))
    }
}

impl StateStore for FileStateStore {
    fn save(&self, key: &[u8], snapshot: &StateSnapshot) -> Result<(), String> {
        let path = self.key_to_path(key);
        let data = serde_json::to_vec(snapshot)
            .map_err(|e| format!("Failed to serialize snapshot: {e}"))?;
        fs::write(&path, data).map_err(|e| format!("Failed to write snapshot: {e}"))?;
        Ok(())
    }

    fn load(&self, key: &[u8]) -> Result<Option<StateSnapshot>, String> {
        let path = self.key_to_path(key);
        if !path.exists() {
            return Ok(None);
        }
        let data = fs::read(&path).map_err(|e| format!("Failed to read snapshot: {e}"))?;
        let snapshot: StateSnapshot = serde_json::from_slice(&data)
            .map_err(|e| format!("Failed to deserialize snapshot: {e}"))?;
        Ok(Some(snapshot))
    }

    fn list_keys(&self) -> Result<Vec<Vec<u8>>, String> {
        let mut keys = Vec::new();
        let entries = fs::read_dir(&self.base_dir)
            .map_err(|e| format!("Failed to read directory: {e}"))?;

        for entry in entries {
            let entry = entry.map_err(|e| format!("Failed to read directory entry: {e}"))?;
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) == Some("snapshot") {
                if let Some(file_stem) = path.file_stem().and_then(|s| s.to_str()) {
                    if let Ok(key_bytes) = hex::decode(file_stem) {
                        keys.push(key_bytes);
                    }
                }
            }
        }

        Ok(keys)
    }
}

