//! Rego conversion traits (no internal `crate::` deps — keeps the module a leaf).

use serde_json::Value;

/// Trait for types that can be converted to Rego input.
///
/// Types that implement `Serialize` automatically get an implementation via
/// the blanket impl. You can also implement this trait manually for custom
/// JSON structures.
///
/// # Examples
///
/// Using the automatic implementation via `Serialize`:
///
/// ```rust
/// use tflo_rego::traits::IntoRegoInput;
/// use serde::Serialize;
///
/// #[derive(Serialize)]
/// struct Detection {
///     ts: i64,
///     freq_hz: u64,
///     power_dbm: f64,
/// }
///
/// let detection = Detection { ts: 1000, freq_hz: 100_000_000, power_dbm: -70.0 };
/// let input = detection.into_rego_input();
///
/// // The struct is automatically serialized to JSON
/// assert!(input.get("ts").is_some());
/// ```
pub trait IntoRegoInput {
    /// Convert this value into a JSON Value for Rego evaluation.
    fn into_rego_input(&self) -> Value;
}
