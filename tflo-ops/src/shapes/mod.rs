//! Generic operator shapes used across the `tflo-ops` catalog.
//!
//! This module provides reusable structural building blocks — generic shapes
//! that concrete operators in `tflo-ops` are built from. [`WindowPrimitive`]
//! is the key abstraction that lets the `Windowed` operator shape treat
//! time-based and count-based windows uniformly.

pub use tflo_core::WindowPrimitive;
