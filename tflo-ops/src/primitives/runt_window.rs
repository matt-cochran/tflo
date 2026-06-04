use super::results::RuntResult;
use super::results::WindowEvent;

/// Detects runt pulses (pulses that cross one threshold but not another).
///
/// # What is a Runt Pulse?
///
/// A runt pulse is an incomplete signal transition that:
/// - Crosses the LOW threshold (starts rising)
/// - But fails to reach the HIGH threshold (incomplete)
/// - Then returns below LOW
///
/// This typically indicates:
/// - Weak signal strength
/// - Marginal transmission quality
/// - Failing hardware
/// - Impedance mismatches
///
/// ```text
/// HIGH ───┬──────────────────────╭─────────────╮──────
///         │                      │  VALID      │
///         │     ╭──╮    ╭─╮      │  PULSE      │
/// LOW ────┼─────┤  ├────┤ ├──────┴─────────────┴──────
///         │     │  │    │ │
/// ────────┴─────┴──┴────┴─┴───────────────────────────
///               ▲       ▲        ▲
///               RUNT    RUNT     VALID
///          (crosses low   (reached high)
///           not high)
/// ```
///
/// # Use Cases
///
/// - **RF signal quality**: Detect weak signals that don't reach full amplitude
/// - **Digital communications**: Identify marginal signal integrity
/// - **Power supply monitoring**: Detect brownouts (voltage dips)
/// - **Sensor validation**: Flag sensors producing weak readings
///
/// # Examples
///
/// ```rust
/// use tflo_ops::primitives::{RuntDetector, RuntResult};
///
/// // LOW = 30, HIGH = 70 (valid signals must reach 70+)
/// let mut detector = RuntDetector::new(30.0, 70.0);
///
/// // Signal is below low threshold
/// assert_eq!(detector.update(20.0), None);
///
/// // Signal crosses low threshold - starts potential pulse
/// assert_eq!(detector.update(50.0), None);
///
/// // Signal returns below low WITHOUT reaching high - RUNT!
/// assert_eq!(detector.update(25.0), Some(RuntResult::Runt { peak: 50.0 }));
///
/// // New pulse starts
/// assert_eq!(detector.update(50.0), None);
///
/// // This time reaches high threshold
/// assert_eq!(detector.update(80.0), None);
///
/// // Returns below low - VALID PULSE
/// assert_eq!(detector.update(25.0), Some(RuntResult::ValidPulse { peak: 80.0 }));
/// ```
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RuntDetector {
    low_threshold: f64,
    high_threshold: f64,
    state: RuntState,
    reached_high: bool,
    peak_value: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) enum RuntState {
    /// Signal is below low threshold
    BelowLow,
    /// Signal is between low and high thresholds
    InTransition,
    /// Signal is above high threshold
    AboveHigh,
}

/// Detects when a signal enters or exits an amplitude window.
///
/// # What is Window Detection?
///
/// Window detection monitors whether a signal is within an acceptable
/// amplitude range (between low and high thresholds):
///
/// ```text
///                         EXIT ↓
/// HIGH ───┬─────────╭───────────╮─────────────────────
///         │         │ OUTSIDE   │
///         │ ╭───╮   │   WINDOW  │   ╭───╮
/// LOW ────┼─┴───┴───┴───────────┴───┴───┴─────────────
///         │ INSIDE      ↑ ENTER     INSIDE
///         │
/// ────────┴───────────────────────────────────────────
///            ▲                        ▲
///         Signal                   Signal
///         in bounds               in bounds
/// ```
///
/// # Use Cases
///
/// - **Process control**: Alert when temperature exits acceptable range
/// - **Quality monitoring**: Flag products outside specification
/// - **Signal integrity**: Detect when SNR falls outside operating window
/// - **Voltage monitoring**: Detect over/under voltage conditions
///
/// # Triggering Modes
///
/// - **`EnteringWindow`**: Triggers when signal transitions from outside to inside
/// - **`ExitingWindow`**: Triggers when signal transitions from inside to outside
/// - **Both**: Triggers on any transition
///
/// # Examples
///
/// ```rust
/// use tflo_ops::primitives::{WindowDetector, WindowEvent};
///
/// // Monitor voltage between 4.5V and 5.5V
/// let mut detector = WindowDetector::new(4.5, 5.5);
///
/// // Start outside window (low)
/// assert_eq!(detector.update(4.0), None);
///
/// // Enter window
/// assert_eq!(detector.update(5.0), Some(WindowEvent::EnteredWindow));
///
/// // Stay inside - no event
/// assert_eq!(detector.update(5.2), None);
///
/// // Exit window (high)
/// assert_eq!(detector.update(6.0), Some(WindowEvent::ExitedHigh));
///
/// // Enter window again
/// assert_eq!(detector.update(5.0), Some(WindowEvent::EnteredWindow));
///
/// // Exit window (low)
/// assert_eq!(detector.update(4.0), Some(WindowEvent::ExitedLow));
/// ```
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct WindowDetector {
    low_threshold: f64,
    high_threshold: f64,
    state: WindowState,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) enum WindowState {
    Unknown,
    BelowWindow,
    InsideWindow,
    AboveWindow,
}

impl RuntDetector {
    /// Create a new runt detector.
    ///
    /// # Arguments
    ///
    /// * `low_threshold` - The lower threshold that starts a potential pulse
    /// * `high_threshold` - The upper threshold that validates a pulse
    ///
    /// # Panics
    ///
    /// Panics if `low_threshold >= high_threshold`.
    #[must_use]
    pub fn new(low_threshold: f64, high_threshold: f64) -> Self {
        assert!(
            low_threshold < high_threshold,
            "low_threshold must be less than high_threshold"
        );
        Self {
            low_threshold,
            high_threshold,
            state: RuntState::BelowLow,
            reached_high: false,
            peak_value: f64::NEG_INFINITY,
        }
    }

    /// Update with a new sample.
    ///
    /// # Returns
    ///
    /// - `None` if no pulse completion occurred
    /// - `Some(RuntResult::Runt { peak })` if a runt pulse completed
    /// - `Some(RuntResult::ValidPulse { peak })` if a valid pulse completed
    pub fn update(&mut self, value: f64) -> Option<RuntResult> {
        // Track peak value during pulse
        if self.state != RuntState::BelowLow {
            self.peak_value = self.peak_value.max(value);
        }

        // Check if we've reached high threshold
        if value >= self.high_threshold {
            self.reached_high = true;
        }

        match self.state {
            RuntState::BelowLow => {
                if value >= self.high_threshold {
                    self.state = RuntState::AboveHigh;
                    self.reached_high = true;
                    self.peak_value = value;
                } else if value >= self.low_threshold {
                    self.state = RuntState::InTransition;
                    self.reached_high = false;
                    self.peak_value = value;
                }
                None
            }
            RuntState::InTransition => {
                if value >= self.high_threshold {
                    self.state = RuntState::AboveHigh;
                    self.reached_high = true;
                    None
                } else if value < self.low_threshold {
                    // Pulse ended without reaching high - RUNT!
                    self.state = RuntState::BelowLow;
                    let result = if self.reached_high {
                        RuntResult::ValidPulse {
                            peak: self.peak_value,
                        }
                    } else {
                        RuntResult::Runt {
                            peak: self.peak_value,
                        }
                    };
                    self.reset_pulse();
                    Some(result)
                } else {
                    None
                }
            }
            RuntState::AboveHigh => {
                if value < self.low_threshold {
                    // Valid pulse completed
                    self.state = RuntState::BelowLow;
                    let result = RuntResult::ValidPulse {
                        peak: self.peak_value,
                    };
                    self.reset_pulse();
                    Some(result)
                } else if value < self.high_threshold {
                    self.state = RuntState::InTransition;
                    None
                } else {
                    None
                }
            }
        }
    }

    const fn reset_pulse(&mut self) {
        self.reached_high = false;
        self.peak_value = f64::NEG_INFINITY;
    }

    /// Check if currently tracking a potential pulse.
    #[must_use]
    pub fn is_in_pulse(&self) -> bool {
        self.state != RuntState::BelowLow
    }

    /// Get the current peak value if in a pulse.
    #[must_use]
    pub fn current_peak(&self) -> Option<f64> {
        if self.is_in_pulse() {
            Some(self.peak_value)
        } else {
            None
        }
    }

    /// Reset the detector state.
    pub const fn reset(&mut self) {
        self.state = RuntState::BelowLow;
        self.reset_pulse();
    }
}

impl WindowDetector {
    /// Create a new window detector.
    ///
    /// # Arguments
    ///
    /// * `low_threshold` - The lower bound of the acceptable window
    /// * `high_threshold` - The upper bound of the acceptable window
    ///
    /// # Panics
    ///
    /// Panics if `low_threshold >= high_threshold`.
    #[must_use]
    pub fn new(low_threshold: f64, high_threshold: f64) -> Self {
        assert!(
            low_threshold < high_threshold,
            "low_threshold must be less than high_threshold"
        );
        Self {
            low_threshold,
            high_threshold,
            state: WindowState::Unknown,
        }
    }

    /// Update with a new sample.
    ///
    /// # Returns
    ///
    /// - `None` if no transition occurred
    /// - `Some(WindowEvent)` if the signal entered or exited the window
    pub fn update(&mut self, value: f64) -> Option<WindowEvent> {
        let new_state = if value < self.low_threshold {
            WindowState::BelowWindow
        } else if value > self.high_threshold {
            WindowState::AboveWindow
        } else {
            WindowState::InsideWindow
        };

        let event = match (self.state, new_state) {
            // First observation - no event
            (WindowState::Unknown, _) => None,

            // Entering window from below or above
            (WindowState::BelowWindow, WindowState::InsideWindow)
            | (WindowState::AboveWindow, WindowState::InsideWindow) => {
                Some(WindowEvent::EnteredWindow)
            }

            // Exiting window through low
            (WindowState::InsideWindow, WindowState::BelowWindow) => Some(WindowEvent::ExitedLow),

            // Exiting window through high
            (WindowState::InsideWindow, WindowState::AboveWindow) => Some(WindowEvent::ExitedHigh),

            // No transition or staying in same state
            _ => None,
        };

        self.state = new_state;
        event
    }

    /// Check if the signal is currently inside the window.
    #[must_use]
    pub fn is_inside(&self) -> bool {
        self.state == WindowState::InsideWindow
    }

    /// Check if the signal is currently outside the window.
    #[must_use]
    pub const fn is_outside(&self) -> bool {
        matches!(
            self.state,
            WindowState::BelowWindow | WindowState::AboveWindow
        )
    }

    /// Check if below the window.
    #[must_use]
    pub fn is_below(&self) -> bool {
        self.state == WindowState::BelowWindow
    }

    /// Check if above the window.
    #[must_use]
    pub fn is_above(&self) -> bool {
        self.state == WindowState::AboveWindow
    }

    /// Get the window thresholds.
    #[must_use]
    pub const fn thresholds(&self) -> (f64, f64) {
        (self.low_threshold, self.high_threshold)
    }

    /// Reset the detector state.
    pub const fn reset(&mut self) {
        self.state = WindowState::Unknown;
    }
}
