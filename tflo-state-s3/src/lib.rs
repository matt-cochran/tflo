#![cfg_attr(
    test,
    allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::indexing_slicing,
        clippy::arithmetic_side_effects,
        clippy::map_err_ignore
    )
)]
#![deny(clippy::print_stdout)] // library code must not write to stdout
//! S3-compatible object store backend for tflo checkpoints.
//!
//! # Phase 1 design
//!
//! This crate exposes [`S3Client`] (a thin trait that any concrete S3
//! client — `aws-sdk-s3`, `rusoto`, `MinIO` HTTP, etc. — can satisfy) and
//! [`S3StateStore`], which implements
//! [`tflo_core::state::AsyncStateStore`]
//! **directly**. The pre-Phase-1 hack of bridging a sync `StateStore`
//! impl to an async client via `tokio::runtime::Handle::try_current()
//! .block_on(...)` is gone — the boundary is async-native end-to-end.
//!
//! # Example (with a hand-rolled in-memory `S3Client`)
//!
//! ```no_run
//! use std::sync::Mutex;
//! use std::collections::HashMap;
//! use async_trait::async_trait;
//! use tflo_state_s3::{S3Client, S3StateStore};
//! use tflo_core::keyed::{StateSnapshot, SnapshotMetadata};
//! use tflo_core::state::AsyncStateStore;
//!
//! #[derive(Default)]
//! struct MemClient { store: Mutex<HashMap<String, Vec<u8>>> }
//!
//! #[async_trait]
//! impl S3Client for MemClient {
//!     async fn put_object(&self, _b: &str, k: &str, d: &[u8]) -> Result<(), String> {
//!         self.store.lock().unwrap().insert(k.into(), d.to_vec());
//!         Ok(())
//!     }
//!     async fn get_object(&self, _b: &str, k: &str) -> Result<Option<Vec<u8>>, String> {
//!         Ok(self.store.lock().unwrap().get(k).cloned())
//!     }
//!     async fn list_objects(&self, _b: &str, prefix: &str) -> Result<Vec<String>, String> {
//!         Ok(self.store.lock().unwrap().keys()
//!             .filter(|k| k.starts_with(prefix)).cloned().collect())
//!     }
//!     async fn delete_object(&self, _b: &str, k: &str) -> Result<(), String> {
//!         self.store.lock().unwrap().remove(k);
//!         Ok(())
//!     }
//! }
//!
//! # async fn run() -> Result<(), String> {
//! let store = S3StateStore::new(MemClient::default(), "bucket".into(), "ckp/".into());
//! let snap = StateSnapshot { data: vec![1,2,3], metadata: SnapshotMetadata {
//!     key: Some(b"k".to_vec()), timestamp_ms: 0, version: 1, topology_fingerprint: None
//! }};
//! store.save(b"k", &snap).await?;
//! let loaded = store.load(b"k").await?;
//! assert!(loaded.is_some());
//! # Ok(()) }
//! ```

#![warn(missing_docs)]
#![warn(missing_debug_implementations)]
#![deny(unsafe_code)]

use async_trait::async_trait;
use tflo_core::keyed::StateSnapshot;

/// Thin trait over S3-compatible object-store operations.
///
/// Users supply a concrete implementation backed by their preferred S3
/// client (`aws-sdk-s3`, `rusoto`, plain HTTP for MinIO, etc.). Keeping
/// the trait surface minimal makes it cheap to swap clients.
#[async_trait]
pub trait S3Client: Send + Sync {
    /// PUT an object.
    ///
    /// # Errors
    ///
    /// Returns an error string when the underlying client rejects the
    /// request or the network call fails.
    async fn put_object(&self, bucket: &str, key: &str, data: &[u8]) -> Result<(), String>;

    /// GET an object. Returns `Ok(None)` when the key does not exist.
    ///
    /// # Errors
    ///
    /// Returns an error string on any failure other than not-found.
    async fn get_object(&self, bucket: &str, key: &str) -> Result<Option<Vec<u8>>, String>;

    /// LIST objects under a prefix.
    ///
    /// # Errors
    ///
    /// Returns an error string when the listing call fails.
    async fn list_objects(&self, bucket: &str, prefix: &str) -> Result<Vec<String>, String>;

    /// DELETE an object. A client that genuinely cannot delete should return
    /// `Err("delete unsupported".into())` rather than a silent success — the
    /// default impl was removed because forgetful implementers caused S3
    /// objects to accumulate indefinitely.
    ///
    /// # Errors
    ///
    /// Returns an error string when the underlying client fails or delete is
    /// not supported by the client.
    async fn delete_object(&self, bucket: &str, key: &str) -> Result<(), String>;
}

/// `AsyncStateStore` implementation backed by an [`S3Client`].
#[derive(Debug)]
pub struct S3StateStore<C: S3Client> {
    client: C,
    bucket: String,
    prefix: String,
}

impl<C: S3Client> S3StateStore<C> {
    /// Construct an `S3StateStore`.
    ///
    /// `prefix` is prepended to every object key — typical values look
    /// like `"checkpoints/"`. The empty prefix is allowed.
    #[must_use]
    pub const fn new(client: C, bucket: String, prefix: String) -> Self {
        Self {
            client,
            bucket,
            prefix,
        }
    }

    /// The bucket this store writes to.
    #[must_use]
    pub fn bucket(&self) -> &str {
        &self.bucket
    }

    /// The key prefix this store applies.
    #[must_use]
    pub fn prefix(&self) -> &str {
        &self.prefix
    }

    fn object_key(&self, key: &[u8]) -> String {
        let hex_key = hex::encode(key);
        format!("{}{hex_key}.snapshot", self.prefix)
    }

    fn parse_object_key(&self, object_key: &str) -> Option<Vec<u8>> {
        let stem = object_key.strip_prefix(&self.prefix)?;
        let hex_part = stem.strip_suffix(".snapshot")?;
        hex::decode(hex_part).ok()
    }
}

#[cfg(feature = "async")]
#[async_trait]
impl<C: S3Client> tflo_core::state::AsyncStateStore for S3StateStore<C> {
    async fn save(&self, key: &[u8], snapshot: &StateSnapshot) -> Result<(), String> {
        let object_key = self.object_key(key);
        let bytes = serde_json::to_vec(snapshot)
            .map_err(|e| format!("Failed to serialize snapshot: {e}"))?;
        self.client
            .put_object(&self.bucket, &object_key, &bytes)
            .await
    }

    async fn load(&self, key: &[u8]) -> Result<Option<StateSnapshot>, String> {
        let object_key = self.object_key(key);
        match self.client.get_object(&self.bucket, &object_key).await? {
            Some(bytes) => {
                let snapshot: StateSnapshot = serde_json::from_slice(&bytes)
                    .map_err(|e| format!("Failed to deserialize snapshot: {e}"))?;
                Ok(Some(snapshot))
            }
            None => Ok(None),
        }
    }

    async fn list_keys(&self) -> Result<Vec<Vec<u8>>, String> {
        let object_keys = self.client.list_objects(&self.bucket, &self.prefix).await?;
        Ok(object_keys
            .into_iter()
            .filter_map(|ok| self.parse_object_key(&ok))
            .collect())
    }

    async fn delete(&self, key: &[u8]) -> Result<(), String> {
        let object_key = self.object_key(key);
        self.client.delete_object(&self.bucket, &object_key).await
    }
}

#[cfg(all(test, feature = "async"))]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::Mutex;
    use tflo_core::keyed::SnapshotMetadata;
    use tflo_core::state::AsyncStateStore;

    #[derive(Default)]
    struct MemClient {
        store: Mutex<HashMap<String, Vec<u8>>>,
    }

    #[async_trait]
    impl S3Client for MemClient {
        async fn put_object(&self, _b: &str, k: &str, d: &[u8]) -> Result<(), String> {
            self.store
                .lock()
                .map_err(|_| "lock poisoned".to_string())?
                .insert(k.into(), d.to_vec());
            Ok(())
        }
        async fn get_object(&self, _b: &str, k: &str) -> Result<Option<Vec<u8>>, String> {
            Ok(self
                .store
                .lock()
                .map_err(|_| "lock poisoned".to_string())?
                .get(k)
                .cloned())
        }
        async fn list_objects(&self, _b: &str, prefix: &str) -> Result<Vec<String>, String> {
            Ok(self
                .store
                .lock()
                .map_err(|_| "lock poisoned".to_string())?
                .keys()
                .filter(|k| k.starts_with(prefix))
                .cloned()
                .collect())
        }
        async fn delete_object(&self, _b: &str, k: &str) -> Result<(), String> {
            self.store
                .lock()
                .map_err(|_| "lock poisoned".to_string())?
                .remove(k);
            Ok(())
        }
    }

    fn make_snap(payload: u8) -> StateSnapshot {
        StateSnapshot {
            data: vec![payload; 8],
            metadata: SnapshotMetadata {
                key: Some(b"k".to_vec()),
                timestamp_ms: i64::from(payload),
                version: 1,
                topology_fingerprint: Some([payload; 32]),
            },
        }
    }

    #[tokio::test]
    async fn save_load_round_trip() {
        let store = S3StateStore::new(MemClient::default(), "b".into(), "ckp/".into());
        let snap = make_snap(42);
        store.save(b"alpha", &snap).await.expect("save ok");
        let loaded = store
            .load(b"alpha")
            .await
            .expect("load ok")
            .expect("present");
        assert_eq!(loaded.data, snap.data);
        assert_eq!(
            loaded.metadata.topology_fingerprint,
            snap.metadata.topology_fingerprint
        );
    }

    #[tokio::test]
    async fn list_returns_keys() {
        let store = S3StateStore::new(MemClient::default(), "b".into(), "ckp/".into());
        store.save(b"alpha", &make_snap(1)).await.expect("ok");
        store.save(b"beta", &make_snap(2)).await.expect("ok");
        let mut keys = store.list_keys().await.expect("ok");
        keys.sort();
        assert_eq!(keys, vec![b"alpha".to_vec(), b"beta".to_vec()]);
    }

    #[tokio::test]
    async fn delete_removes_key() {
        let store = S3StateStore::new(MemClient::default(), "b".into(), "ckp/".into());
        store.save(b"alpha", &make_snap(1)).await.expect("ok");
        store.delete(b"alpha").await.expect("ok");
        assert!(store.load(b"alpha").await.expect("ok").is_none());
    }

    #[tokio::test]
    async fn missing_key_returns_none_not_error() {
        let store = S3StateStore::new(MemClient::default(), "b".into(), "ckp/".into());
        assert!(store.load(b"nope").await.expect("ok").is_none());
    }
}
