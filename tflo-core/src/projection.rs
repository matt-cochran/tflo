//! Projection read-model — the queryable "current state" half of the
//! derived-events temporal stack.
//!
//! A projection is a keyed fold over the (raw + derived) event stream: status is
//! *derived* from history, not stored as a mutable flag. [`ProjectionStore`] is
//! the materialized view of that fold — the addressable "what is the state of
//! entity `K` right now?" that humans, dashboards, and read APIs need.
//!
//! **Pluggable backend, in-memory default.** The hot path is the sync
//! [`InMemoryProjectionStore`]. Durability is not a parallel mechanism: a store
//! [`snapshot`](ProjectionStore::snapshot)s to bytes and is checkpointed through
//! the existing [`AsyncStateStore`](crate::state::AsyncStateStore) /
//! [`Checkpointer`](crate::state::Checkpointer) — so file/S3 persistence comes
//! for free via `tflo-state-files` / `tflo-state-s3`, with no new storage code.

use std::collections::HashMap;
use std::hash::Hash;

use serde::{de::DeserializeOwned, Serialize};

/// A materialized keyed read-model: the current derived state per entity.
///
/// Implementors are the *cache* of a deterministic fold over history — so a lost
/// store is always rebuildable by replaying events through the reducer. Keep
/// implementations side-effect-free on the hot path; persist via `snapshot` +
/// the existing checkpoint machinery.
pub trait ProjectionStore<K, S> {
    /// Current state for `key`, if any has been folded yet.
    fn get(&self, key: &K) -> Option<S>;
    /// Insert or overwrite the state for `key` (last-writer-wins — the caller
    /// orders by event-time `ts` so corrections supersede correctly).
    fn put(&mut self, key: K, value: S);
    /// Remove and return the state for `key`.
    fn remove(&mut self, key: &K) -> Option<S>;
    /// All `(key, state)` pairs — for queries / dashboards / "list everything".
    fn entries(&self) -> Vec<(K, S)>;
    /// Number of keyed states held.
    fn len(&self) -> usize;
    /// Whether the store holds no state.
    fn is_empty(&self) -> bool {
        self.len() == 0
    }
    /// Serialize the whole store to bytes for checkpointing through an
    /// [`AsyncStateStore`](crate::state::AsyncStateStore). The default impl
    /// requires `entries()` to round-trip; backends with their own durability
    /// may override.
    ///
    /// # Errors
    /// Returns a message if the entries cannot be serialized.
    fn snapshot(&self) -> Result<Vec<u8>, String>
    where
        K: Serialize,
        S: Serialize,
    {
        serde_json::to_vec(&self.entries()).map_err(|e| e.to_string())
    }
}

/// The default in-memory projection store: a `HashMap`. Zero infra — runs in a
/// browser or a unit test with nothing attached; swap to a checkpointed backend
/// for production durability without changing the reducer.
#[derive(Debug, Clone)]
pub struct InMemoryProjectionStore<K, S> {
    map: HashMap<K, S>,
}

impl<K, S> Default for InMemoryProjectionStore<K, S> {
    fn default() -> Self {
        Self { map: HashMap::new() }
    }
}

impl<K, S> InMemoryProjectionStore<K, S>
where
    K: Clone + Eq + Hash,
    S: Clone,
{
    /// A fresh, empty store.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Rebuild a store from a [`snapshot`](ProjectionStore::snapshot)'s bytes —
    /// the restore side of checkpointing through an `AsyncStateStore`.
    ///
    /// # Errors
    /// Returns a message if the bytes are not a valid serialized entry list.
    pub fn restore(bytes: &[u8]) -> Result<Self, String>
    where
        K: DeserializeOwned,
        S: DeserializeOwned,
    {
        let entries: Vec<(K, S)> = serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
        Ok(Self {
            map: entries.into_iter().collect(),
        })
    }
}

impl<K, S> ProjectionStore<K, S> for InMemoryProjectionStore<K, S>
where
    K: Clone + Eq + Hash,
    S: Clone,
{
    fn get(&self, key: &K) -> Option<S> {
        self.map.get(key).cloned()
    }
    fn put(&mut self, key: K, value: S) {
        self.map.insert(key, value);
    }
    fn remove(&mut self, key: &K) -> Option<S> {
        self.map.remove(key)
    }
    fn entries(&self) -> Vec<(K, S)> {
        self.map
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect()
    }
    fn len(&self) -> usize {
        self.map.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn in_memory_put_get_remove() {
        let mut s = InMemoryProjectionStore::<String, i64>::new();
        assert!(s.is_empty());
        s.put("a".into(), 1);
        s.put("b".into(), 2);
        assert_eq!(s.get(&"a".into()), Some(1));
        assert_eq!(s.len(), 2);
        // last-writer-wins
        s.put("a".into(), 9);
        assert_eq!(s.get(&"a".into()), Some(9));
        assert_eq!(s.remove(&"b".into()), Some(2));
        assert_eq!(s.len(), 1);
    }

    #[test]
    fn snapshot_restore_round_trips_through_bytes() {
        // The durability seam: a store serializes to bytes (for an
        // AsyncStateStore checkpoint) and restores identically.
        let mut s = InMemoryProjectionStore::<String, i64>::new();
        s.put("onboarding:u1".into(), 3);
        s.put("onboarding:u2".into(), 7);
        let bytes = s.snapshot().expect("snapshot");
        let restored = InMemoryProjectionStore::<String, i64>::restore(&bytes).expect("restore");
        assert_eq!(restored.get(&"onboarding:u1".into()), Some(3));
        assert_eq!(restored.get(&"onboarding:u2".into()), Some(7));
        assert_eq!(restored.len(), 2);
    }
}
