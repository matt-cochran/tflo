//! Generic operator shapes used across the `tflo-ops` catalog.
//!
//! This module provides reusable structural building blocks — generic shapes
//! that concrete operators in `tflo-ops` are built from. [`WindowPrimitive`]
//! is the key abstraction that lets the `Windowed` operator shape treat
//! time-based and count-based windows uniformly.

pub use tflo_core::{BivariateWindow, WindowPrimitive};

mod bivariate;
mod tracker;
mod windowed;
pub use bivariate::BivariateWindowed;
pub use tracker::StatefulTracker;
pub use windowed::Windowed;

use tflo_core::compile::Computed;

/// A windowed reduction: collapses a window primitive `W` to a single `f64`.
///
/// Reductions are zero-sized named unit structs (e.g. `Mean`, `Std`) so a
/// [`Windowed<W, R>`](Windowed) stays `Default`-restorable and `Serialize` with the
/// reduction `#[serde(skip)]`-ped. Concrete reductions are added in a later task.
pub trait Reduce<W>: Default + Send + Sync + 'static {
    /// Reduce the current window contents to a scalar.
    fn reduce(&self, window: &W) -> f64;
}

/// A stateful step: folds one input value (and its timestamp) into mutable
/// tracker state `S`, yielding a [`Computed`] result.
///
/// Steps are zero-sized named unit structs so a `StatefulTracker<S, Step>`
/// stays `Default`-restorable with the step `#[serde(skip)]`-ped.
pub trait TrackStep<S>: Default + Send + Sync + 'static {
    /// Advance the tracker state and produce this record's result.
    fn step(&self, state: &mut S, value: f64, ts: i64) -> Computed;
}
