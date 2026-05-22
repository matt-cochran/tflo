//! Generic checkpoint helpers for [`Operator::save`] and [`Operator::load`].
//!
//! Most operators can implement checkpointing as a one-liner by delegating to
//! these helpers, provided they derive [`serde::Serialize`] and
//! [`serde::Deserialize`].
//!
//! ```ignore
//! fn save(&self) -> Option<Vec<u8>> { checkpoint::save(self) }
//! fn load(&mut self, bytes: &[u8]) -> Result<(), OperatorLoadError> { checkpoint::load(self, bytes) }
//! ```

use serde::{Serialize, de::DeserializeOwned};
use tflo_core::operator::OperatorLoadError;

/// Generic [`Operator::save`] body: postcard-encode the whole operator.
#[must_use]
pub fn save<T: Serialize>(op: &T) -> Option<Vec<u8>> {
    postcard::to_stdvec(op).ok()
}

/// Generic [`Operator::load`] body: postcard-decode in place.
///
/// # Errors
/// Returns [`OperatorLoadError`] if the bytes are malformed.
pub fn load<T: DeserializeOwned>(op: &mut T, bytes: &[u8]) -> Result<(), OperatorLoadError> {
    *op =
        postcard::from_bytes(bytes).map_err(|e| OperatorLoadError::new(format!("decode: {e}")))?;
    Ok(())
}
