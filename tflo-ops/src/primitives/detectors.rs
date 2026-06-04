use super::cross::CrossDetector;
use super::event_mode::ThresholdCrossEventMode;

/// Cross detector with hysteresis to prevent signal chatter.
///
/// # The Problem: Signal Chatter
///
/// When a signal hovers near a threshold, noise causes it to rapidly cross back
/// and forth, generating many false triggers:
///
/// ```text
/// threshold ────────╮  ╭──╮ ╭─╮ ╭──╮  ╭────────────
///                   ╰──╯  ╰─╯ ╰─╯  ╰──╯
///                   ↑  ↑  ↑ ↑ ↑ ↑  ↑  ↑
///                   Multiple false triggers!
/// ```
///
/// # The Solution: Hysteresis
///
/// Hysteresis creates a "dead band" around the threshold. The signal must move
/// beyond the threshold by the hysteresis margin to trigger:
///
/// - To trigger **Buy**: value must exceed `threshold + hysteresis`
/// - To trigger **Sell**: value must fall below `threshold - hysteresis`
///
/// ```text
/// th + h ───────────────────────────────────────
///                        ↑ Must cross HERE to Buy
/// threshold ───────── noise zone (ignored) ────
///                        ↓ Must cross HERE to Sell
/// th - h ───────────────────────────────────────
/// ```
///
/// # Use Cases
///
/// - **Temperature control**: Prevent heater cycling when temp is near setpoint
/// - **Signal detection**: Avoid false acquisitions from noise near threshold
/// - **Trading**: Filter out insignificant price fluctuations
///
/// # Choosing the Hysteresis Value
///
/// - **Too small**: Won't filter noise effectively
/// - **Too large**: May miss legitimate signals
/// - **Rule of thumb**: Set hysteresis to 2-3x your expected noise amplitude
///
/// # Examples
///
/// ```rust
/// use tflo_ops::primitives::{HysteresisCrossDetector, ThresholdCrossEventMode};
///
/// // Threshold = 100, Hysteresis = 5
/// // Rising requires value ≥ 105
/// // Falling requires value ≤ 95
/// let mut detector = HysteresisCrossDetector::new(5.0);
///
/// // Initialize below threshold
/// assert_eq!(detector.update(90.0, 100.0), ThresholdCrossEventMode::None);
///
/// // Above threshold but within hysteresis band - no trigger
/// assert_eq!(detector.update(103.0, 100.0), ThresholdCrossEventMode::None);
///
/// // Above threshold + hysteresis - triggers Rising
/// assert_eq!(detector.update(106.0, 100.0), ThresholdCrossEventMode::Rising);
///
/// // Falls below threshold but within hysteresis band - no trigger
/// assert_eq!(detector.update(97.0, 100.0), ThresholdCrossEventMode::None);
///
/// // Falls below threshold - hysteresis - triggers Falling
/// assert_eq!(detector.update(94.0, 100.0), ThresholdCrossEventMode::Falling);
/// ```
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct HysteresisCrossDetector {
    hysteresis: f64,
    state: HysteresisState,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) enum HysteresisState {
    Unknown,
    Above,
    Below,
}

impl Default for CrossDetector {
    fn default() -> Self {
        Self::new()
    }
}

impl CrossDetector {
    /// Create a new cross detector.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            prev_value: None,
            prev_threshold: None,
        }
    }

    /// Update with new values and check for a cross.
    ///
    /// Returns:
    /// - `ThresholdCrossEventMode::Rising` if value crossed above threshold
    /// - `ThresholdCrossEventMode::Falling` if value crossed below threshold
    /// - `ThresholdCrossEventMode::None` if no cross occurred or this is the first observation
    pub fn update(&mut self, value: f64, threshold: f64) -> ThresholdCrossEventMode {
        let edge = match (self.prev_value, self.prev_threshold) {
            (Some(prev_val), Some(prev_thresh)) => {
                let was_below = prev_val < prev_thresh;
                let is_above = value >= threshold;
                let was_above = prev_val >= prev_thresh;
                let is_below = value < threshold;

                if was_below && is_above {
                    ThresholdCrossEventMode::Rising // Crossed above
                } else if was_above && is_below {
                    ThresholdCrossEventMode::Falling // Crossed below
                } else {
                    ThresholdCrossEventMode::None
                }
            }
            _ => ThresholdCrossEventMode::None, // First observation
        };

        self.prev_value = Some(value);
        self.prev_threshold = Some(threshold);

        edge
    }

    /// Update and detect only crosses above the threshold.
    ///
    /// Returns `ThresholdCrossEventMode::Rising` if crossed above, `ThresholdCrossEventMode::None` otherwise.
    pub fn update_above(&mut self, value: f64, threshold: f64) -> ThresholdCrossEventMode {
        match self.update(value, threshold) {
            ThresholdCrossEventMode::Rising => ThresholdCrossEventMode::Rising,
            ThresholdCrossEventMode::Falling | ThresholdCrossEventMode::None => {
                ThresholdCrossEventMode::None
            }
        }
    }

    /// Update and detect only crosses below the threshold.
    ///
    /// Returns `ThresholdCrossEventMode::Falling` if crossed below, `ThresholdCrossEventMode::None` otherwise.
    pub fn update_below(&mut self, value: f64, threshold: f64) -> ThresholdCrossEventMode {
        match self.update(value, threshold) {
            ThresholdCrossEventMode::Falling => ThresholdCrossEventMode::Falling,
            ThresholdCrossEventMode::Rising | ThresholdCrossEventMode::None => {
                ThresholdCrossEventMode::None
            }
        }
    }

    /// Reset the detector state.
    pub const fn reset(&mut self) {
        self.prev_value = None;
        self.prev_threshold = None;
    }

    /// Check if the detector has been initialized with at least one observation.
    #[must_use]
    pub const fn is_initialized(&self) -> bool {
        self.prev_value.is_some()
    }

    /// Get the previous value.
    #[must_use]
    pub const fn prev_value(&self) -> Option<f64> {
        self.prev_value
    }

    /// Get the previous threshold.
    #[must_use]
    pub const fn prev_threshold(&self) -> Option<f64> {
        self.prev_threshold
    }
}

impl HysteresisCrossDetector {
    /// Create a new hysteresis cross detector.
    ///
    /// # Arguments
    ///
    /// * `hysteresis` - The margin beyond the threshold required to trigger.
    ///   Must be positive. Larger values provide more noise immunity but may
    ///   miss smaller legitimate signals.
    #[must_use]
    pub const fn new(hysteresis: f64) -> Self {
        Self {
            hysteresis,
            state: HysteresisState::Unknown,
        }
    }

    /// Update with new values and check for a cross with hysteresis.
    ///
    /// Returns:
    /// - `ThresholdCrossEventMode::Rising` if value crossed above `threshold + hysteresis`
    /// - `ThresholdCrossEventMode::Falling` if value crossed below `threshold - hysteresis`
    /// - `ThresholdCrossEventMode::None` otherwise
    pub fn update(&mut self, value: f64, threshold: f64) -> ThresholdCrossEventMode {
        match self.state {
            HysteresisState::Unknown => {
                // Initialize state based on first observation
                if value >= threshold {
                    self.state = HysteresisState::Above;
                } else {
                    self.state = HysteresisState::Below;
                }
                ThresholdCrossEventMode::None
            }
            HysteresisState::Below => {
                // Need to cross above threshold + hysteresis to trigger
                if value >= threshold + self.hysteresis {
                    self.state = HysteresisState::Above;
                    ThresholdCrossEventMode::Rising
                } else {
                    ThresholdCrossEventMode::None
                }
            }
            HysteresisState::Above => {
                // Need to cross below threshold - hysteresis to trigger
                if value <= threshold - self.hysteresis {
                    self.state = HysteresisState::Below;
                    ThresholdCrossEventMode::Falling
                } else {
                    ThresholdCrossEventMode::None
                }
            }
        }
    }

    /// Get the current state of the detector.
    #[must_use]
    pub fn is_above(&self) -> bool {
        self.state == HysteresisState::Above
    }

    /// Get the current state of the detector.
    #[must_use]
    pub fn is_below(&self) -> bool {
        self.state == HysteresisState::Below
    }

    /// Get the hysteresis value.
    #[must_use]
    pub const fn hysteresis(&self) -> f64 {
        self.hysteresis
    }

    /// Reset the detector state.
    pub const fn reset(&mut self) {
        self.state = HysteresisState::Unknown;
    }
}
