#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing, clippy::arithmetic_side_effects, clippy::let_underscore_must_use))]
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
use std::io::Write;
use std::path::{Path, PathBuf};
use tflo_core::keyed::{StateSnapshot, StateStore};

/// Atomically write `data` to `path` by writing to a unique sibling temp
/// file, fsync'ing, then renaming over the destination.
///
/// The temp file MUST live in the same directory as the destination so
/// that `fs::rename` is an atomic rename (POSIX guarantees this only for
/// same-filesystem renames; sibling paths satisfy this).
///
/// On any failure (file create, write, fsync, rename) the partial temp
/// file is best-effort removed so we don't litter the state directory
/// with orphans. A leftover orphan is still harmless — `list_keys` only
/// surfaces files whose name decodes as hex with a `.snapshot`
/// extension, and the temp filename intentionally violates both rules.
fn atomic_write_helper(path: &Path, data: &[u8]) -> Result<(), String> {
    let parent = match path.parent() {
        Some(p) => p,
        None => return Err(format!("Path has no parent directory: {}", path.display())),
    };

    // Unique temp name in the same directory. PID + nanoseconds gives a
    // collision-resistant name across concurrent processes/threads
    // without needing a CSPRNG dependency.
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let tmp_name = format!(".tflo-state-tmp.{}.{}", std::process::id(), nanos);
    let tmp = parent.join(tmp_name);

    // Create + write + fsync. Any failure removes the temp.
    let mut file = match fs::File::create(&tmp) {
        Ok(f) => f,
        Err(e) => {
            return Err(format!(
                "Failed to create temp file {}: {e}",
                tmp.display()
            ));
        }
    };
    if let Err(e) = file.write_all(data) {
        drop(file);
        drop(fs::remove_file(&tmp));
        return Err(format!("Failed to write temp file {}: {e}", tmp.display()));
    }
    if let Err(e) = file.sync_all() {
        drop(file);
        drop(fs::remove_file(&tmp));
        return Err(format!("Failed to fsync temp file {}: {e}", tmp.display()));
    }
    drop(file);

    // Atomic rename on POSIX. On Windows this is best-effort; the
    // sibling path strategy at least keeps the operation cross-volume
    // safe.
    if let Err(e) = fs::rename(&tmp, path) {
        drop(fs::remove_file(&tmp));
        return Err(format!(
            "Failed to rename {} -> {}: {e}",
            tmp.display(),
            path.display()
        ));
    }
    Ok(())
}

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
        atomic_write_helper(&path, &data)
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
        tokio::task::spawn_blocking(move || atomic_write_helper(&path, &bytes))
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

#[cfg(test)]
mod atomic_tests {
    use super::*;
    use tflo_core::keyed::SnapshotMetadata;

    fn sample_snapshot() -> StateSnapshot {
        StateSnapshot {
            data: vec![9, 8, 7, 6, 5],
            metadata: SnapshotMetadata {
                key: Some(b"alpha".to_vec()),
                timestamp_ms: 1234,
                version: 1,
                topology_fingerprint: Some([3u8; 32]),
            },
        }
    }

    #[test]
    fn save_then_load_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let store = FileStateStore::new(dir.path()).unwrap();
        let snap = sample_snapshot();
        store.save(b"alpha", &snap).unwrap();
        let loaded = store.load(b"alpha").unwrap().unwrap();
        assert_eq!(loaded.data, snap.data);
        assert_eq!(loaded.metadata.timestamp_ms, snap.metadata.timestamp_ms);
        assert_eq!(loaded.metadata.version, snap.metadata.version);
        assert_eq!(
            loaded.metadata.topology_fingerprint,
            snap.metadata.topology_fingerprint
        );
    }

    #[test]
    fn save_recovers_from_orphan_temp_file() {
        let dir = tempfile::tempdir().unwrap();
        let store = FileStateStore::new(dir.path()).unwrap();

        // Pre-plant an orphan temp file in the store directory, as if a
        // prior process crashed mid-write.
        let orphan = dir.path().join(".tflo-state-tmp.99999.0");
        std::fs::write(&orphan, b"garbage-from-prior-run").unwrap();
        assert!(orphan.exists(), "orphan precondition");

        // Save should succeed regardless of the leftover orphan: the
        // helper writes to its own unique temp name keyed by PID+nanos.
        let snap = sample_snapshot();
        store.save(b"alpha", &snap).unwrap();

        // The orphan is NOT cleaned up by save (that's recovery-tool
        // territory), but it MUST NOT appear in list_keys — the
        // listing filters by `.snapshot` extension and hex-decodable
        // stem, both of which the orphan name violates.
        let keys = store.list_keys().unwrap();
        assert_eq!(keys.len(), 1, "only the real key should be listed");
        assert_eq!(keys[0], b"alpha".to_vec());

        // And the real snapshot is round-trippable.
        let loaded = store.load(b"alpha").unwrap().unwrap();
        assert_eq!(loaded.data, snap.data);
    }

    #[test]
    fn temp_file_cleaned_on_create_failure() {
        // Point the store at a directory, then call the helper directly
        // with a destination whose parent does not exist. This exercises
        // the File::create failure branch without depending on chmod
        // semantics that differ between CI environments.
        let dir = tempfile::tempdir().unwrap();
        let bogus = dir.path().join("does-not-exist-subdir").join("x.snapshot");
        let err = atomic_write_helper(&bogus, b"payload").unwrap_err();
        assert!(
            err.contains("Failed to create temp file"),
            "expected create-temp error, got: {err}"
        );

        // The directory we DO have access to must contain no temp
        // orphans (the failed create couldn't have left one because the
        // parent directory itself doesn't exist).
        let entries: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert!(
            entries.is_empty(),
            "no orphan temp files should remain, got: {entries:?}"
        );
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
