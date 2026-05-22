//! Stateful operations on `Comp`: lookback, cumulative, returns, rate-based.

use super::{Comp, Node};
use std::sync::Arc;
use std::time::Duration;

impl<R: 'static> Comp<R, f64> {
    // ========================================================================
    // LOOKBACK
    // ========================================================================

    /// Get the previous value.
    #[must_use]
    pub fn prev(&self) -> Comp<R> {
        Self::add_node_to_state(&self.state, Node::Prev(self.id))
    }

    /// Get the previous value for records with the same key.
    #[must_use]
    pub fn prev_by<F, K>(&self, f: F) -> Comp<R>
    where
        F: Fn(&R) -> K + Send + Sync + 'static,
        K: std::hash::Hash + 'static,
    {
        use std::hash::Hasher;
        let h = Arc::new(move |r: &R| {
            let mut hasher = std::collections::hash_map::DefaultHasher::new();
            std::hash::Hash::hash(&f(r), &mut hasher);
            hasher.finish()
        });
        Self::add_node_to_state(&self.state, Node::PrevBy(self.id, h))
    }

    /// Get the value from a specified duration ago.
    #[must_use]
    pub fn lag(&self, duration: Duration) -> Comp<R> {
        Self::add_node_to_state(&self.state, Node::Lag(self.id, duration))
    }

    /// Get the difference between current value and value from a specified duration ago.
    #[must_use]
    pub fn delta(&self, duration: Duration) -> Comp<R> {
        Self::add_node_to_state(&self.state, Node::Delta(self.id, duration))
    }

    // ========================================================================
    // CUMULATIVE (EXPANDING)
    // ========================================================================

    /// Cumulative sum since the start of the stream.
    #[must_use]
    pub fn cumsum(&self) -> Comp<R> {
        Self::add_node_to_state(&self.state, Node::CumSum(self.id))
    }

    /// Cumulative maximum (high-water mark) since the start.
    #[must_use]
    pub fn cummax(&self) -> Comp<R> {
        Self::add_node_to_state(&self.state, Node::CumMax(self.id))
    }

    /// Cumulative minimum since the start.
    #[must_use]
    pub fn cummin(&self) -> Comp<R> {
        Self::add_node_to_state(&self.state, Node::CumMin(self.id))
    }

    /// Cumulative product since the start.
    #[must_use]
    pub fn cumprod(&self) -> Comp<R> {
        Self::add_node_to_state(&self.state, Node::CumProd(self.id))
    }

    // ========================================================================
    // RETURNS
    // ========================================================================

    /// Percentage change from the previous value.
    ///
    /// Computes `(current - prev) / prev * 100`.
    #[must_use]
    pub fn pct_change(&self) -> Comp<R> {
        Self::add_node_to_state(&self.state, Node::PctChange(self.id))
    }

    /// Log return from the previous value.
    ///
    /// Computes `ln(current / prev)`.
    #[must_use]
    pub fn log_return(&self) -> Comp<R> {
        Self::add_node_to_state(&self.state, Node::LogReturn(self.id))
    }

    // ========================================================================
    // RATE-BASED
    // ========================================================================

    /// Rate of change per unit time.
    #[must_use]
    pub fn rate(&self, window: Duration) -> Comp<R> {
        Self::add_node_to_state(&self.state, Node::Rate(self.id, window))
    }

    /// First derivative (velocity).
    #[must_use]
    pub fn velocity(&self, window: Duration) -> Comp<R> {
        Self::add_node_to_state(&self.state, Node::Velocity(self.id, window))
    }

    /// Second derivative (acceleration).
    #[must_use]
    pub fn acceleration(&self, window: Duration) -> Comp<R> {
        Self::add_node_to_state(&self.state, Node::Acceleration(self.id, window))
    }
}
