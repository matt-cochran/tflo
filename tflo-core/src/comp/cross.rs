//! Cross detection and comparison operations on `Comp`.

use super::{Comp, Node};
use crate::event::ThresholdCrossEventMode;

impl<R: 'static> Comp<R, f64> {
    // ========================================================================
    // CROSS DETECTION
    // ========================================================================

    /// Detect when this value crosses the other (either direction).
    ///
    /// Returns `ThresholdCross::Rising` for cross above, `ThresholdCross::Falling` for cross below.
    #[must_use]
    pub fn cross(&self, other: &Comp<R>) -> Comp<R, ThresholdCrossEventMode> {
        Comp::<R, f64>::add_signal_node_to_state(&self.state, Node::Cross(self.id, other.id))
    }

    /// Detect when this value crosses above the other.
    #[must_use]
    pub fn cross_above(&self, other: &Comp<R>) -> Comp<R, ThresholdCrossEventMode> {
        Comp::<R, f64>::add_signal_node_to_state(&self.state, Node::CrossAbove(self.id, other.id))
    }

    /// Detect when this value crosses below the other.
    #[must_use]
    pub fn cross_under(&self, other: &Comp<R>) -> Comp<R, ThresholdCrossEventMode> {
        Comp::<R, f64>::add_signal_node_to_state(&self.state, Node::CrossUnder(self.id, other.id))
    }

    /// Detect when this value crosses a threshold with hysteresis.
    ///
    /// Hysteresis prevents "chatter" by requiring the signal to move beyond
    /// the threshold by a margin before triggering.
    #[must_use]
    pub fn cross_hysteresis(
        &self,
        threshold: &Comp<R>,
        margin: f64,
    ) -> Comp<R, ThresholdCrossEventMode> {
        Comp::<R, f64>::add_signal_node_to_state(
            &self.state,
            Node::CrossHysteresis(self.id, threshold.id, margin),
        )
    }

    // ========================================================================
    // COMPARISONS
    // ========================================================================

    /// Greater than comparison. Returns 1.0 if true, 0.0 if false.
    #[must_use]
    pub fn gt(&self, other: &Comp<R>) -> Comp<R> {
        Self::add_node_to_state(&self.state, Node::Gt(self.id, other.id))
    }

    /// Greater than or equal comparison.
    #[must_use]
    pub fn gte(&self, other: &Comp<R>) -> Comp<R> {
        Self::add_node_to_state(&self.state, Node::Gte(self.id, other.id))
    }

    /// Less than comparison.
    #[must_use]
    pub fn lt(&self, other: &Comp<R>) -> Comp<R> {
        Self::add_node_to_state(&self.state, Node::Lt(self.id, other.id))
    }

    /// Less than or equal comparison.
    #[must_use]
    pub fn lte(&self, other: &Comp<R>) -> Comp<R> {
        Self::add_node_to_state(&self.state, Node::Lte(self.id, other.id))
    }
}
