//! Event mode types for signal detection operations.
//!
//! These types are defined here and re-exported by tflo-core to avoid
//! circular dependencies.

/// Threshold crossing detection modes.
///
/// Represents the direction of a threshold crossing event when a value
/// transitions from one side of a threshold to the other. This is domain-neutral
/// and applies to any threshold crossing scenario (price levels, signal thresholds,
/// temperature limits, etc.).
///
/// # Use Cases
///
/// - Cross detection: When a value crosses above or below a threshold
/// - Level detection: Monitoring when signals enter/exit amplitude zones
/// - Event generation: Creating domain events from threshold transitions
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Hash)]
pub enum ThresholdCrossEventMode {
    /// Value crossed threshold in positive direction (from below to above).
    ///
    /// The value transitioned from being less than the threshold to being
    /// greater than or equal to the threshold.
    Rising,
    /// Value crossed threshold in negative direction (from above to below).
    ///
    /// The value transitioned from being greater than the threshold to being
    /// less than the threshold.
    Falling,
    /// No threshold crossing occurred.
    ///
    /// The value remained on the same side of the threshold, or this is
    /// the first observation (no previous state to compare).
    #[default]
    None,
}

impl ThresholdCrossEventMode {
    /// Check if this is a rising threshold cross (from below to above).
    #[must_use]
    pub const fn from_below(self) -> bool {
        matches!(self, Self::Rising)
    }

    /// Check if this is a falling threshold cross (from above to below).
    #[must_use]
    pub const fn from_above(self) -> bool {
        matches!(self, Self::Falling)
    }

    /// Check if no threshold crossing occurred.
    #[must_use]
    pub const fn is_none(self) -> bool {
        matches!(self, Self::None)
    }
}
