//! Typed value storage for the computation engine.

use super::Computed;
use std::any::Any;

/// A computed node output held in the [`ValueStore`](super::ValueStore).
///
/// `Computed` is the hot path: the overwhelming majority of graph nodes
/// produce a [`Computed`] (a finite `f64` or a typed [`Absent`](super::Absent)
/// reason), and storing it inline avoids a heap allocation per node per
/// record. `Other` boxes everything else — the domain event enums produced by
/// signal detectors, and arbitrary types produced by `map`/`fold` composition
/// nodes.
pub(crate) enum Value {
    /// An inline [`Computed`] — no heap allocation.
    Computed(Computed),
    /// A boxed value of any other type.
    Other(Box<dyn Any + Send + Sync>),
}

impl Value {
    /// View the contained value as `&dyn Any` for downcasting.
    ///
    /// Works uniformly for both variants: `Computed` yields `&Computed`,
    /// `Other` yields the boxed value. This lets `ValueStore::get`/`get_cloned`
    /// downcast to any concrete type without special-casing the hot path.
    #[inline]
    pub(crate) fn as_any(&self) -> &(dyn Any + Send + Sync) {
        match self {
            Value::Computed(c) => c,
            Value::Other(b) => b.as_ref(),
        }
    }
}

impl std::fmt::Debug for Value {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Value::Computed(c) => write!(f, "Computed({c:?})"),
            Value::Other(_) => write!(f, "Other(<dyn Any>)"),
        }
    }
}

impl From<f64> for Value {
    /// A bare `f64` is always a *present* value — sources (`prop`, `const`)
    /// produce one directly.
    #[inline]
    fn from(v: f64) -> Self {
        Value::Computed(Ok(v))
    }
}

impl From<Computed> for Value {
    #[inline]
    fn from(c: Computed) -> Self {
        Value::Computed(c)
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
