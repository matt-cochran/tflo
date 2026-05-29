//! `InMemoryCursorStore` — back-compat sync `CursorStore` + async impl
//! behind `async` feature. Extracted from `lib.rs` via structureos `move`.

use crate::KafkaOffset;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tflo_core::adapter::CursorStore;

/// In-memory cursor store. Implements the sync `CursorStore` trait; when
/// the `async` feature is on it also implements
/// [`tflo_core::state::AsyncCursorStore`] over the same backing store so
/// the same instance can serve both APIs in tests / single-process apps.
#[derive(Debug, Clone, Default)]
pub struct InMemoryCursorStore {
    cursors: Arc<Mutex<HashMap<Vec<u8>, KafkaOffset>>>,
}

impl InMemoryCursorStore {
    /// Construct.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

impl CursorStore for InMemoryCursorStore {
    type Cursor = KafkaOffset;

    fn save_cursor(&self, key: &[u8], cursor: &Self::Cursor) -> Result<(), String> {
        let mut guard = self
            .cursors
            .lock()
            .map_err(|e| format!("cursor store mutex poisoned: {e}"))?;
        guard.insert(key.to_vec(), cursor.clone());
        Ok(())
    }

    fn load_cursor(&self, key: &[u8]) -> Result<Option<Self::Cursor>, String> {
        let guard = self
            .cursors
            .lock()
            .map_err(|e| format!("cursor store mutex poisoned: {e}"))?;
        Ok(guard.get(key).cloned())
    }

    fn list_cursor_keys(&self) -> Result<Vec<Vec<u8>>, String> {
        let guard = self
            .cursors
            .lock()
            .map_err(|e| format!("cursor store mutex poisoned: {e}"))?;
        Ok(guard.keys().cloned().collect())
    }
}

#[cfg(feature = "async")]
#[async_trait::async_trait]
impl tflo_core::state::AsyncCursorStore<KafkaOffset> for InMemoryCursorStore {
    async fn save_cursor(&self, key: &[u8], cursor: &KafkaOffset) -> Result<(), String> {
        <Self as CursorStore>::save_cursor(self, key, cursor)
    }
    async fn load_cursor(&self, key: &[u8]) -> Result<Option<KafkaOffset>, String> {
        <Self as CursorStore>::load_cursor(self, key)
    }
}
