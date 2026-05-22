use super::results::PulseWidthResult;

/// Filters out glitches (pulses shorter than a minimum duration).
///
/// # What is a Glitch?
///
/// A glitch is a brief, unwanted pulse typically caused by:
/// - Electrical noise or interference
/// - Sensor bounce or contact chatter
/// - Transient conditions that don't represent real signal changes
///
/// ```text
/// Valid pulses have sufficient duration, glitches are too short:
///
///     ╭────────╮     ╭╮        ╭──────────────╮
/// ────┴────────┴─────┴┴────────┴──────────────┴───
///     │  10ms  │     1ms       │     20ms     │
///     ▲              ▲         ▲
///     VALID        GLITCH!     VALID
///    (≥5ms)       (<5ms)      (≥5ms)
/// ```
///
/// # How It Works
///
/// The filter tracks when the signal crosses above the threshold and measures
/// how long it stays above. Only when the signal crosses back below AND was
/// above for at least `min_duration` is it considered a valid pulse.
///
/// # Use Cases
///
/// - **RF signal processing**: Ignore brief noise spikes in SNR measurements
/// - **Switch debouncing**: Filter out mechanical contact bounce
/// - **Sensor filtering**: Remove spurious readings from noisy sensors
/// - **Network monitoring**: Ignore brief disconnections (micro-outages)
///
/// # Examples
///
/// ```rust
/// use tflo_core::primitives::GlitchFilter;
///
/// // Require pulses to be at least 5ms to be valid
/// let mut filter = GlitchFilter::new(100.0, 5);
///
/// // Signal goes high at t=0
/// assert_eq!(filter.update(110.0, 0), None);
///
/// // Still high at t=2ms (too short to be valid yet)
/// assert_eq!(filter.update(110.0, 2), None);
///
/// // Goes low at t=3ms - pulse was only 3ms, FILTERED OUT
/// assert_eq!(filter.update(90.0, 3), Some(false)); // false = glitch
///
/// // New pulse starts at t=10ms
/// assert_eq!(filter.update(110.0, 10), None);
///
/// // Pulse ends at t=20ms - 10ms duration, VALID
/// assert_eq!(filter.update(90.0, 20), Some(true)); // true = valid pulse
/// ```
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct GlitchFilter {
    threshold: f64,
    min_duration_ms: i64,
    state: GlitchState,
    pulse_start_ts: Option<i64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) enum GlitchState {
    Low,
    High,
}

/// Detects and validates pulse widths against min/max requirements.
///
/// # What is Pulse Width Detection?
///
/// Pulse width detection measures how long a signal stays above a threshold
/// and validates it against timing requirements:
///
/// ```text
///     ╭──╮      ╭────────╮      ╭─────────────────────╮
/// ────┴──┴──────┴────────┴──────┴─────────────────────┴──
///     │2ms│     │  8ms   │      │       25ms          │
///     ▲         ▲               ▲
///     TOO       VALID           TOO
///     SHORT    (5-15ms)         LONG
/// ```
///
/// # Use Cases
///
/// - **Protocol validation**: Ensure pulses meet timing specifications
/// - **Quality control**: Detect timing drift in production systems
/// - **Signal characterization**: Measure pulse statistics over time
/// - **Anomaly detection**: Flag signals outside normal timing ranges
///
/// # Examples
///
/// ```rust
/// use tflo_core::primitives::{PulseWidthDetector, PulseWidthResult};
///
/// // Valid pulses must be between 5ms and 15ms
/// let mut detector = PulseWidthDetector::new(100.0, 5, 15);
///
/// // Pulse starts at t=0
/// assert_eq!(detector.update(110.0, 0), None);
///
/// // Pulse ends at t=3ms - TOO SHORT
/// let result = detector.update(90.0, 3);
/// assert!(matches!(result, Some(PulseWidthResult::TooShort { .. })));
///
/// // New pulse starts at t=10ms
/// assert_eq!(detector.update(110.0, 10), None);
///
/// // Pulse ends at t=18ms - 8ms duration, VALID
/// let result = detector.update(90.0, 18);
/// assert!(matches!(result, Some(PulseWidthResult::Valid { .. })));
///
/// // Another pulse at t=20ms
/// assert_eq!(detector.update(110.0, 20), None);
///
/// // Ends at t=50ms - 30ms, TOO LONG
/// let result = detector.update(90.0, 50);
/// assert!(matches!(result, Some(PulseWidthResult::TooLong { .. })));
/// ```
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PulseWidthDetector {
    threshold: f64,
    min_width_ms: i64,
    max_width_ms: i64,
    state: PulseWidthState,
    pulse_start_ts: Option<i64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) enum PulseWidthState {
    Low,
    High,
}

impl GlitchFilter {
    /// Create a new glitch filter.
    ///
    /// # Arguments
    ///
    /// * `threshold` - The amplitude threshold for detecting pulses
    /// * `min_duration_ms` - Minimum pulse duration in milliseconds to be valid
    #[must_use]
    pub fn new(threshold: f64, min_duration_ms: i64) -> Self {
        Self {
            threshold,
            min_duration_ms,
            state: GlitchState::Low,
            pulse_start_ts: None,
        }
    }

    /// Update with a new sample.
    ///
    /// # Arguments
    ///
    /// * `value` - The signal amplitude
    /// * `ts_ms` - The timestamp in milliseconds
    ///
    /// # Returns
    ///
    /// - `None` if no pulse transition occurred
    /// - `Some(true)` if a valid pulse ended (duration ≥ min_duration)
    /// - `Some(false)` if a glitch was filtered out (duration < min_duration)
    pub fn update(&mut self, value: f64, ts_ms: i64) -> Option<bool> {
        let is_high = value >= self.threshold;

        match (self.state, is_high) {
            (GlitchState::Low, true) => {
                // Rising edge - start timing the pulse
                self.state = GlitchState::High;
                self.pulse_start_ts = Some(ts_ms);
                None
            }
            (GlitchState::High, false) => {
                // Falling edge - check if pulse was long enough
                self.state = GlitchState::Low;
                if let Some(start_ts) = self.pulse_start_ts.take() {
                    let duration = ts_ms - start_ts;
                    Some(duration >= self.min_duration_ms)
                } else {
                    Some(false)
                }
            }
            _ => None, // No state change
        }
    }

    /// Check if currently in a pulse (above threshold).
    #[must_use]
    pub fn is_high(&self) -> bool {
        self.state == GlitchState::High
    }

    /// Get the current pulse duration if in a pulse.
    ///
    /// Returns `None` if not currently in a pulse.
    #[must_use]
    pub fn current_pulse_duration(&self, current_ts_ms: i64) -> Option<i64> {
        self.pulse_start_ts.map(|start| current_ts_ms - start)
    }

    /// Reset the filter state.
    pub fn reset(&mut self) {
        self.state = GlitchState::Low;
        self.pulse_start_ts = None;
    }
}

impl PulseWidthDetector {
    /// Create a new pulse width detector.
    ///
    /// # Arguments
    ///
    /// * `threshold` - The amplitude threshold for detecting pulses
    /// * `min_width_ms` - Minimum valid pulse width in milliseconds
    /// * `max_width_ms` - Maximum valid pulse width in milliseconds
    ///
    /// # Panics
    ///
    /// Panics if `min_width_ms > max_width_ms`.
    #[must_use]
    pub fn new(threshold: f64, min_width_ms: i64, max_width_ms: i64) -> Self {
        assert!(
            min_width_ms <= max_width_ms,
            "min_width_ms must be <= max_width_ms"
        );
        Self {
            threshold,
            min_width_ms,
            max_width_ms,
            state: PulseWidthState::Low,
            pulse_start_ts: None,
        }
    }

    /// Update with a new sample.
    ///
    /// # Arguments
    ///
    /// * `value` - The signal amplitude
    /// * `ts_ms` - The timestamp in milliseconds
    ///
    /// # Returns
    ///
    /// - `None` if no pulse ended
    /// - `Some(PulseWidthResult)` with the width classification when a pulse ends
    pub fn update(&mut self, value: f64, ts_ms: i64) -> Option<PulseWidthResult> {
        let is_high = value >= self.threshold;

        match (self.state, is_high) {
            (PulseWidthState::Low, true) => {
                // Rising edge
                self.state = PulseWidthState::High;
                self.pulse_start_ts = Some(ts_ms);
                None
            }
            (PulseWidthState::High, false) => {
                // Falling edge - measure width
                self.state = PulseWidthState::Low;
                if let Some(start_ts) = self.pulse_start_ts.take() {
                    let width_ms = ts_ms - start_ts;
                    Some(self.classify_width(width_ms))
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    fn classify_width(&self, width_ms: i64) -> PulseWidthResult {
        if width_ms < self.min_width_ms {
            PulseWidthResult::TooShort { width_ms }
        } else if width_ms > self.max_width_ms {
            PulseWidthResult::TooLong { width_ms }
        } else {
            PulseWidthResult::Valid { width_ms }
        }
    }

    /// Check if currently in a pulse.
    #[must_use]
    pub fn is_in_pulse(&self) -> bool {
        self.state == PulseWidthState::High
    }

    /// Get the current pulse duration if in a pulse.
    #[must_use]
    pub fn current_width(&self, current_ts_ms: i64) -> Option<i64> {
        self.pulse_start_ts.map(|start| current_ts_ms - start)
    }

    /// Reset the detector state.
    pub fn reset(&mut self) {
        self.state = PulseWidthState::Low;
        self.pulse_start_ts = None;
    }
}
