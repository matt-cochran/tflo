//! Fluent builders for signal detection primitives.
//!
//! This module provides fluent builder APIs for signal detection:
//!
//! ```rust
//! use tflo_core::prelude::*;
//!
//! // Instead of: price.cross_above(&threshold)
//! // You can write: price.cross().above(&threshold)
//!
//! // Instead of: price.cross_hysteresis(&threshold, margin)
//! // You can write: price.cross().hysteresis(&threshold, margin)
//! ```

use crate::comp::{Comp, Node};
use crate::event::ThresholdCrossEventMode;

/// Fluent builder for cross detection operations.
#[derive(Clone)]
pub struct CrossBuilder<R> {
    comp: Comp<R, f64>,
}

impl<R> std::fmt::Debug for CrossBuilder<R> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CrossBuilder")
            .field("comp", &self.comp)
            .finish()
    }
}

impl<R: 'static> CrossBuilder<R> {
    /// Create a new cross builder.
    pub(crate) fn new(comp: Comp<R, f64>) -> Self {
        Self { comp }
    }

    /// Detect when this value crosses the other (either direction).
    ///
    /// Returns `ThresholdCross::Rising` for cross above, `ThresholdCross::Falling` for cross below.
    #[must_use]
    pub fn with(self, other: &Comp<R>) -> Comp<R, ThresholdCrossEventMode> {
        Comp::<R, f64>::add_signal_node_to_state(
            &self.comp.state,
            Node::Cross(self.comp.id, other.id),
        )
    }

    /// Detect when this value crosses above the other.
    ///
    /// Returns `ThresholdCross::Rising` on cross above, `ThresholdCross::None` otherwise.
    #[must_use]
    pub fn above(self, other: &Comp<R>) -> Comp<R, ThresholdCrossEventMode> {
        Comp::<R, f64>::add_signal_node_to_state(
            &self.comp.state,
            Node::CrossAbove(self.comp.id, other.id),
        )
    }

    /// Detect when this value crosses below the other.
    ///
    /// Returns `ThresholdCross::Falling` on cross below, `ThresholdCross::None` otherwise.
    #[must_use]
    pub fn below(self, other: &Comp<R>) -> Comp<R, ThresholdCrossEventMode> {
        Comp::<R, f64>::add_signal_node_to_state(
            &self.comp.state,
            Node::CrossUnder(self.comp.id, other.id),
        )
    }

    /// Detect when this value crosses a threshold with hysteresis.
    ///
    /// Hysteresis prevents "chatter" by requiring the signal to move beyond
    /// the threshold by a margin before triggering, and then move back past
    /// the threshold minus the margin before resetting.
    ///
    /// Returns `ThresholdCross::Rising` on cross above, `ThresholdCross::Falling` on cross below.
    ///
    /// # Arguments
    ///
    /// * `threshold` - The threshold value (as a `Comp`)
    /// * `margin` - The hysteresis margin (dead band size)
    ///
    /// # Example
    ///
    /// ```ignore
    /// // Trigger when price crosses 100 with ±2 margin
    /// let edge = price.cross().hysteresis(&threshold, 2.0);
    /// // Won't trigger again until price drops below 98 (100 - 2)
    /// ```
    #[must_use]
    pub fn hysteresis(self, threshold: &Comp<R>, margin: f64) -> Comp<R, ThresholdCrossEventMode> {
        Comp::<R, f64>::add_signal_node_to_state(
            &self.comp.state,
            Node::CrossHysteresis(self.comp.id, threshold.id, margin),
        )
    }
}

/// Extension trait for creating cross detection builders.
///
/// This trait provides a fluent API for cross detection. Note that `Comp<R, f64>`
/// already has a `cross(&self, other: &Comp<R>)` method, so this trait provides
/// an alternative fluent API: `price.cross_builder().above(&threshold)`.
pub trait CrossBuilderExt {
    /// The record type.
    type Record;

    /// Create a cross detection builder.
    ///
    /// # Example
    ///
    /// ```ignore
    /// use tflo_core::CrossBuilderExt;
    /// let signal = price.cross_builder().above(&threshold);
    /// let signal = price.cross_builder().hysteresis(&threshold, 2.0);
    /// ```
    fn cross_builder(self) -> CrossBuilder<Self::Record>
    where
        Self: Sized;
}

impl<R: 'static> CrossBuilderExt for Comp<R, f64> {
    type Record = R;

    fn cross_builder(self) -> CrossBuilder<R> {
        CrossBuilder::new(self)
    }
}
