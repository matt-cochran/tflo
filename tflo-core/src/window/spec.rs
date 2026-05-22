//! Window specification for chainable window syntax.
//!
//! This module provides [`WindowSpec`] which enables fluent window syntax:
//!
//! ```rust
//! use tflo_core::prelude::*;
//!
//! // Instead of: price.sma(20.secs())
//! // You can write: price.over(20_u64.secs()).sma()
//!
//! // Instead of: price.sma(20usize)
//! // You can write: price.over(20.samples()).sma()
//! ```

use super::Window;
use crate::comp::{Comp, Node};
use std::time::Duration;

/// Extension trait for creating window specifications.
pub trait WindowSpecExt {
    /// Create a window specification for time-based windows.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let sma = price.over(20.secs()).sma();
    /// ```
    fn over(self, duration: Duration) -> WindowSpec<Self>
    where
        Self: Sized;

    /// Create a window specification for count-based windows.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let sma = price.over(20.samples()).sma();
    /// ```
    fn over_n(self, n: usize) -> WindowSpec<Self>
    where
        Self: Sized;
}

/// Extension trait for sample-based windows.
pub trait IntoSamples {
    /// Convert to a sample count for count-based windows.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let sma = price.over(20.samples()).sma();
    /// ```
    fn samples(self) -> usize;
}

impl IntoSamples for usize {
    fn samples(self) -> usize {
        self
    }
}

impl IntoSamples for u32 {
    fn samples(self) -> usize {
        self as usize
    }
}

impl IntoSamples for u64 {
    fn samples(self) -> usize {
        self as usize
    }
}

/// Window specification that enables chainable syntax.
///
/// This type holds a window specification and a reference to the computation,
/// allowing fluent method chaining like `price.over(20.secs()).sma()`.
#[derive(Clone)]
pub struct WindowSpec<C> {
    window: Window,
    comp: C,
}

impl<C> WindowSpec<C> {
    /// Create a new window specification.
    pub(crate) fn new(window: Window, comp: C) -> Self {
        Self { window, comp }
    }
}

impl<C: std::fmt::Debug> std::fmt::Debug for WindowSpec<C> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WindowSpec")
            .field("window", &self.window)
            .field("comp", &self.comp)
            .finish()
    }
}

impl<R: 'static> WindowSpecExt for Comp<R, f64> {
    fn over(self, duration: Duration) -> WindowSpec<Self> {
        WindowSpec::new(Window::Time(duration), self)
    }

    fn over_n(self, n: usize) -> WindowSpec<Self> {
        WindowSpec::new(Window::Count(n), self)
    }
}

impl<R: 'static> WindowSpec<Comp<R, f64>> {
    /// Apply simple moving average to the window specification.
    #[must_use]
    pub fn sma(self) -> Comp<R, f64> {
        Comp::<R, f64>::add_node_to_state(&self.comp.state, Node::Sma(self.comp.id, self.window))
    }

    /// Apply exponential moving average to the window specification.
    #[must_use]
    pub fn ema(self) -> Comp<R, f64> {
        Comp::<R, f64>::add_node_to_state(&self.comp.state, Node::Ema(self.comp.id, self.window))
    }

    /// Apply standard deviation to the window specification.
    #[must_use]
    pub fn std(self) -> Comp<R, f64> {
        Comp::<R, f64>::add_node_to_state(&self.comp.state, Node::Std(self.comp.id, self.window))
    }

    /// Apply variance to the window specification.
    #[must_use]
    pub fn variance(self) -> Comp<R, f64> {
        Comp::<R, f64>::add_node_to_state(
            &self.comp.state,
            Node::Variance(self.comp.id, self.window),
        )
    }

    /// Apply maximum to the window specification.
    #[must_use]
    pub fn max(self) -> Comp<R, f64> {
        Comp::<R, f64>::add_node_to_state(&self.comp.state, Node::Max(self.comp.id, self.window))
    }

    /// Apply minimum to the window specification.
    #[must_use]
    pub fn min(self) -> Comp<R, f64> {
        Comp::<R, f64>::add_node_to_state(&self.comp.state, Node::Min(self.comp.id, self.window))
    }

    /// Apply sum to the window specification.
    #[must_use]
    pub fn sum(self) -> Comp<R, f64> {
        Comp::<R, f64>::add_node_to_state(&self.comp.state, Node::Sum(self.comp.id, self.window))
    }

    /// Apply count to the window specification.
    #[must_use]
    pub fn count(self) -> Comp<R, f64> {
        Comp::<R, f64>::add_node_to_state(&self.comp.state, Node::Count(self.comp.id, self.window))
    }

    /// Apply weighted moving average to the window specification.
    #[must_use]
    pub fn wma(self) -> Comp<R, f64> {
        Comp::<R, f64>::add_node_to_state(&self.comp.state, Node::Wma(self.comp.id, self.window))
    }

    /// Apply RSI to the window specification.
    #[must_use]
    pub fn rsi(self) -> Comp<R, f64> {
        Comp::<R, f64>::add_node_to_state(&self.comp.state, Node::Rsi(self.comp.id, self.window))
    }
}

// Allow direct conversion from WindowSpec to Window for backward compatibility
impl<C> From<WindowSpec<C>> for Window {
    fn from(spec: WindowSpec<C>) -> Self {
        spec.window
    }
}
