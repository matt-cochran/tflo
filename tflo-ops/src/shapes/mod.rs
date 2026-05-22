//! Generic operator shapes used across the `tflo-ops` catalog.
//!
//! This module provides reusable structural building blocks — generic shapes
//! that concrete operators in `tflo-ops` are built from. [`WindowPrimitive`]
//! is the key abstraction that lets the `Windowed` operator shape treat
//! time-based and count-based windows uniformly.

pub use tflo_core::WindowPrimitive;

mod windowed;
pub use windowed::Windowed;

/// A windowed reduction: collapses a window primitive `W` to a single `f64`.
///
/// Reductions are zero-sized named unit structs (e.g. `Mean`, `Std`) so a
/// [`Windowed<W, R>`](Windowed) stays `Default`-restorable and `Serialize` with the
/// reduction `#[serde(skip)]`-ped. Concrete reductions are added in a later task.
pub trait Reduce<W>: Default + Send + Sync + 'static {
    /// Reduce the current window contents to a scalar.
    fn reduce(&self, window: &W) -> f64;
}
