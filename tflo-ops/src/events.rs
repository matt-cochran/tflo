//! Typed event outputs produced by the event-detector operators.
//!
//! The event-detector operators ([`ops::detectors`](crate::ops::detectors))
//! produce *typed* (non-`f64`) outputs: a direction enum, a pulse
//! classification, a window-transition enum. The types live here so a
//! detector method can return `Comp<R, ThresholdCrossEventMode>` (etc.) and
//! `.collect()` can pull the typed value back out.
//!
//! These types are modelled after the legacy `tflo-core` event/result enums
//! (`tflo_core::primitives::{event_mode, results}`) and carry the same derives
//! and core helper methods. Each `impl`s [`tflo_core::compile::ExtractOutput`]
//! — a foreign trait for a now-`tflo-ops`-local type, allowed by the orphan
//! rule. The `ExtractOutput` impls mirror `tflo-core`'s exactly: a plain
//! `get_cloned` from the first node id, the same pattern the
//! `impl_extract_output!` macro emits.
//!
//! **Intentional omissions:** the `tflo-core` originals carry deprecated
//! `to_edge_mode()` and `to_signal()` aliases. Those aliases are intentionally
//! not provided here — this crate starts clean. Use `to_threshold_cross()`
//! instead (available on every result/event type in this module).
//!
//! Phase 4 of the `tflo-ops` split deletes the `tflo-core` originals; until
//! then both definitions coexist (they are distinct types — a `tflo-ops`
//! detector op stores and extracts the `tflo-ops` copy).

use tflo_core::comp::NodeId;
use tflo_core::compile::{ExtractOutput, ValueStore};

// ============================================================================
// ThresholdCrossEventMode
// ============================================================================

/// Threshold crossing detection modes.
///
/// Represents the direction of a threshold crossing event when a value
/// transitions from one side of a threshold to the other. This is
/// domain-neutral and applies to any threshold crossing scenario (price
/// levels, signal thresholds, temperature limits, etc.).
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

impl ExtractOutput for ThresholdCrossEventMode {
    fn extract(store: &ValueStore, ids: &[NodeId]) -> Option<Self> {
        store.get_cloned(ids.first()?)
    }
}

// ============================================================================
// GlitchResult
// ============================================================================

/// Result of a glitch filter evaluation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GlitchResult {
    /// A valid pulse ended (duration ≥ minimum).
    ValidPulse,
    /// A glitch was detected and rejected (duration < minimum).
    Rejected,
    /// No pulse transition occurred (still in pulse or between pulses).
    NoTransition,
}

impl GlitchResult {
    /// Returns `true` if this is a valid pulse.
    #[must_use]
    pub const fn is_valid_pulse(&self) -> bool {
        matches!(self, Self::ValidPulse)
    }

    /// Returns `true` if this is a rejected glitch.
    #[must_use]
    pub const fn is_rejected(&self) -> bool {
        matches!(self, Self::Rejected)
    }

    /// Returns `true` if no transition occurred.
    #[must_use]
    pub const fn is_no_transition(&self) -> bool {
        matches!(self, Self::NoTransition)
    }

    /// Convert to threshold crossing mode.
    ///
    /// - `ValidPulse` → `ThresholdCrossEventMode::Rising`
    /// - `Rejected` → `ThresholdCrossEventMode::Falling`
    /// - `NoTransition` → `ThresholdCrossEventMode::None`
    #[must_use]
    pub const fn to_threshold_cross(&self) -> ThresholdCrossEventMode {
        match self {
            Self::ValidPulse => ThresholdCrossEventMode::Rising,
            Self::Rejected => ThresholdCrossEventMode::Falling,
            Self::NoTransition => ThresholdCrossEventMode::None,
        }
    }
}

impl ExtractOutput for GlitchResult {
    fn extract(store: &ValueStore, ids: &[NodeId]) -> Option<Self> {
        store.get_cloned(ids.first()?)
    }
}

// ============================================================================
// RuntResult
// ============================================================================

/// Result of a runt detection.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum RuntResult {
    /// A runt pulse was detected (crossed low but not high).
    Runt {
        /// The peak value reached during the runt pulse.
        peak: f64,
    },
    /// A valid pulse was detected (crossed both thresholds).
    ValidPulse {
        /// The peak value reached during the pulse.
        peak: f64,
    },
}

impl RuntResult {
    /// Returns `true` if this is a valid pulse.
    #[must_use]
    pub const fn is_valid(&self) -> bool {
        matches!(self, Self::ValidPulse { .. })
    }

    /// Returns `true` if this is a runt pulse.
    #[must_use]
    pub const fn is_runt(&self) -> bool {
        matches!(self, Self::Runt { .. })
    }

    /// Get the peak value regardless of result type.
    #[must_use]
    pub const fn peak(&self) -> f64 {
        match self {
            Self::Runt { peak } | Self::ValidPulse { peak } => *peak,
        }
    }

    /// Convert to threshold crossing mode.
    ///
    /// - `ValidPulse` → `ThresholdCrossEventMode::Rising`
    /// - `Runt` → `ThresholdCrossEventMode::Falling`
    #[must_use]
    pub const fn to_threshold_cross(&self) -> ThresholdCrossEventMode {
        match self {
            Self::ValidPulse { .. } => ThresholdCrossEventMode::Rising,
            Self::Runt { .. } => ThresholdCrossEventMode::Falling,
        }
    }
}

impl ExtractOutput for RuntResult {
    fn extract(store: &ValueStore, ids: &[NodeId]) -> Option<Self> {
        store.get_cloned(ids.first()?)
    }
}

// ============================================================================
// PulseWidthResult
// ============================================================================

/// Result of a pulse width measurement.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PulseWidthResult {
    /// Pulse was shorter than minimum width.
    TooShort {
        /// The actual pulse width in milliseconds.
        width_ms: i64,
    },
    /// Pulse width was within valid range.
    Valid {
        /// The actual pulse width in milliseconds.
        width_ms: i64,
    },
    /// Pulse was longer than maximum width.
    TooLong {
        /// The actual pulse width in milliseconds.
        width_ms: i64,
    },
}

impl PulseWidthResult {
    /// Returns `true` if this is a valid pulse width.
    #[must_use]
    pub const fn is_valid(&self) -> bool {
        matches!(self, Self::Valid { .. })
    }

    /// Returns `true` if the pulse was too short.
    #[must_use]
    pub const fn is_too_short(&self) -> bool {
        matches!(self, Self::TooShort { .. })
    }

    /// Returns `true` if the pulse was too long.
    #[must_use]
    pub const fn is_too_long(&self) -> bool {
        matches!(self, Self::TooLong { .. })
    }

    /// Get the pulse width in milliseconds.
    #[must_use]
    pub const fn width_ms(&self) -> i64 {
        match self {
            Self::TooShort { width_ms } | Self::Valid { width_ms } | Self::TooLong { width_ms } => {
                *width_ms
            }
        }
    }

    /// Get the pulse duration in milliseconds (alias for `width_ms`).
    #[must_use]
    pub const fn duration_ms(&self) -> Option<i64> {
        Some(self.width_ms())
    }

    /// Convert to threshold crossing mode.
    ///
    /// - `Valid` → `ThresholdCrossEventMode::Rising`
    /// - `TooShort` or `TooLong` → `ThresholdCrossEventMode::Falling`
    #[must_use]
    pub const fn to_threshold_cross(&self) -> ThresholdCrossEventMode {
        match self {
            Self::Valid { .. } => ThresholdCrossEventMode::Rising,
            Self::TooShort { .. } | Self::TooLong { .. } => ThresholdCrossEventMode::Falling,
        }
    }
}

impl ExtractOutput for PulseWidthResult {
    fn extract(store: &ValueStore, ids: &[NodeId]) -> Option<Self> {
        store.get_cloned(ids.first()?)
    }
}

// ============================================================================
// WindowEvent
// ============================================================================

/// Events generated by a window detector.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowEvent {
    /// Signal entered the window (from either side).
    EnteredWindow,
    /// Signal exited the window through the low threshold.
    ExitedLow,
    /// Signal exited the window through the high threshold.
    ExitedHigh,
}

impl WindowEvent {
    /// Returns `true` if this is an entry event.
    #[must_use]
    pub const fn is_entered(&self) -> bool {
        matches!(self, Self::EnteredWindow)
    }

    /// Returns `true` if this is an exit through the low threshold.
    #[must_use]
    pub const fn is_exited_low(&self) -> bool {
        matches!(self, Self::ExitedLow)
    }

    /// Returns `true` if this is an exit through the high threshold.
    #[must_use]
    pub const fn is_exited_high(&self) -> bool {
        matches!(self, Self::ExitedHigh)
    }

    /// Returns `true` if this is any exit event.
    #[must_use]
    pub const fn is_exited(&self) -> bool {
        matches!(self, Self::ExitedLow | Self::ExitedHigh)
    }

    /// Convert to threshold crossing mode.
    ///
    /// - `EnteredWindow` → `ThresholdCrossEventMode::Rising`
    /// - `ExitedLow` or `ExitedHigh` → `ThresholdCrossEventMode::Falling`
    #[must_use]
    pub const fn to_threshold_cross(&self) -> ThresholdCrossEventMode {
        match self {
            Self::EnteredWindow => ThresholdCrossEventMode::Rising,
            Self::ExitedLow | Self::ExitedHigh => ThresholdCrossEventMode::Falling,
        }
    }
}

impl ExtractOutput for WindowEvent {
    fn extract(store: &ValueStore, ids: &[NodeId]) -> Option<Self> {
        store.get_cloned(ids.first()?)
    }
}
