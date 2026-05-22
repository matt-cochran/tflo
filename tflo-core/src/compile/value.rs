//! Typed value storage for the computation engine.

use std::any::Any;

/// A computed node output held in the [`ValueStore`](super::ValueStore).
///
/// `F64` is the hot path: the overwhelming majority of graph nodes produce an
/// `f64`, and storing it inline avoids a heap allocation per node per record.
/// `Other` boxes everything else — the domain event enums produced by signal
/// detectors, and arbitrary types produced by `map`/`fold` composition nodes.
pub(crate) enum Value {
    /// An inline `f64` — no heap allocation.
    F64(f64),
    /// A boxed value of any other type.
    Other(Box<dyn Any + Send + Sync>),
}

impl Value {
    /// View the contained value as `&dyn Any` for downcasting.
    ///
    /// Works uniformly for both variants: `F64` yields `&f64`, `Other` yields
    /// the boxed value. This lets `ValueStore::get`/`get_cloned` downcast to
    /// any concrete type without special-casing `f64`.
    #[inline]
    pub(crate) fn as_any(&self) -> &(dyn Any + Send + Sync) {
        match self {
            Value::F64(v) => v,
            Value::Other(b) => b.as_ref(),
        }
    }
}

impl std::fmt::Debug for Value {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Value::F64(v) => write!(f, "F64({v})"),
            Value::Other(_) => write!(f, "Other(<dyn Any>)"),
        }
    }
}

impl From<f64> for Value {
    #[inline]
    fn from(v: f64) -> Self {
        Value::F64(v)
    }
}

/// Implements `From<T> for Value` by boxing — for the non-`f64` node outputs.
macro_rules! impl_other_from {
    ($($t:ty),+ $(,)?) => {
        $(
            impl From<$t> for Value {
                #[inline]
                fn from(v: $t) -> Self {
                    Value::Other(Box::new(v))
                }
            }
        )+
    };
}

impl_other_from!(
    crate::event::ThresholdCrossEventMode,
    crate::primitives::GlitchResult,
    Option<crate::primitives::RuntResult>,
    Option<crate::primitives::PulseWidthResult>,
    Option<crate::primitives::WindowEvent>,
);
