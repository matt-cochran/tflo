//! Rhai conversion traits (no internal `crate::` deps — keeps the module a leaf).

use rhai::{Dynamic, Scope};

/// Trait for types that can be converted to a Rhai scope.
///
/// Implement this trait for your domain types to enable Rhai scripting.
///
/// # Examples
///
/// ```rust
/// use tflo_rhai::traits::IntoRhaiScope;
/// use rhai::Scope;
///
/// struct Detection {
///     ts: i64,
///     freq_hz: u64,
///     power_dbm: f64,
///     snr_db: f64,
/// }
///
/// impl IntoRhaiScope for Detection {
///     fn into_rhai_scope(&self) -> Scope<'static> {
///         let mut scope = Scope::new();
///         scope.push("ts", self.ts);
///         scope.push("freq_hz", self.freq_hz as i64);
///         scope.push("freq_mhz", self.freq_hz as f64 / 1e6);
///         scope.push("power", self.power_dbm);
///         scope.push("snr", self.snr_db);
///         scope
///     }
/// }
/// ```
pub trait IntoRhaiScope {
    /// Convert this value into a Rhai scope for evaluation.
    fn into_rhai_scope(&self) -> Scope<'static>;
}

/// Trait for types that can be converted to a Rhai Dynamic value.
pub trait IntoRhaiDynamic {
    /// Convert this value into a Rhai Dynamic.
    fn into_rhai_dynamic(&self) -> Dynamic;
}
