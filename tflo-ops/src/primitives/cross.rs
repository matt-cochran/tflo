//! Signal crossing and triggering primitives.
//!
//! This module provides a comprehensive set of signal detection primitives inspired by
//! oscilloscope trigger concepts. These are essential building blocks for:
//!
//! - **Trading systems**: Generating buy/sell signals when prices cross moving averages
//! - **RF signal processing**: Detecting signal acquisition, loss, and anomalies
//! - **Industrial monitoring**: Alerting when measurements exceed acceptable bounds
//! - **Sensor data validation**: Filtering noise and detecting incomplete transitions
//!
//! # Trigger Types Overview
//!
//! | Trigger | Detects | Use Case |
//! |---------|---------|----------|
//! | [`CrossDetector`] | Value crossing a threshold | Basic edge detection |
//! | [`HysteresisCrossDetector`] | Crossing with noise immunity | Stable triggers near threshold |
//! | [`GlitchFilter`] | Pulses too short to be valid | Noise spike rejection |
//! | [`RuntDetector`] | Incomplete amplitude transitions | Weak signal detection |
//! | [`PulseWidthDetector`] | Pulse duration analysis | Timing validation |
//! | [`WindowDetector`] | Signal inside/outside amplitude band | Bounds monitoring |
//!
//! # Visual Guide to Trigger Concepts
//!
//! ```text
//! EDGE TRIGGER (CrossDetector)
//! ════════════════════════════════════════════════════════════════
//! Triggers when signal crosses threshold in specified direction.
//!
//!          ╭────────╮
//!          │        │      Rising edge (Buy)
//!    ──────┘        └────  triggers HERE ↓
//!    ─────────────────────  threshold ─────────────────────────
//!                    ↑
//!              Falling edge (Sell)
//!
//!
//! HYSTERESIS TRIGGER (HysteresisCrossDetector)
//! ════════════════════════════════════════════════════════════════
//! Prevents false triggers when signal oscillates near threshold.
//! Signal must cross threshold ± hysteresis margin to trigger.
//!
//!              Must cross HERE to trigger Buy ──╮
//!    th + h ─────────────────────────────────────┼────────────
//!                    ╭──╮  ╭─╮  ╭───╮            │
//!    threshold ──────┤  │──┤ │──┤   │────────────┼────────────
//!                    │  │  │ │  │   │            ↓
//!    th - h ─────────┴──┴──┴─┴──┴───┴────────────────────────
//!              Must cross HERE to trigger Sell ──╯
//!
//!    Without hysteresis: Multiple false triggers from noise
//!    With hysteresis: Clean single trigger when truly crossing
//!
//!
//! GLITCH FILTER (GlitchFilter)
//! ════════════════════════════════════════════════════════════════
//! Ignores pulses shorter than minimum duration (noise spikes).
//!
//!          ╭────────╮     ╭╮        ╭──────────────╮
//!    ──────┴────────┴─────┴┴────────┴──────────────┴───
//!          │        │     ││        │              │
//!          ╰──8ms───╯     1ms       ╰────15ms──────╯
//!          ▲              ▲         ▲
//!          valid          GLITCH!   valid
//!         (≥5ms)         (<5ms)    (≥5ms)
//!
//!    min_duration = 5ms → Short spike is filtered out
//!
//!
//! RUNT DETECTOR (RuntDetector)
//! ════════════════════════════════════════════════════════════════
//! Detects pulses that cross low threshold but not high threshold.
//! Indicates weak or incomplete signal transitions.
//!
//!    high ───┬──────────────────────╭─────────────╮──────
//!            │                      │  VALID      │
//!            │     ╭──╮    ╭─╮      │  PULSE      │
//!    low ────┼─────┤  ├────┤ ├──────┴─────────────┴──────
//!            │     │  │    │ │
//!    ────────┴─────┴──┴────┴─┴───────────────────────────
//!                  ▲       ▲        ▲
//!                  RUNT    RUNT     normal
//!             (crosses low    (reached high
//!              not high)       threshold)
//!
//!
//! PULSE WIDTH DETECTOR (PulseWidthDetector)
//! ════════════════════════════════════════════════════════════════
//! Validates pulse duration against min/max requirements.
//!
//!          ╭──╮      ╭────────╮      ╭─────────────────────╮
//!    ──────┴──┴──────┴────────┴──────┴─────────────────────┴──
//!          │2ms│     │  8ms   │      │       25ms          │
//!          ▲         ▲               ▲
//!          TOO       VALID           TOO
//!          SHORT    (5-15ms)         LONG
//!
//!    min_width=5ms, max_width=15ms
//!
//!
//! WINDOW DETECTOR (WindowDetector)
//! ════════════════════════════════════════════════════════════════
//! Monitors when signal enters or exits an amplitude window.
//!
//!                         EXIT
//!                          ↓
//!    high ───┬─────────╭───────────╮─────────────────────
//!            │         │ OUTSIDE   │
//!            │ ╭───╮   │           │   ╭───╮
//!    low ────┼─┴───┴───┴───────────┴───┴───┴─────────────
//!            │ INSIDE  ↑               INSIDE
//!            │        ENTER
//!    ────────┴───────────────────────────────────────────
//!               ▲                        ▲
//!            Signal                   Signal
//!            in bounds               in bounds
//! ```

// ============================================================================
// BASIC EDGE TRIGGER
// ============================================================================

/// Detects when one value crosses above or below another.
///
/// A "cross above" occurs when the value transitions from below to above the threshold.
/// A "cross below" occurs when the value transitions from above to below the threshold.
///
/// # Use Cases
///
/// - **Trading**: Generate buy signals when price crosses above moving average
/// - **Threshold alerts**: Trigger when temperature exceeds safe limits
/// - **State detection**: Identify when a sensor reading changes state
///
/// # Examples
///
/// ```rust
/// use tflo_ops::primitives::{CrossDetector, ThresholdCrossEventMode};
///
/// let mut detector = CrossDetector::new();
///
/// // Below threshold
/// assert_eq!(detector.update(90.0, 100.0), ThresholdCrossEventMode::None);
///
/// // Still below
/// assert_eq!(detector.update(95.0, 100.0), ThresholdCrossEventMode::None);
///
/// // Crosses above!
/// assert_eq!(detector.update(105.0, 100.0), ThresholdCrossEventMode::Rising);
///
/// // Still above
/// assert_eq!(detector.update(110.0, 100.0), ThresholdCrossEventMode::None);
///
/// // Crosses below!
/// assert_eq!(detector.update(95.0, 100.0), ThresholdCrossEventMode::Falling);
/// ```
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CrossDetector {
    pub(crate) prev_value: Option<f64>,
    pub(crate) prev_threshold: Option<f64>,
}

// ============================================================================
// HYSTERESIS TRIGGER
// ============================================================================

// ============================================================================
// GLITCH FILTER
// ============================================================================

// ============================================================================
// RUNT DETECTOR
// ============================================================================

// ============================================================================
// PULSE WIDTH DETECTOR
// ============================================================================

// ============================================================================
// WINDOW DETECTOR
// ============================================================================

// ============================================================================
// TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::primitives::{
        GlitchFilter, HysteresisCrossDetector, PulseWidthDetector, PulseWidthResult, RuntDetector,
        RuntResult, ThresholdCrossEventMode, WindowDetector, WindowEvent,
    };

    // --- CrossDetector Tests ---

    #[test]
    fn test_cross_above() {
        let mut detector = CrossDetector::new();

        assert_eq!(detector.update(90.0, 100.0), ThresholdCrossEventMode::None); // First obs
        assert_eq!(detector.update(95.0, 100.0), ThresholdCrossEventMode::None); // Still below
        assert_eq!(
            detector.update(105.0, 100.0),
            ThresholdCrossEventMode::Rising
        ); // Crossed above
        assert_eq!(detector.update(110.0, 100.0), ThresholdCrossEventMode::None); // Still above
    }

    #[test]
    fn test_cross_below() {
        let mut detector = CrossDetector::new();

        assert_eq!(detector.update(110.0, 100.0), ThresholdCrossEventMode::None); // First obs
        assert_eq!(detector.update(105.0, 100.0), ThresholdCrossEventMode::None); // Still above
        assert_eq!(
            detector.update(95.0, 100.0),
            ThresholdCrossEventMode::Falling
        ); // Crossed below
        assert_eq!(detector.update(90.0, 100.0), ThresholdCrossEventMode::None); // Still below
    }

    #[test]
    fn test_update_above_only() {
        let mut detector = CrossDetector::new();

        let _ = detector.update_above(110.0, 100.0); // Initialize above
        assert_eq!(
            detector.update_above(95.0, 100.0),
            ThresholdCrossEventMode::None
        ); // Cross below ignored
        assert_eq!(
            detector.update_above(105.0, 100.0),
            ThresholdCrossEventMode::Rising
        ); // Cross above detected
    }

    // --- HysteresisCrossDetector Tests ---

    #[test]
    fn test_hysteresis() {
        let mut detector = HysteresisCrossDetector::new(5.0);

        let _ = detector.update(90.0, 100.0); // Initialize below
        assert_eq!(detector.update(103.0, 100.0), ThresholdCrossEventMode::None); // Above but not by 5
        assert_eq!(
            detector.update(106.0, 100.0),
            ThresholdCrossEventMode::Rising
        ); // Above by >5
        assert_eq!(detector.update(97.0, 100.0), ThresholdCrossEventMode::None); // Below but not by 5
        assert_eq!(
            detector.update(94.0, 100.0),
            ThresholdCrossEventMode::Falling
        ); // Below by >5
    }

    #[test]
    fn test_hysteresis_state_accessors() {
        let mut detector = HysteresisCrossDetector::new(5.0);

        let _ = detector.update(90.0, 100.0);
        assert!(detector.is_below());
        assert!(!detector.is_above());

        let _ = detector.update(106.0, 100.0);
        assert!(!detector.is_below());
        assert!(detector.is_above());

        assert_eq!(detector.hysteresis(), 5.0);
    }

    // --- GlitchFilter Tests ---

    #[test]
    fn test_glitch_filter_valid_pulse() {
        let mut filter = GlitchFilter::new(100.0, 5);

        assert_eq!(filter.update(110.0, 0), None); // Start pulse
        assert!(filter.is_high());
        assert_eq!(filter.update(110.0, 3), None); // Still high
        assert_eq!(filter.update(90.0, 10), Some(true)); // End after 10ms - valid
    }

    #[test]
    fn test_glitch_filter_glitch() {
        let mut filter = GlitchFilter::new(100.0, 5);

        assert_eq!(filter.update(110.0, 0), None); // Start pulse
        assert_eq!(filter.update(90.0, 3), Some(false)); // End after 3ms - glitch
    }

    #[test]
    fn test_glitch_filter_duration_accessor() {
        let mut filter = GlitchFilter::new(100.0, 5);

        assert_eq!(filter.current_pulse_duration(10), None);
        let _ = filter.update(110.0, 10);
        assert_eq!(filter.current_pulse_duration(15), Some(5));
    }

    // --- RuntDetector Tests ---

    #[test]
    fn test_runt_detector_valid_pulse() {
        let mut detector = RuntDetector::new(30.0, 70.0);

        assert_eq!(detector.update(20.0), None); // Below low
        assert_eq!(detector.update(50.0), None); // Between low and high
        assert_eq!(detector.update(80.0), None); // Above high
        let result = detector.update(20.0); // Back below low
        assert!(matches!(result, Some(RuntResult::ValidPulse { .. })));
    }

    #[test]
    fn test_runt_detector_runt_pulse() {
        let mut detector = RuntDetector::new(30.0, 70.0);

        assert_eq!(detector.update(20.0), None); // Below low
        assert_eq!(detector.update(50.0), None); // Between low and high
        let result = detector.update(20.0); // Back below without reaching high
        assert!(matches!(result, Some(RuntResult::Runt { .. })));
    }

    #[test]
    fn test_runt_detector_peak_tracking() {
        let mut detector = RuntDetector::new(30.0, 70.0);

        let _ = detector.update(20.0);
        let _ = detector.update(50.0);
        assert_eq!(detector.current_peak(), Some(50.0));
        let _ = detector.update(60.0);
        assert_eq!(detector.current_peak(), Some(60.0));
    }

    // --- PulseWidthDetector Tests ---

    #[test]
    fn test_pulse_width_valid() {
        let mut detector = PulseWidthDetector::new(100.0, 5, 15);

        assert_eq!(detector.update(110.0, 0), None);
        let result = detector.update(90.0, 10);
        assert_eq!(result, Some(PulseWidthResult::Valid { width_ms: 10 }));
    }

    #[test]
    fn test_pulse_width_too_short() {
        let mut detector = PulseWidthDetector::new(100.0, 5, 15);

        assert_eq!(detector.update(110.0, 0), None);
        let result = detector.update(90.0, 3);
        assert_eq!(result, Some(PulseWidthResult::TooShort { width_ms: 3 }));
    }

    #[test]
    fn test_pulse_width_too_long() {
        let mut detector = PulseWidthDetector::new(100.0, 5, 15);

        assert_eq!(detector.update(110.0, 0), None);
        let result = detector.update(90.0, 25);
        assert_eq!(result, Some(PulseWidthResult::TooLong { width_ms: 25 }));
    }

    #[test]
    fn test_pulse_width_in_pulse_accessor() {
        let mut detector = PulseWidthDetector::new(100.0, 5, 15);

        assert!(!detector.is_in_pulse());
        let _ = detector.update(110.0, 0);
        assert!(detector.is_in_pulse());
        assert_eq!(detector.current_width(5), Some(5));
    }

    // --- WindowDetector Tests ---

    #[test]
    fn test_window_enter_from_below() {
        let mut detector = WindowDetector::new(4.5, 5.5);

        assert_eq!(detector.update(4.0), None); // Initialize below
        assert_eq!(detector.update(5.0), Some(WindowEvent::EnteredWindow));
        assert!(detector.is_inside());
    }

    #[test]
    fn test_window_enter_from_above() {
        let mut detector = WindowDetector::new(4.5, 5.5);

        assert_eq!(detector.update(6.0), None); // Initialize above
        assert_eq!(detector.update(5.0), Some(WindowEvent::EnteredWindow));
    }

    #[test]
    fn test_window_exit_low() {
        let mut detector = WindowDetector::new(4.5, 5.5);

        let _ = detector.update(5.0); // Initialize inside
        assert_eq!(detector.update(4.0), Some(WindowEvent::ExitedLow));
        assert!(detector.is_below());
    }

    #[test]
    fn test_window_exit_high() {
        let mut detector = WindowDetector::new(4.5, 5.5);

        let _ = detector.update(5.0); // Initialize inside
        assert_eq!(detector.update(6.0), Some(WindowEvent::ExitedHigh));
        assert!(detector.is_above());
    }

    #[test]
    fn test_window_no_event_while_inside() {
        let mut detector = WindowDetector::new(4.5, 5.5);

        let _ = detector.update(5.0); // Initialize inside
        assert_eq!(detector.update(5.2), None);
        assert_eq!(detector.update(4.8), None);
        assert_eq!(detector.update(5.4), None);
    }

    #[test]
    fn test_window_thresholds_accessor() {
        let detector = WindowDetector::new(4.5, 5.5);
        assert_eq!(detector.thresholds(), (4.5, 5.5));
    }
}
