//! `Deduplicator<K>` — idempotent-sink primitive backed by the
//! workspace [`AsyncStateStore`](crate::state::AsyncStateStore).
//!
//! The framework guarantee for sinks is at-least-once: a sink call may
//! fire twice on the same record under retry, restart, or rebalance.
//! Sinks that must be idempotent under this contract use
//! [`Deduplicator`] as a thin wrapper around the write call: each
//! key's most recently seen marker is persisted to the
//! `AsyncStateStore`, and `should_emit(key)` returns `false` for keys
//! already seen within the configured window.
//!
//! Scope and non-goals (see `docs/non-goals.md`):
//! - Exactly-once via two-phase commit is **not** in scope; this
//!   primitive solves the smaller problem of "drop the duplicate".
//! - Cross-process deduplication is the responsibility of the host
//!   `AsyncStateStore` impl (e.g. S3 with a strongly-consistent read).
//!
//! ```ignore
//! use tflo_core::dedup::Deduplicator;
//! use tflo_core::state::AsyncStateStore;
//! use std::sync::Arc;
//!
//! async fn example(store: Arc<dyn AsyncStateStore>) -> Result<(), Box<dyn std::error::Error>> {
//!     let dedup = Deduplicator::<String>::new(store, b"sink-A".to_vec());
//!     let key = "msg-123".to_string();
//!     if dedup.should_emit(&key).await? {
//!         // Write to the sink; mark as seen.
//!         dedup.mark_emitted(&key).await?;
//!     }
//!     Ok(())
//! }
//! ```

#[cfg(feature = "async")]
mod imp {
    use crate::keyed::StateSnapshot;
    use crate::state::AsyncStateStore;
    use std::collections::HashSet;
    use std::hash::Hash;
    use std::marker::PhantomData;
    use std::sync::Arc;
    use std::sync::Mutex;

    /// Idempotent-sink helper. Tracks the set of recently-emitted keys
    /// in a process-local cache and persists them to an
    /// [`AsyncStateStore`] under a per-sink namespace.
    ///
    /// `K` is the key type the sink uses to identify a record (e.g.
    /// `String`, a `(topic, partition, offset)` tuple, a UUID).
    /// Implementations of `K` must be `Hash + Eq + Clone`.
    pub struct Deduplicator<K> {
        store: Arc<dyn AsyncStateStore>,
        namespace: Vec<u8>,
        cache: Mutex<HashSet<K>>,
        _marker: PhantomData<K>,
    }

    impl<K> std::fmt::Debug for Deduplicator<K> {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.debug_struct("Deduplicator")
                .field("namespace_len", &self.namespace.len())
                .finish_non_exhaustive()
        }
    }

    impl<K> Deduplicator<K>
    where
        K: Eq + Hash + Clone + AsRef<[u8]> + Send + Sync,
    {
        /// Construct a deduplicator. `namespace` qualifies the
        /// store-key under which seen markers are persisted, so
        /// multiple sinks can share a single store without colliding.
        #[must_use]
        pub fn new(store: Arc<dyn AsyncStateStore>, namespace: Vec<u8>) -> Self {
            Self {
                store,
                namespace,
                cache: Mutex::new(HashSet::new()),
                _marker: PhantomData,
            }
        }

        /// Returns `true` when this key has not been recorded as
        /// emitted yet. Cache-first; on miss, falls through to the
        /// `AsyncStateStore`.
        ///
        /// # Errors
        ///
        /// Returns an error string when the underlying store call
        /// fails. Treat the error as transient — see
        /// [`ComputeError::kind`](crate::error::ComputeError::kind).
        ///
        /// # Panics
        ///
        /// Panics only if the in-process cache `Mutex` is poisoned —
        /// which means a prior holder of the lock panicked. Lock
        /// poisoning is unrecoverable here because the cache is shared
        /// global state; the process is already in an inconsistent
        /// state and continuing would mask the original panic.
        pub async fn should_emit(&self, key: &K) -> Result<bool, String> {
            // Scope the mutex guard tightly so static analysis can see it
            // released before any `.await`. A poisoned lock indicates a
            // prior panic in the cache — fail loudly rather than mask it.
            let cache_hit = {
                #[allow(clippy::expect_used)]
                let guard = self.cache.lock().expect("cache mutex poisoned");
                guard.contains(key)
            };
            if cache_hit {
                return Ok(false);
            }
            let store_key = self.compose_key(key);
            let existing = self.store.load(&store_key).await?;
            Ok(existing.is_none())
        }

        /// Mark `key` as emitted. Writes a small marker into the
        /// `AsyncStateStore` AND updates the in-process cache so
        /// subsequent `should_emit` calls in this process hit the fast
        /// path without re-querying the store.
        ///
        /// # Errors
        ///
        /// Returns an error string when the store write fails. The
        /// in-process cache is **not** updated on store failure so a
        /// retry observes the same `should_emit == true`.
        ///
        /// # Panics
        ///
        /// Panics only if the in-process cache `Mutex` is poisoned (see
        /// [`should_emit`](Self::should_emit) for the same rationale).
        pub async fn mark_emitted(&self, key: &K) -> Result<(), String> {
            let store_key = self.compose_key(key);
            let snapshot = StateSnapshot {
                data: Vec::new(),
                metadata: crate::keyed::SnapshotMetadata::default(),
            };
            self.store.save(&store_key, &snapshot).await?;
            #[allow(clippy::expect_used)]
            self.cache
                .lock()
                .expect("cache mutex poisoned")
                .insert(key.clone());
            Ok(())
        }

        /// Compose the store-side key as `namespace || ":" || key_bytes`.
        fn compose_key(&self, key: &K) -> Vec<u8> {
            let key_bytes = key.as_ref();
            // The capacity computation is bounded by the sum of two
            // user-controlled slice lengths plus one — both come from
            // `&[u8]` so `usize` overflow is impossible in practice
            // (would require ~16 EB total). Annotating the intent.
            #[allow(clippy::arithmetic_side_effects)]
            let capacity = self.namespace.len() + 1 + key_bytes.len();
            let mut out = Vec::with_capacity(capacity);
            out.extend_from_slice(&self.namespace);
            out.push(b':');
            out.extend_from_slice(key_bytes);
            out
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use async_trait::async_trait;
        use std::collections::HashMap;
        use std::sync::Mutex as StdMutex;

        struct InMemStore {
            inner: StdMutex<HashMap<Vec<u8>, StateSnapshot>>,
        }

        #[async_trait]
        impl AsyncStateStore for InMemStore {
            async fn save(&self, k: &[u8], v: &StateSnapshot) -> Result<(), String> {
                self.inner.lock().unwrap().insert(k.to_vec(), v.clone());
                Ok(())
            }
            async fn load(&self, k: &[u8]) -> Result<Option<StateSnapshot>, String> {
                Ok(self.inner.lock().unwrap().get(k).cloned())
            }
            async fn list_keys(&self) -> Result<Vec<Vec<u8>>, String> {
                Ok(self.inner.lock().unwrap().keys().cloned().collect())
            }
        }

        fn store() -> Arc<dyn AsyncStateStore> {
            Arc::new(InMemStore {
                inner: StdMutex::new(HashMap::new()),
            })
        }

        #[tokio::test]
        async fn dedup_returns_true_on_first_should_emit() {
            let dedup: Deduplicator<String> = Deduplicator::new(store(), b"ns".to_vec());
            let k = "abc".to_string();
            assert!(dedup.should_emit(&k).await.unwrap());
        }

        #[tokio::test]
        async fn dedup_returns_false_after_mark_emitted() {
            let dedup: Deduplicator<String> = Deduplicator::new(store(), b"ns".to_vec());
            let k = "abc".to_string();
            assert!(dedup.should_emit(&k).await.unwrap());
            dedup.mark_emitted(&k).await.unwrap();
            assert!(!dedup.should_emit(&k).await.unwrap());
        }

        #[tokio::test]
        async fn dedup_does_not_collide_across_namespaces() {
            let s = store();
            let a: Deduplicator<String> = Deduplicator::new(s.clone(), b"sink-A".to_vec());
            let b: Deduplicator<String> = Deduplicator::new(s, b"sink-B".to_vec());
            let k = "k".to_string();
            a.mark_emitted(&k).await.unwrap();
            // Same key in a different namespace must NOT see the mark.
            assert!(b.should_emit(&k).await.unwrap());
        }
    }
}

#[cfg(feature = "async")]
pub use imp::Deduplicator;
