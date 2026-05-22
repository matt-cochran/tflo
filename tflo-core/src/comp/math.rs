//! Stateless unary math operations on `Comp`.

use super::{Comp, Node};

impl<R: 'static> Comp<R, f64> {
    /// Absolute value.
    #[must_use]
    pub fn abs(&self) -> Comp<R> {
        Self::add_node_to_state(&self.state, Node::Abs(self.id))
    }

    /// Square root.
    #[must_use]
    pub fn sqrt(&self) -> Comp<R> {
        Self::add_node_to_state(&self.state, Node::Sqrt(self.id))
    }

    /// Natural logarithm.
    #[must_use]
    pub fn ln(&self) -> Comp<R> {
        Self::add_node_to_state(&self.state, Node::Ln(self.id))
    }

    /// Power function: x^n.
    #[must_use]
    pub fn pow(&self, n: f64) -> Comp<R> {
        Self::add_node_to_state(&self.state, Node::Pow(self.id, n))
    }

    /// Exponential: e^x.
    #[must_use]
    pub fn exp(&self) -> Comp<R> {
        Self::add_node_to_state(&self.state, Node::Exp(self.id))
    }

    /// Base-10 logarithm.
    #[must_use]
    pub fn log10(&self) -> Comp<R> {
        Self::add_node_to_state(&self.state, Node::Log10(self.id))
    }

    /// Base-2 logarithm.
    #[must_use]
    pub fn log2(&self) -> Comp<R> {
        Self::add_node_to_state(&self.state, Node::Log2(self.id))
    }

    /// Clamp value to the range [min, max].
    #[must_use]
    pub fn clamp(&self, min: f64, max: f64) -> Comp<R> {
        Self::add_node_to_state(&self.state, Node::Clamp(self.id, min, max))
    }

    /// Floor (round down to nearest integer).
    #[must_use]
    pub fn floor(&self) -> Comp<R> {
        Self::add_node_to_state(&self.state, Node::Floor(self.id))
    }

    /// Ceiling (round up to nearest integer).
    #[must_use]
    pub fn ceil(&self) -> Comp<R> {
        Self::add_node_to_state(&self.state, Node::Ceil(self.id))
    }

    /// Round to nearest integer.
    #[must_use]
    pub fn round(&self) -> Comp<R> {
        Self::add_node_to_state(&self.state, Node::Round(self.id))
    }
}
