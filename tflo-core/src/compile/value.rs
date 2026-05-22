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
pub enum NodeOutput {
    /// An inline [`Computed`] — no heap allocation.
    Computed(Computed),
    /// A boxed value of any other type.
    Other(Box<dyn Any + Send + Sync>),
}

impl NodeOutput {
    /// View the contained value as `&dyn Any` for downcasting.
    ///
    /// Works uniformly for both variants: `Computed` yields `&Computed`,
    /// `Other` yields the boxed value. This lets `ValueStore::get`/`get_cloned`
    /// downcast to any concrete type without special-casing the hot path.
    #[inline]
    #[must_use]
    pub fn as_any(&self) -> &(dyn Any + Send + Sync) {
        match self {
            NodeOutput::Computed(c) => c,
            NodeOutput::Other(b) => b.as_ref(),
        }
    }

    /// View the output as a [`Computed`], or `None` if it is an `Other` typed value.
    #[inline]
    #[must_use]
    pub fn as_computed(&self) -> Option<Computed> {
        match self {
            NodeOutput::Computed(c) => Some(*c),
            NodeOutput::Other(_) => None,
        }
    }

    /// Wrap a computed `f64`-or-absent result.
    #[inline]
    #[must_use]
    pub fn computed(c: Computed) -> Self {
        NodeOutput::Computed(c)
    }

    /// Wrap any other typed value (an event enum, a `map`/`fold` output).
    ///
    /// This is the orphan-rule-safe way for a downstream crate's operator to
    /// emit a non-`f64` output — it cannot `impl From<…> for NodeOutput`.
    #[inline]
    #[must_use]
    pub fn other<T: std::any::Any + Send + Sync>(value: T) -> Self {
        NodeOutput::Other(Box::new(value))
    }
}

impl std::fmt::Debug for NodeOutput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            NodeOutput::Computed(c) => write!(f, "Computed({c:?})"),
            NodeOutput::Other(_) => write!(f, "Other(<dyn Any>)"),
        }
    }
}

impl From<f64> for NodeOutput {
    /// A bare `f64` is always a *present* value — sources (`prop`, `const`)
    /// produce one directly.
    #[inline]
    fn from(v: f64) -> Self {
        NodeOutput::Computed(Ok(v))
    }
}

impl From<Computed> for NodeOutput {
    #[inline]
    fn from(c: Computed) -> Self {
        NodeOutput::Computed(c)
    }
}
