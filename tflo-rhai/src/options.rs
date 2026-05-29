//! Engine-construction options for Rhai-backed iterator adapters.
//!
//! `RhaiOptions` carries DoS-mitigation knobs that are forwarded to the
//! underlying `rhai::Engine` when the canonical
//! [`RhaiFilterResult`](crate::filter::RhaiFilterResult) and
//! [`RhaiMapResult`](crate::transform::RhaiMapResult) constructors build
//! one for the caller.
//!
//! The defaults are intentionally conservative: an adversarial script
//! that loops or recurses without bound will be terminated with an
//! evaluation error instead of being allowed to consume unbounded CPU
//! and stack. Callers that want a different budget (or none at all)
//! pass an explicit `RhaiOptions` value to the `*_with_options`
//! constructors. Callers that want to share their own pre-configured
//! `rhai::Engine` keep using `*_with_engine`.

use rhai::Engine;

/// Resource budgets for a Rhai engine.
///
/// Each field is optional — `None` means "leave the Rhai default
/// untouched". The [`Default`] impl sets `max_operations` and
/// `max_call_levels` to safe values so the canonical fallible
/// constructors are not DoS-able by an adversarial script. String and
/// array sizes default to `None` because there is no single safe value
/// across embeddings.
#[derive(Debug, Clone)]
pub struct RhaiOptions {
    /// Maximum number of Rhai operations allowed per evaluation.
    ///
    /// Forwarded to [`Engine::set_max_operations`]. Caps total CPU work
    /// per script run. `None` leaves the Rhai default (unlimited).
    pub max_operations: Option<u64>,

    /// Maximum function-call / recursion depth.
    ///
    /// Forwarded to [`Engine::set_max_call_levels`]. Caps stack growth
    /// from recursive Rhai functions. `None` leaves the Rhai default.
    pub max_call_levels: Option<usize>,

    /// Maximum length (in characters) for any string value the script
    /// may construct.
    ///
    /// Forwarded to [`Engine::set_max_string_size`]. `None` leaves the
    /// Rhai default (unlimited).
    pub max_string_size: Option<usize>,

    /// Maximum number of elements in any array the script may
    /// construct.
    ///
    /// Forwarded to [`Engine::set_max_array_size`]. `None` leaves the
    /// Rhai default (unlimited).
    pub max_array_size: Option<usize>,
}

impl Default for RhaiOptions {
    fn default() -> Self {
        Self {
            max_operations: Some(100_000),
            max_call_levels: Some(32),
            max_string_size: None,
            max_array_size: None,
        }
    }
}

impl RhaiOptions {
    /// Construct an unlimited-budget set of options.
    ///
    /// Equivalent to the Rhai defaults — no caps are applied. Use this
    /// only when the script source is fully trusted (e.g. read from a
    /// signed config or shipped with the binary).
    #[must_use]
    pub const fn unlimited() -> Self {
        Self {
            max_operations: None,
            max_call_levels: None,
            max_string_size: None,
            max_array_size: None,
        }
    }

    /// Build a fresh `rhai::Engine` with these options applied.
    #[must_use]
    pub fn build_engine(&self) -> Engine {
        let mut engine = Engine::new();
        self.apply_to(&mut engine);
        engine
    }

    /// Apply these options to an already-constructed Rhai engine.
    ///
    /// Each `Some` field calls the matching `Engine::set_max_*`
    /// setter; `None` fields are left at whatever the engine currently
    /// has configured. This is useful when callers want to start from
    /// an engine with custom function packages registered.
    pub fn apply_to(&self, engine: &mut Engine) {
        if let Some(n) = self.max_operations {
            let _ = engine.set_max_operations(n);
        }
        if let Some(n) = self.max_call_levels {
            let _ = engine.set_max_call_levels(n);
        }
        if let Some(n) = self.max_string_size {
            let _ = engine.set_max_string_size(n);
        }
        if let Some(n) = self.max_array_size {
            let _ = engine.set_max_array_size(n);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_sets_operation_and_call_caps() {
        let opts = RhaiOptions::default();
        assert_eq!(opts.max_operations, Some(100_000));
        assert_eq!(opts.max_call_levels, Some(32));
        assert_eq!(opts.max_string_size, None);
        assert_eq!(opts.max_array_size, None);
    }

    #[test]
    fn unlimited_clears_all_caps() {
        let opts = RhaiOptions::unlimited();
        assert!(opts.max_operations.is_none());
        assert!(opts.max_call_levels.is_none());
        assert!(opts.max_string_size.is_none());
        assert!(opts.max_array_size.is_none());
    }

    #[test]
    fn build_engine_returns_engine() {
        // We can't introspect the Engine's limits, but we can confirm
        // construction does not panic and the engine is usable.
        let engine = RhaiOptions::default().build_engine();
        let value: i64 = engine.eval("1 + 2").expect("trivial expr evaluates");
        assert_eq!(value, 3);
    }
}
