#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]
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
//!         topology_fingerprint: None,
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
        let entries =
            fs::read_dir(&self.base_dir).map_err(|e| format!("Failed to read directory: {e}"))?;

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

// ── AsyncStateStore impl (Phase 1, gated on `async` feature) ────────
//
// File I/O is naturally synchronous; we run each call on the runtime's
// blocking pool via `tokio::task::spawn_blocking` so the async caller's
// reactor isn't blocked. This is the recommended Tokio pattern for
// blocking I/O and lets `FileStateStore` participate in async pipelines
// (Checkpointer, KafkaShardRouter, etc.) without changing its
// underlying implementation.

#[cfg(feature = "async")]
#[async_trait::async_trait]
impl tflo_core::state::AsyncStateStore for FileStateStore {
    async fn save(&self, key: &[u8], snapshot: &StateSnapshot) -> Result<(), String> {
        let path = self.key_to_path(key);
        let bytes = serde_json::to_vec(snapshot)
            .map_err(|e| format!("Failed to serialize snapshot: {e}"))?;
        tokio::task::spawn_blocking(move || {
            fs::write(&path, bytes).map_err(|e| format!("Failed to write snapshot: {e}"))
        })
        .await
        .map_err(|e| format!("spawn_blocking failed: {e}"))?
    }

    async fn load(&self, key: &[u8]) -> Result<Option<StateSnapshot>, String> {
        let path = self.key_to_path(key);
        tokio::task::spawn_blocking(move || -> Result<Option<StateSnapshot>, String> {
            if !path.exists() {
                return Ok(None);
            }
            let data = fs::read(&path).map_err(|e| format!("Failed to read snapshot: {e}"))?;
            let snapshot: StateSnapshot = serde_json::from_slice(&data)
                .map_err(|e| format!("Failed to deserialize snapshot: {e}"))?;
            Ok(Some(snapshot))
        })
        .await
        .map_err(|e| format!("spawn_blocking failed: {e}"))?
    }

    async fn list_keys(&self) -> Result<Vec<Vec<u8>>, String> {
        let base = self.base_dir.clone();
        tokio::task::spawn_blocking(move || -> Result<Vec<Vec<u8>>, String> {
            let mut keys = Vec::new();
            let entries = fs::read_dir(&base)
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
        })
        .await
        .map_err(|e| format!("spawn_blocking failed: {e}"))?
    }

    async fn delete(&self, key: &[u8]) -> Result<(), String> {
        let path = self.key_to_path(key);
        tokio::task::spawn_blocking(move || -> Result<(), String> {
            if path.exists() {
                fs::remove_file(&path)
                    .map_err(|e| format!("Failed to remove snapshot: {e}"))?;
            }
            Ok(())
        })
        .await
        .map_err(|e| format!("spawn_blocking failed: {e}"))?
    }
}

#[cfg(all(test, feature = "async"))]
mod async_tests {
    use super::*;
    use tflo_core::keyed::SnapshotMetadata;
    use tflo_core::state::AsyncStateStore;

    fn temp_dir() -> std::path::PathBuf {
        let mut d = std::env::temp_dir();
        d.push(format!(
            "tflo-state-files-async-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        d
    }

    #[tokio::test]
    async fn round_trip_save_load_async() {
        let dir = temp_dir();
        let store = FileStateStore::new(&dir).expect("ok");
        let snap = StateSnapshot {
            data: vec![1, 2, 3, 4, 5],
            metadata: SnapshotMetadata {
                key: Some(b"k".to_vec()),
                timestamp_ms: 42,
                version: 1,
                topology_fingerprint: Some([7u8; 32]),
            },
        };
        AsyncStateStore::save(&store, b"k", &snap).await.expect("save ok");
        let loaded = AsyncStateStore::load(&store, b"k").await.expect("load ok");
        assert!(loaded.is_some());
        let loaded = loaded.expect("present");
        assert_eq!(loaded.data, snap.data);
        assert_eq!(
            loaded.metadata.topology_fingerprint,
            snap.metadata.topology_fingerprint
        );
        // Cleanup.
        let _ = std::fs::remove_dir_all(&dir);
    }
}
