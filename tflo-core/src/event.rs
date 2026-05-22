//! Signal system for tflo - extensible and composable with combinators.
//!
//! This module provides the core signal abstraction `Signal<TMode, TPayload>`
//! that enables domain-specific signal types while maintaining composability
//! with stream combinators.

/// Core signal wrapper - composable with combinators.
///
/// `Signal` is a generic signal carrier that pairs a mode (the signal type)
/// with an optional payload (signal-specific data). This design enables:
///
/// - **Domain extensibility**: Create custom signal types in downstream crates
/// - **Combinator compatibility**: Works seamlessly with `group_by_key`, `filter`, `map`
/// - **Type safety**: Mode and payload are type-checked at compile time
///
/// # Examples
///
/// ```rust
/// use tflo_core::event::{Signal, ThresholdCrossEventMode};
///
/// // Simple threshold cross signal (no payload)
/// let cross = Signal::simple(ThresholdCrossEventMode::Rising);
///
/// // Threshold cross signal with metadata payload
/// let cross_with_data = Signal::new(ThresholdCrossEventMode::Rising, 42.0);
///
/// // Domain-specific signal
/// #[derive(Clone, Debug, PartialEq)]
/// enum TradeAction { Buy, Sell }
/// struct TradeMetadata { price: f64, volume: f64 }
///
/// type TradeSignal = Signal<TradeAction, TradeMetadata>;
/// let trade = TradeSignal::new(TradeAction::Buy, TradeMetadata { price: 100.0, volume: 10.0 });
/// ```
#[derive(Clone, Debug, PartialEq)]
pub struct Signal<TMode, TPayload = ()> {
    /// The event mode/type (e.g., Rising, Falling, Entered, etc.)
    pub mode: TMode,
    /// Optional payload data associated with this event
    pub payload: TPayload,
}

impl<TMode, TPayload> Signal<TMode, TPayload> {
    /// Create a new event with mode and payload.
    #[must_use]
    pub fn new(mode: TMode, payload: TPayload) -> Self {
        Self { mode, payload }
    }

    /// Transform the payload while preserving the mode.
    ///
    /// # Example
    ///
    /// ```rust
    /// use tflo_core::event::{Signal, ThresholdCrossEventMode};
    ///
    /// let signal = Signal::new(ThresholdCrossEventMode::Rising, 10.0);
    /// let doubled = signal.map_payload(|x| x * 2.0);
    /// assert_eq!(doubled.payload, 20.0);
    /// ```
    #[must_use]
    pub fn map_payload<U, F>(self, f: F) -> Signal<TMode, U>
    where
        F: FnOnce(TPayload) -> U,
    {
        Signal {
            mode: self.mode,
            payload: f(self.payload),
        }
    }

    /// Replace the payload with a new value.
    #[must_use]
    pub fn with_payload<U>(self, payload: U) -> Signal<TMode, U> {
        Signal {
            mode: self.mode,
            payload,
        }
    }

    /// Extract the mode, consuming the event.
    #[must_use]
    pub fn into_mode(self) -> TMode {
        self.mode
    }

    /// Extract the payload, consuming the event.
    #[must_use]
    pub fn into_payload(self) -> TPayload {
        self.payload
    }

    /// Get a reference to the mode.
    #[must_use]
    pub fn mode(&self) -> &TMode {
        &self.mode
    }

    /// Get a reference to the payload.
    #[must_use]
    pub fn payload(&self) -> &TPayload {
        &self.payload
    }
}

impl<TMode> Signal<TMode, ()> {
    /// Create a simple signal with no payload.
    ///
    /// # Example
    ///
    /// ```rust
    /// use tflo_core::event::{Signal, ThresholdCrossEventMode};
    ///
    /// let cross = Signal::simple(ThresholdCrossEventMode::Rising);
    /// ```
    #[must_use]
    pub fn simple(mode: TMode) -> Self {
        Self { mode, payload: () }
    }
}

/// Marker trait for event modes.
///
/// This trait can be implemented by custom event modes to enable
/// common operations like checking if an event is "active".
pub trait EventMode: Clone + Send + Sync + 'static {
    /// Check if this event mode represents an active/triggered state.
    ///
    /// For example, `ThresholdCross::Rising` and `ThresholdCross::Falling` are active,
    /// while `ThresholdCross::None` is not.
    fn is_active(&self) -> bool;
}

/// Threshold crossing detection modes.
///
/// Represents the direction of a threshold crossing event when a value
/// transitions from one side of a threshold to the other.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Hash)]
pub enum ThresholdCrossEventMode {
    /// Value crossed threshold in positive direction (from below to above).
    Rising,
    /// Value crossed threshold in negative direction (from above to below).
    Falling,
    /// No threshold crossing occurred.
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

impl EventMode for ThresholdCrossEventMode {
    fn is_active(&self) -> bool {
        !matches!(self, ThresholdCrossEventMode::None)
    }
}

/// Built-in zone detection modes.
///
/// Used for window detection operations (when a signal enters/exits amplitude bounds).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Hash)]
pub enum ZoneEventMode {
    /// Signal entered the target zone.
    Entered,
    /// Signal exited below the low threshold.
    ExitedLow,
    /// Signal exited above the high threshold.
    ExitedHigh,
    /// Signal is inside the zone (no transition).
    #[default]
    Inside,
}

impl EventMode for ZoneEventMode {
    fn is_active(&self) -> bool {
        !matches!(self, ZoneEventMode::Inside)
    }
}

impl ZoneEventMode {
    /// Check if this is an entry event.
    #[must_use]
    pub const fn is_entered(self) -> bool {
        matches!(self, Self::Entered)
    }

    /// Check if this is an exit event (either low or high).
    #[must_use]
    pub const fn is_exited(self) -> bool {
        matches!(self, Self::ExitedLow | Self::ExitedHigh)
    }

    /// Check if this is an exit below low threshold.
    #[must_use]
    pub const fn is_exited_low(self) -> bool {
        matches!(self, Self::ExitedLow)
    }

    /// Check if this is an exit above high threshold.
    #[must_use]
    pub const fn is_exited_high(self) -> bool {
        matches!(self, Self::ExitedHigh)
    }
}

/// Built-in pulse validation modes.
///
/// Used for pulse width and glitch detection operations.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PulseEventMode {
    /// Pulse is valid (within duration bounds).
    Valid,
    /// Pulse is too short (glitch).
    TooShort,
    /// Pulse is too long.
    TooLong,
    /// Pulse is a runt (incomplete amplitude transition).
    Runt,
}

impl EventMode for PulseEventMode {
    fn is_active(&self) -> bool {
        matches!(self, PulseEventMode::Valid)
    }
}

impl PulseEventMode {
    /// Check if this is a valid pulse.
    #[must_use]
    pub const fn is_valid(self) -> bool {
        matches!(self, Self::Valid)
    }

    /// Check if this pulse is too short.
    #[must_use]
    pub const fn is_too_short(self) -> bool {
        matches!(self, Self::TooShort)
    }

    /// Check if this pulse is too long.
    #[must_use]
    pub const fn is_too_long(self) -> bool {
        matches!(self, Self::TooLong)
    }

    /// Check if this is a runt pulse.
    #[must_use]
    pub const fn is_runt(self) -> bool {
        matches!(self, Self::Runt)
    }
}

/// Metadata for pulse events.
#[derive(Clone, Debug, PartialEq)]
pub struct PulseMetadata {
    /// Pulse width in milliseconds.
    pub width_ms: i64,
    /// Peak amplitude (if available).
    pub peak: Option<f64>,
}

impl PulseMetadata {
    /// Create new pulse metadata.
    #[must_use]
    pub fn new(width_ms: i64, peak: Option<f64>) -> Self {
        Self { width_ms, peak }
    }

    /// Get the pulse width in milliseconds.
    #[must_use]
    pub const fn width_ms(&self) -> i64 {
        self.width_ms
    }

    /// Get the peak amplitude, if available.
    #[must_use]
    pub const fn peak(&self) -> Option<f64> {
        self.peak
    }
}

// Type aliases for common patterns
/// Threshold crossing signal (no payload).
///
/// A convenience type alias for `Signal<ThresholdCrossEventMode>` representing
/// a threshold crossing signal without additional payload data.
pub type EdgeSignal = Signal<ThresholdCrossEventMode>;

/// Zone signal with optional payload.
pub type ZoneSignal<P = ()> = Signal<ZoneEventMode, P>;

/// Pulse signal with metadata payload.
pub type PulseSignal = Signal<PulseEventMode, PulseMetadata>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_edge_signal_simple() {
        let signal = EdgeSignal::simple(ThresholdCrossEventMode::Rising);
        assert!(signal.mode.from_below());
        assert!(signal.mode.is_active());
    }

    #[test]
    fn test_edge_signal_with_payload() {
        let signal = Signal::new(ThresholdCrossEventMode::Rising, 42.0);
        assert_eq!(signal.payload, 42.0);
        assert!(signal.mode.from_below());
    }

    #[test]
    fn test_map_payload() {
        let signal = Signal::new(ThresholdCrossEventMode::Rising, 10.0);
        let doubled = signal.map_payload(|x| x * 2.0);
        assert_eq!(doubled.payload, 20.0);
        assert_eq!(doubled.mode, ThresholdCrossEventMode::Rising);
    }

    #[test]
    fn test_with_payload() {
        let signal = EdgeSignal::simple(ThresholdCrossEventMode::Rising);
        let with_data = signal.with_payload(100.0);
        assert_eq!(with_data.payload, 100.0);
    }

    #[test]
    fn test_zone_mode() {
        assert!(ZoneEventMode::Entered.is_active());
        assert!(ZoneEventMode::ExitedLow.is_active());
        assert!(!ZoneEventMode::Inside.is_active());
    }

    #[test]
    fn test_pulse_mode() {
        assert!(PulseEventMode::Valid.is_active());
        assert!(!PulseEventMode::TooShort.is_active());
    }

    #[test]
    fn test_pulse_metadata() {
        let meta = PulseMetadata::new(100, Some(3.3));
        assert_eq!(meta.width_ms(), 100);
        assert_eq!(meta.peak(), Some(3.3));
    }
}
