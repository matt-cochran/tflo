//! Closure-based functional graph primitives on `Comp`.
//!
//! These methods let consumers define their own stateless transforms,
//! filters, filter-maps, and stateful scans without modifying `tflo-core`.
//!
//! # Naming
//!
//! All methods accept an optional `.named("...")` for graph-inspection
//! readability.  When omitted a generic label is used.

use super::{Comp, Node, ScanCodec};
use crate::compile::{Absent, Computed};
use std::sync::Arc;

/// Build a [`ScanCodec`] for a concrete, `serde`-able accumulator type `S`.
///
/// `encode` downcasts the type-erased live accumulator back to `S` and
/// `postcard`-encodes it; `decode` `postcard`-decodes bytes into a fresh
/// boxed `S`. Both return `None` on mismatch / malformed input so the
/// snapshot layer surfaces a typed error instead of panicking.
fn scan_codec<S>() -> ScanCodec
where
    S: serde::Serialize + serde::de::DeserializeOwned + Send + Sync + 'static,
{
    ScanCodec {
        encode: Arc::new(|state: &(dyn std::any::Any + Send + Sync)| {
            state
                .downcast_ref::<S>()
                .and_then(|s| postcard::to_stdvec(s).ok())
        }),
        decode: Arc::new(|bytes: &[u8]| {
            postcard::from_bytes::<S>(bytes)
                .ok()
                .map(|s| Box::new(s) as Box<dyn std::any::Any + Send + Sync>)
        }),
    }
}

// ── closure type aliases ───────────────────────────────────────────────────

/// Thread-safe closure alias for internal storage — `f64 → f64`.
type Fn64 = Arc<dyn Fn(f64) -> f64 + Send + Sync>;
/// Thread-safe closure alias — `(f64, f64) → f64`.
type Fn64Bin = Arc<dyn Fn(f64, f64) -> f64 + Send + Sync>;
/// Thread-safe closure alias — `f64 → bool`.
type Fn64Bool = Arc<dyn Fn(f64) -> bool + Send + Sync>;
/// Thread-safe closure alias — `f64 → Option<f64>`.
type Fn64Opt = Arc<dyn Fn(f64) -> Option<f64> + Send + Sync>;

// ═══════════════════════════════════════════════════════════════════════════
// Public API
// ═══════════════════════════════════════════════════════════════════════════

impl<R: 'static> Comp<R, f64> {
    // ── map_f64 ──────────────────────────────────────────────────────────

    /// Apply a stateless unary transform to this computation.
    ///
    /// The closure receives one `f64` value and returns one `f64`.
    ///
    /// # Optional naming
    ///
    /// ```rust,ignore
    /// price.map_f64(|x| x.ln() * 2.0).named("log_scaled_price")
    /// ```
    #[must_use]
    pub fn map_f64<F>(&self, f: F) -> Self
    where
        F: Fn(f64) -> f64 + Send + Sync + 'static,
    {
        let closure: Fn64 = Arc::new(f);
        Self::add_node_to_state(
            &self.state,
            Node::MapF64 {
                input: self.id,
                f: closure,
                name: None,
            },
        )
    }

    // ── map2_f64 ────────────────────────────────────────────────────────

    /// Apply a stateless binary transform from this and another computation.
    ///
    /// The closure receives two `f64` values `(self, other)` and returns one `f64`.
    #[must_use]
    pub fn map2_f64<F>(&self, other: &Self, f: F) -> Self
    where
        F: Fn(f64, f64) -> f64 + Send + Sync + 'static,
    {
        let closure: Fn64Bin = Arc::new(f);
        Self::add_node_to_state(
            &self.state,
            Node::Map2F64 {
                a: self.id,
                b: other.id,
                f: closure,
                name: None,
            },
        )
    }

    // ── filter_f64 ──────────────────────────────────────────────────────

    /// Keep only values where the predicate returns `true`.
    ///
    /// Suppressed values do not appear in the output stream.
    #[must_use]
    pub fn filter_f64<F>(&self, f: F) -> Self
    where
        F: Fn(f64) -> bool + Send + Sync + 'static,
    {
        let predicate: Fn64Bool = Arc::new(f);
        Self::add_node_to_state(
            &self.state,
            Node::FilterF64 {
                input: self.id,
                predicate,
                name: None,
            },
        )
    }

    // ── filter_map_f64 ──────────────────────────────────────────────────

    /// Apply a transform that may suppress the output.
    ///
    /// Returns `Some(value)` to emit, `None` to suppress.
    #[must_use]
    pub fn filter_map_f64<F>(&self, f: F) -> Self
    where
        F: Fn(f64) -> Option<f64> + Send + Sync + 'static,
    {
        let closure: Fn64Opt = Arc::new(f);
        Self::add_node_to_state(
            &self.state,
            Node::FilterMapF64 {
                input: self.id,
                f: closure,
                name: None,
            },
        )
    }

    // ── scan_f64 ────────────────────────────────────────────────────────

    /// Stateful unary scan.
    ///
    /// `init` produces fresh state when the graph is compiled.
    /// `step` receives `(&mut S, f64)` and returns one `f64` per record.
    ///
    /// # Optional naming
    ///
    /// ```rust,ignore
    /// price.scan_f64(|| 0.0, |state, x| { *state = 0.9 * *state + 0.1 * x; *state })
    ///     .named("ema_custom")
    /// ```
    #[must_use]
    pub fn scan_f64<S, Init, Step>(&self, init: Init, step: Step) -> Self
    where
        S: Send + Sync + 'static,
        Init: Fn() -> S + Send + Sync + 'static,
        Step: Fn(&mut S, f64) -> f64 + Send + Sync + 'static,
    {
        let state_factory: Arc<dyn Fn() -> Box<dyn std::any::Any + Send + Sync> + Send + Sync> =
            Arc::new(move || Box::new(init()));
        let step_fn: Arc<
            dyn Fn(&mut Box<dyn std::any::Any + Send + Sync>, f64) -> Computed + Send + Sync,
        > = Arc::new(move |state, x| match state.downcast_mut::<S>() {
            Some(s) => Ok(step(s, x)),
            // The compiler always pairs a `ScanF64` op with a `ScanState` of
            // the matching type; a mismatch can only mean an uninitialised
            // state, so degrade to "warming up" rather than panicking.
            None => Err(Absent::WarmingUp),
        });
        Self::add_node_to_state(
            &self.state,
            Node::ScanF64 {
                input: self.id,
                ctor: state_factory,
                step: step_fn,
                codec: None,
                name: None,
            },
        )
    }

    // ── scan2_f64 ───────────────────────────────────────────────────────

    /// Stateful binary scan.
    ///
    /// `init` produces fresh state when the graph is compiled.
    /// `step` receives `(&mut S, f64, f64)` and returns one `f64` per record.
    #[must_use]
    pub fn scan2_f64<S, Init, Step>(&self, other: &Self, init: Init, step: Step) -> Self
    where
        S: Send + Sync + 'static,
        Init: Fn() -> S + Send + Sync + 'static,
        Step: Fn(&mut S, f64, f64) -> f64 + Send + Sync + 'static,
    {
        let state_factory: Arc<dyn Fn() -> Box<dyn std::any::Any + Send + Sync> + Send + Sync> =
            Arc::new(move || Box::new(init()));
        let step_fn: Arc<
            dyn Fn(&mut Box<dyn std::any::Any + Send + Sync>, f64, f64) -> Computed + Send + Sync,
        > = Arc::new(move |state, a, b| match state.downcast_mut::<S>() {
            Some(s) => Ok(step(s, a, b)),
            None => Err(Absent::WarmingUp),
        });
        Self::add_node_to_state(
            &self.state,
            Node::Scan2F64 {
                a: self.id,
                b: other.id,
                ctor: state_factory,
                step: step_fn,
                codec: None,
                name: None,
            },
        )
    }

    // ── scan_f64_checkpointed ────────────────────────────────────────────

    /// Stateful unary scan whose accumulator can be **checkpointed**.
    ///
    /// Identical in behaviour to [`scan_f64`](Self::scan_f64), but the state
    /// type `S` must be `serde`-serializable. The builder attaches a
    /// [`ScanCodec`](crate::comp::ScanCodec) so that a graph containing this
    /// node can be captured by
    /// [`snapshot`](crate::compile::CompiledGraph::snapshot) and recovered by
    /// [`restore`](crate::compile::CompiledGraph::restore) — enabling
    /// roll-back / state-as-of-T. The plain `scan_f64` remains
    /// non-snapshottable because its state is opaque.
    ///
    /// ```rust,ignore
    /// price.scan_f64_checkpointed(
    ///     || 0.0_f64,
    ///     |s, x| { *s += x; *s },
    /// )
    /// ```
    #[must_use]
    pub fn scan_f64_checkpointed<S, Init, Step>(&self, init: Init, step: Step) -> Self
    where
        S: serde::Serialize + serde::de::DeserializeOwned + Send + Sync + 'static,
        Init: Fn() -> S + Send + Sync + 'static,
        Step: Fn(&mut S, f64) -> f64 + Send + Sync + 'static,
    {
        let state_factory: Arc<dyn Fn() -> Box<dyn std::any::Any + Send + Sync> + Send + Sync> =
            Arc::new(move || Box::new(init()));
        let step_fn: Arc<
            dyn Fn(&mut Box<dyn std::any::Any + Send + Sync>, f64) -> Computed + Send + Sync,
        > = Arc::new(move |state, x| match state.downcast_mut::<S>() {
            Some(s) => Ok(step(s, x)),
            None => Err(Absent::WarmingUp),
        });
        Self::add_node_to_state(
            &self.state,
            Node::ScanF64 {
                input: self.id,
                ctor: state_factory,
                step: step_fn,
                codec: Some(scan_codec::<S>()),
                name: None,
            },
        )
    }

    // ── scan2_f64_checkpointed ───────────────────────────────────────────

    /// Stateful binary scan whose accumulator can be **checkpointed**.
    ///
    /// The checkpointed analogue of [`scan2_f64`](Self::scan2_f64); see
    /// [`scan_f64_checkpointed`](Self::scan_f64_checkpointed).
    #[must_use]
    pub fn scan2_f64_checkpointed<S, Init, Step>(
        &self,
        other: &Self,
        init: Init,
        step: Step,
    ) -> Self
    where
        S: serde::Serialize + serde::de::DeserializeOwned + Send + Sync + 'static,
        Init: Fn() -> S + Send + Sync + 'static,
        Step: Fn(&mut S, f64, f64) -> f64 + Send + Sync + 'static,
    {
        let state_factory: Arc<dyn Fn() -> Box<dyn std::any::Any + Send + Sync> + Send + Sync> =
            Arc::new(move || Box::new(init()));
        let step_fn: Arc<
            dyn Fn(&mut Box<dyn std::any::Any + Send + Sync>, f64, f64) -> Computed + Send + Sync,
        > = Arc::new(move |state, a, b| match state.downcast_mut::<S>() {
            Some(s) => Ok(step(s, a, b)),
            None => Err(Absent::WarmingUp),
        });
        Self::add_node_to_state(
            &self.state,
            Node::Scan2F64 {
                a: self.id,
                b: other.id,
                ctor: state_factory,
                step: step_fn,
                codec: Some(scan_codec::<S>()),
                name: None,
            },
        )
    }

    // ── .named(...) ──────────────────────────────────────────────────────

    /// Attach a human-readable name to this custom functional node for
    /// graph-plan, debug, and diagnostic output.
    ///
    /// The name is **optional metadata only** — it has no effect on
    /// semantics, correctness, or type-checking.
    ///
    /// Calling `.named(...)` on a non-custom built-in node is a no-op.
    #[must_use]
    pub fn named(self, name: &str) -> Self {
        let id = self.id;
        {
            let mut state = self.state.borrow_mut();
            if let Some((
                _,
                Node::MapF64 { name: n, .. }
                | Node::Map2F64 { name: n, .. }
                | Node::FilterF64 { name: n, .. }
                | Node::FilterMapF64 { name: n, .. }
                | Node::ScanF64 { name: n, .. }
                | Node::Scan2F64 { name: n, .. },
            )) = state.nodes.iter_mut().find(|(nid, _)| *nid == id)
            {
                *n = Some(name.to_string());
            }
        }
        self
    }
}

#[cfg(test)]
#[path = "custom_tests.rs"]
mod tests;
