//! S3-compatible object store backend for tflo checkpoints.
//!
//! This crate provides a `StateStore` implementation that persists
//! state snapshots to S3-compatible object stores (AWS S3, MinIO, etc.).
//!
//! # Example
//!
//! ```rust
//! use tflo_state_s3::S3StateStore;
//! use tflo_core::keyed::{StateSnapshot, SnapshotMetadata, StateStore};
//!
//! // Note: This is a trait-based design - actual S3 client integration
//! // would be provided by an implementation crate (e.g., using aws-sdk-s3)
//! ```

#![warn(missing_docs)]
#![warn(missing_debug_implementations)]
#![deny(unsafe_code)]

use async_trait::async_trait;
use tflo_core::keyed::{StateSnapshot, StateStore};

/// Trait for S3-compatible object store operations.
///
/// This trait abstracts over different S3 client implementations,
/// allowing users to plug in their preferred client (aws-sdk-s3, rusoto, etc.).
#[async_trait]
pub trait S3Client: Send + Sync {
    /// Put an object in the bucket.
    async fn put_object(&self, bucket: &str, key: &str, data: &[u8]) -> Result<(), String>;

    /// Get an object from the bucket.
    async fn get_object(&self, bucket: &str, key: &str) -> Result<Option<Vec<u8>>, String>;

    /// List all objects with a given prefix.
    async fn list_objects(&self, bucket: &str, prefix: &str) -> Result<Vec<String>, String>;
}

/// S3-based state store implementation.
///
/// Stores each key's snapshot as a separate object in an S3 bucket.
/// Object keys are prefixed with the provided prefix and hex-encoded keys.
#[derive(Debug)]
pub struct S3StateStore<C: S3Client> {
    #[allow(dead_code)]
    client: C,
    #[allow(dead_code)]
    bucket: String,
    prefix: String,
}

impl<C: S3Client> S3StateStore<C> {
    /// Create a new S3-based state store.
    ///
    /// # Arguments
    ///
    /// * `client`: S3 client implementation
    /// * `bucket`: S3 bucket name
    /// * `prefix`: Prefix for object keys (e.g., "checkpoints/")
    #[must_use]
    pub fn new(client: C, bucket: String, prefix: String) -> Self {
        Self {
            client,
            bucket,
            prefix,
        }
    }

    fn key_to_object_key(&self, key: &[u8]) -> String {
        let hex_key = hex::encode(key);
        format!("{}{hex_key}.snapshot", self.prefix)
    }
}

// Note: StateStore is synchronous, but S3 operations are async.
// For a production implementation, we'd need to either:
// 1. Make StateStore async (breaking change)
// 2. Use a blocking async runtime (tokio::runtime::Handle::current().block_on)
// 3. Provide a separate async StateStore trait
//
// For now, this is a placeholder showing the structure. A real implementation
// would need to handle the sync/async boundary appropriately.

impl<C: S3Client> StateStore for S3StateStore<C> {
    fn save(&self, key: &[u8], snapshot: &StateSnapshot) -> Result<(), String> {
        let object_key = self.key_to_object_key(key);
        let data = serde_json::to_vec(snapshot)
            .map_err(|e| format!("Failed to serialize snapshot: {e}"))?;

        #[cfg(feature = "async")]
        {
            tokio::runtime::Handle::try_current()
                .map_err(|_| "No tokio runtime available".to_string())?
                .block_on(self.client.put_object(&self.bucket, &object_key, &data))
        }

        #[cfg(not(feature = "async"))]
        {
            let _ = (object_key, data);
            Err("S3StateStore requires async runtime. Enable 'async' feature.".to_string())
        }
    }

    fn load(&self, key: &[u8]) -> Result<Option<StateSnapshot>, String> {
        let object_key = self.key_to_object_key(key);

        #[cfg(feature = "async")]
        {
            let data = tokio::runtime::Handle::try_current()
                .map_err(|_| "No tokio runtime available".to_string())?
                .block_on(self.client.get_object(&self.bucket, &object_key))?;

            match data {
                Some(bytes) => {
                    let snapshot: StateSnapshot = serde_json::from_slice(&bytes)
                        .map_err(|e| format!("Failed to deserialize snapshot: {e}"))?;
                    Ok(Some(snapshot))
                }
                None => Ok(None),
            }
        }

        #[cfg(not(feature = "async"))]
        {
            let _ = object_key;
            Err("S3StateStore requires async runtime. Enable 'async' feature.".to_string())
        }
    }

    fn list_keys(&self) -> Result<Vec<Vec<u8>>, String> {
        let prefix = &self.prefix;

        #[cfg(feature = "async")]
        {
            let object_keys = tokio::runtime::Handle::try_current()
                .map_err(|_| "No tokio runtime available".to_string())?
                .block_on(self.client.list_objects(&self.bucket, prefix))?;

            let mut keys = Vec::new();
            for object_key in object_keys {
                if let Some(stem) = object_key.strip_prefix(prefix) {
                    if let Some(hex_key) = stem.strip_suffix(".snapshot") {
                        if let Ok(key_bytes) = hex::decode(hex_key) {
                            keys.push(key_bytes);
                        }
                    }
                }
            }

            Ok(keys)
        }

        #[cfg(not(feature = "async"))]
        {
            let _ = prefix;
            Err("S3StateStore requires async runtime. Enable 'async' feature.".to_string())
        }
    }
}
