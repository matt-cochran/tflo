use tflo_core::primitives::HysteresisCrossDetector;
use tflo_core::event::ThresholdCrossEventMode;
use tflo_core::primitives::CrossDetector;
use tflo_core::primitives::GlitchFilter;
use tflo_core::primitives::RuntDetector;
use tflo_core::primitives::RuntResult;
use tflo_core::primitives::PulseWidthDetector;
use tflo_core::primitives::PulseWidthResult;
use tflo_core::primitives::WindowDetector;
use tflo_core::primitives::WindowEvent;


/// When HysteresisCrossDetector is created with a hysteresis value,
/// the detector shall initialize in Unknown state,
/// So that the first observation establishes the baseline,
/// And the detector will be ready to detect crossings.
#[test]
fn test_hysteresis_detector_initialization() {
    let detector = HysteresisCrossDetector::new(5.0);
    assert!(format!("{detector:?}").contains("HysteresisCrossDetector"));
}

/// When value crosses threshold plus hysteresis from below,
/// HysteresisCrossDetector shall return `ThresholdCrossEventMode::Rising`,
/// So that only significant upward crossings are reported,
/// And the detector will be in Above state.
#[test]
fn test_hysteresis_cross_above() {
    let mut detector = HysteresisCrossDetector::new(5.0);

    // First observation establishes baseline (below threshold)
    assert_eq!(detector.update(90.0, 100.0), ThresholdCrossEventMode::None);

    // Value above threshold but within hysteresis - no signal
    assert_eq!(detector.update(103.0, 100.0), ThresholdCrossEventMode::None);

    // Value above threshold + hysteresis - Buy signal
    assert_eq!(detector.update(106.0, 100.0), ThresholdCrossEventMode::Rising);
}

/// When value crosses threshold minus hysteresis from above,
/// HysteresisCrossDetector shall return `ThresholdCrossEventMode::Falling`,
/// So that only significant downward crossings are reported,
/// And the detector will be in Below state.
#[test]
fn test_hysteresis_cross_below() {
    let mut detector = HysteresisCrossDetector::new(5.0);

    // Establish above baseline
    assert_eq!(detector.update(110.0, 100.0), ThresholdCrossEventMode::None);

    // Value below threshold but within hysteresis - no signal
    assert_eq!(detector.update(97.0, 100.0), ThresholdCrossEventMode::None);

    // Value below threshold - hysteresis - Sell signal
    assert_eq!(detector.update(94.0, 100.0), ThresholdCrossEventMode::Falling);
}

/// When reset is called on HysteresisCrossDetector,
/// the detector shall return to Unknown state,
/// So that subsequent observations start fresh,
/// And the next observation will establish a new baseline.
#[test]
fn test_hysteresis_reset() {
    let mut detector = HysteresisCrossDetector::new(5.0);

    // Establish state
    let _ = detector.update(110.0, 100.0);
    let _ = detector.update(94.0, 100.0);

    // Reset
    detector.reset();

    // Next observation should be Unknown state initialization
    assert_eq!(detector.update(90.0, 100.0), ThresholdCrossEventMode::None);
}

/// When CrossDetector detects a crossing,
/// the detector shall return the appropriate signal,
/// So that trend changes are identified,
/// And the signal will be Buy for upward, Sell for downward.
#[test]
fn test_cross_detector_comprehensive() {
    let mut detector = CrossDetector::new();

    // Initialize below
    let _ = detector.update_above(90.0, 100.0);

    // Cross above - Buy
    assert_eq!(
        detector.update_above(110.0, 100.0),
        ThresholdCrossEventMode::Rising
    );

    // Stay above - None
    assert_eq!(
        detector.update_above(115.0, 100.0),
        ThresholdCrossEventMode::None
    );

    // Cross below - handled by update_below
    let mut detector2 = CrossDetector::new();
    let _ = detector2.update(110.0, 100.0); // Initialize above
    assert_eq!(
        detector2.update(90.0, 100.0),
        ThresholdCrossEventMode::Falling
    );
}

/// When GlitchFilter receives a pulse shorter than min_duration,
/// the filter shall classify it as a glitch,
/// So that noise spikes are filtered out,
/// And only valid pulses are passed through.
#[test]
fn test_glitch_filter_rejects_short_pulses() {
    let mut filter = GlitchFilter::new(100.0, 5); // 5ms minimum

    // Pulse starts at t=0
    assert_eq!(filter.update(110.0, 0), None);
    assert!(filter.is_high());

    // Pulse ends at t=3ms (too short)
    let result = filter.update(90.0, 3);
    assert_eq!(result, Some(false)); // false = glitch
}

/// When GlitchFilter receives a pulse longer than min_duration,
/// the filter shall classify it as valid,
/// So that real signals pass through,
/// And the pulse duration meets requirements.
#[test]
fn test_glitch_filter_accepts_valid_pulses() {
    let mut filter = GlitchFilter::new(100.0, 5); // 5ms minimum

    // Pulse starts at t=10
    assert_eq!(filter.update(110.0, 10), None);

    // Pulse ends at t=20 (10ms duration, valid)
    let result = filter.update(90.0, 20);
    assert_eq!(result, Some(true)); // true = valid
}

/// When current_pulse_duration is called during a pulse,
/// GlitchFilter shall return the elapsed time,
/// So that users can monitor pulse progress,
/// And take action before pulse ends.
#[test]
fn test_glitch_filter_tracks_duration() {
    let mut filter = GlitchFilter::new(100.0, 5);

    // No pulse yet
    assert_eq!(filter.current_pulse_duration(0), None);

    // Start pulse at t=10
    let _ = filter.update(110.0, 10);

    // Check duration at t=15
    assert_eq!(filter.current_pulse_duration(15), Some(5));
    assert_eq!(filter.current_pulse_duration(20), Some(10));
}

/// When RuntDetector sees a pulse that crosses low but not high threshold,
/// the detector shall classify it as a runt,
/// So that weak/incomplete signals are identified,
/// And signal quality issues are detected.
#[test]
fn test_runt_detector_detects_runts() {
    let mut detector = RuntDetector::new(30.0, 70.0);

    // Below low
    assert_eq!(detector.update(20.0), None);

    // Enter transition zone (above low, below high)
    assert_eq!(detector.update(50.0), None);
    assert!(detector.is_in_pulse());
    assert_eq!(detector.current_peak(), Some(50.0));

    // Return below low without reaching high - RUNT
    let result = detector.update(20.0);
    assert!(matches!(result, Some(RuntResult::Runt { peak: 50.0 })));
}

/// When RuntDetector sees a pulse that crosses both thresholds,
/// the detector shall classify it as valid,
/// So that complete signal transitions are distinguished from runts,
/// And the peak value is recorded.
#[test]
fn test_runt_detector_valid_pulses() {
    let mut detector = RuntDetector::new(30.0, 70.0);

    let _ = detector.update(20.0); // Below low
    let _ = detector.update(50.0); // Transition
    let _ = detector.update(80.0); // Above high
    assert_eq!(detector.current_peak(), Some(80.0));

    // Complete pulse
    let result = detector.update(20.0);
    assert!(matches!(
        result,
        Some(RuntResult::ValidPulse { peak: 80.0 })
    ));
}

/// When RuntDetector tracks a pulse,
/// the detector shall record the peak value,
/// So that amplitude characteristics are captured,
/// And signal strength can be assessed.
#[test]
fn test_runt_detector_tracks_peak() {
    let mut detector = RuntDetector::new(30.0, 70.0);

    let _ = detector.update(20.0);
    let _ = detector.update(40.0);
    assert_eq!(detector.current_peak(), Some(40.0));

    let _ = detector.update(60.0);
    assert_eq!(detector.current_peak(), Some(60.0));

    // Peak should not decrease
    let _ = detector.update(50.0);
    assert_eq!(detector.current_peak(), Some(60.0));
}

/// When PulseWidthDetector measures a pulse within valid range,
/// the detector shall return Valid result,
/// So that compliant pulses are identified,
/// And timing validation succeeds.
#[test]
fn test_pulse_width_valid_range() {
    let mut detector = PulseWidthDetector::new(100.0, 5, 15);

    // Start pulse at t=0
    assert_eq!(detector.update(110.0, 0), None);
    assert!(detector.is_in_pulse());

    // End at t=10 (10ms, within 5-15)
    let result = detector.update(90.0, 10);
    assert_eq!(result, Some(PulseWidthResult::Valid { width_ms: 10 }));
}

/// When PulseWidthDetector measures a pulse below minimum,
/// the detector shall return TooShort result,
/// So that timing violations are flagged,
/// And the actual width is reported.
#[test]
fn test_pulse_width_too_short() {
    let mut detector = PulseWidthDetector::new(100.0, 5, 15);

    let _ = detector.update(110.0, 0);
    let result = detector.update(90.0, 3);
    assert_eq!(result, Some(PulseWidthResult::TooShort { width_ms: 3 }));
}

/// When PulseWidthDetector measures a pulse above maximum,
/// the detector shall return TooLong result,
/// So that timing drift is detected,
/// And the actual width is reported.
#[test]
fn test_pulse_width_too_long() {
    let mut detector = PulseWidthDetector::new(100.0, 5, 15);

    let _ = detector.update(110.0, 0);
    let result = detector.update(90.0, 25);
    assert_eq!(result, Some(PulseWidthResult::TooLong { width_ms: 25 }));
}

/// When current_width is called during a pulse,
/// PulseWidthDetector shall return elapsed time,
/// So that pulse progress can be monitored,
/// And early warnings can be generated.
#[test]
fn test_pulse_width_current_width() {
    let mut detector = PulseWidthDetector::new(100.0, 5, 15);

    assert!(!detector.is_in_pulse());
    let _ = detector.update(110.0, 10);
    assert!(detector.is_in_pulse());
    assert_eq!(detector.current_width(17), Some(7));
}

/// When WindowDetector sees signal enter the window from below,
/// the detector shall return EnteredWindow event,
/// So that users know the signal is now in bounds,
/// And appropriate actions can be taken.
#[test]
fn test_window_enter_from_below() {
    let mut detector = WindowDetector::new(4.5, 5.5);

    // Initialize below window
    assert_eq!(detector.update(4.0), None);
    assert!(detector.is_below());
    assert!(detector.is_outside());

    // Enter window
    let event = detector.update(5.0);
    assert_eq!(event, Some(WindowEvent::EnteredWindow));
    assert!(detector.is_inside());
}

/// When WindowDetector sees signal enter the window from above,
/// the detector shall return EnteredWindow event,
/// So that users know the signal is now in bounds,
/// And the entry direction is unified.
#[test]
fn test_window_enter_from_above() {
    let mut detector = WindowDetector::new(4.5, 5.5);

    // Initialize above window
    assert_eq!(detector.update(6.0), None);
    assert!(detector.is_above());

    // Enter window
    let event = detector.update(5.0);
    assert_eq!(event, Some(WindowEvent::EnteredWindow));
}

/// When WindowDetector sees signal exit through low threshold,
/// the detector shall return ExitedLow event,
/// So that users know the signal fell below minimum,
/// And under-voltage conditions are detected.
#[test]
fn test_window_exit_low() {
    let mut detector = WindowDetector::new(4.5, 5.5);

    let _ = detector.update(5.0); // Start inside
    let event = detector.update(4.0);
    assert_eq!(event, Some(WindowEvent::ExitedLow));
}

/// When WindowDetector sees signal exit through high threshold,
/// the detector shall return ExitedHigh event,
/// So that users know the signal exceeded maximum,
/// And over-voltage conditions are detected.
#[test]
fn test_window_exit_high() {
    let mut detector = WindowDetector::new(4.5, 5.5);

    let _ = detector.update(5.0); // Start inside
    let event = detector.update(6.0);
    assert_eq!(event, Some(WindowEvent::ExitedHigh));
}

/// When WindowDetector sees signal move within the window,
/// the detector shall return None,
/// So that only boundary crossings trigger events,
/// And normal fluctuations don't generate noise.
#[test]
fn test_window_no_event_while_inside() {
    let mut detector = WindowDetector::new(4.5, 5.5);

    let _ = detector.update(5.0); // Initialize inside

    // Movement within window generates no events
    assert_eq!(detector.update(5.1), None);
    assert_eq!(detector.update(4.8), None);
    assert_eq!(detector.update(5.4), None);
    assert!(detector.is_inside());
}

/// When WindowDetector sees signal move outside the window boundaries,
/// the detector shall return None,
/// So that only actual transitions trigger events,
/// And staying outside doesn't flood with events.
#[test]
fn test_window_no_event_while_outside() {
    let mut detector = WindowDetector::new(4.5, 5.5);

    let _ = detector.update(3.0); // Initialize below

    // Movement outside window generates no events
    assert_eq!(detector.update(2.5), None);
    assert_eq!(detector.update(3.5), None);
    assert_eq!(detector.update(4.0), None);
    assert!(detector.is_outside());
}

/// When thresholds is called on WindowDetector,
/// the detector shall return the configured bounds,
/// So that users can verify configuration,
/// And the window range is accessible.
#[test]
fn test_window_thresholds_accessor() {
    let detector = WindowDetector::new(4.5, 5.5);
    assert_eq!(detector.thresholds(), (4.5, 5.5));
}

/// When reset is called on WindowDetector,
/// the detector shall clear its state,
/// So that subsequent observations start fresh,
/// And the next observation will reinitialize.
#[test]
fn test_window_reset() {
    let mut detector = WindowDetector::new(4.5, 5.5);

    let _ = detector.update(5.0);
    assert!(detector.is_inside());

    detector.reset();

    // After reset, first observation is initialization (no event)
    assert_eq!(detector.update(5.0), None);
}
