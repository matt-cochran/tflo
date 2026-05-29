//! Relative Strength Index (RSI) primitive.
//!
//! RSI is a momentum indicator that measures the magnitude of recent price changes
//! to evaluate overbought or oversold conditions.

use std::collections::VecDeque;
use std::time::Duration;
use tflo_core::operator::WindowPrimitive;

/// Count-based Relative Strength Index calculator.
///
/// RSI = 100 - (100 / (1 + RS))
/// where RS = Average Gain / Average Loss over the period.
///
/// Traditional RSI uses a smoothed (Wilder's) moving average for gains/losses,
/// but this implementation uses simple moving average for sliding windows.
///
/// # Examples
///
/// ```rust
/// use tflo_ops::primitives::RsiCountWindow;
///
/// let mut rsi = RsiCountWindow::new(14);
///
/// // Push a series of prices
/// for price in [44.0, 44.25, 44.5, 43.75, 44.5, 44.25, 44.0, 43.5, 43.25, 43.0, 43.25, 43.5, 43.75, 44.0, 44.25] {
///     rsi.push(price);
/// }
///
/// let value = rsi.rsi();
/// assert!(value >= 0.0 && value <= 100.0);
/// ```
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RsiCountWindow {
    period: usize,
    gains: VecDeque<f64>,
    losses: VecDeque<f64>,
    prev_value: Option<f64>,
    sum_gains: f64,
    sum_losses: f64,
}

impl RsiCountWindow {
    /// Create a new count-based RSI calculator with the given period.
    ///
    /// The standard RSI period is 14.
    #[must_use]
    pub fn new(period: usize) -> Self {
        Self {
            period,
            gains: VecDeque::with_capacity(period),
            losses: VecDeque::with_capacity(period),
            prev_value: None,
            sum_gains: 0.0,
            sum_losses: 0.0,
        }
    }

    /// Push a new value and update RSI calculation.
    pub fn push(&mut self, value: f64) {
        if let Some(prev) = self.prev_value {
            let change = value - prev;

            let (gain, loss) = if change > 0.0 {
                (change, 0.0)
            } else {
                (0.0, -change)
            };

            // Evict oldest if at capacity
            if self.gains.len() >= self.period {
                if let Some(old_gain) = self.gains.pop_front() {
                    self.sum_gains -= old_gain;
                }
                if let Some(old_loss) = self.losses.pop_front() {
                    self.sum_losses -= old_loss;
                }
            }

            // Add new values
            self.gains.push_back(gain);
            self.losses.push_back(loss);
            self.sum_gains += gain;
            self.sum_losses += loss;
        }

        self.prev_value = Some(value);
    }

    /// Get the number of changes in the window.
    ///
    /// Note: This is one less than the number of values pushed,
    /// since RSI is calculated from changes between consecutive values.
    #[must_use]
    pub fn count(&self) -> usize {
        self.gains.len()
    }

    /// Check if we have enough data for a meaningful RSI.
    #[must_use]
    pub fn is_valid(&self) -> bool {
        self.gains.len() >= self.period
    }

    /// Get the current RSI value.
    ///
    /// Returns a value between 0 and 100:
    /// - RSI > 70: Typically considered overbought
    /// - RSI < 30: Typically considered oversold
    ///
    /// Returns `f64::NAN` if not enough data.
    #[must_use]
    pub fn rsi(&self) -> f64 {
        if self.gains.is_empty() {
            return f64::NAN;
        }

        let n = self.gains.len() as f64;
        let avg_gain = self.sum_gains / n;
        let avg_loss = self.sum_losses / n;

        if avg_loss == 0.0 {
            if avg_gain == 0.0 {
                return 50.0; // No movement
            }
            return 100.0; // All gains, no losses
        }

        let rs = avg_gain / avg_loss;
        100.0 - (100.0 / (1.0 + rs))
    }

    /// Get the average gain.
    #[must_use]
    pub fn avg_gain(&self) -> f64 {
        if self.gains.is_empty() {
            f64::NAN
        } else {
            self.sum_gains / self.gains.len() as f64
        }
    }

    /// Get the average loss.
    #[must_use]
    pub fn avg_loss(&self) -> f64 {
        if self.losses.is_empty() {
            f64::NAN
        } else {
            self.sum_losses / self.losses.len() as f64
        }
    }

    /// Get the Relative Strength (RS = avg gain / avg loss).
    #[must_use]
    pub fn rs(&self) -> f64 {
        let avg_loss = self.avg_loss();
        if avg_loss == 0.0 || avg_loss.is_nan() {
            f64::NAN
        } else {
            self.avg_gain() / avg_loss
        }
    }

    /// Clear the window.
    pub fn clear(&mut self) {
        self.gains.clear();
        self.losses.clear();
        self.prev_value = None;
        self.sum_gains = 0.0;
        self.sum_losses = 0.0;
    }
}

/// Time-based Relative Strength Index calculator.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RsiTimeWindow {
    window_ms: i64,
    buffer: VecDeque<(i64, f64, f64)>, // (ts, gain, loss)
    prev_value: Option<f64>,
    sum_gains: f64,
    sum_losses: f64,
}

impl RsiTimeWindow {
    /// Create a new time-based RSI calculator.
    #[must_use]
    pub const fn new(window: Duration) -> Self {
        #[allow(clippy::cast_possible_wrap)]
        let window_ms = window.as_millis() as i64;
        Self {
            window_ms,
            buffer: VecDeque::new(),
            prev_value: None,
            sum_gains: 0.0,
            sum_losses: 0.0,
        }
    }

    /// Push a new value at the given timestamp.
    pub fn push(&mut self, ts: i64, value: f64) {
        if let Some(prev) = self.prev_value {
            let change = value - prev;

            let (gain, loss) = if change > 0.0 {
                (change, 0.0)
            } else {
                (0.0, -change)
            };

            // Add new value
            self.buffer.push_back((ts, gain, loss));
            self.sum_gains += gain;
            self.sum_losses += loss;

            // Evict old values.
            // SAFETY: `ts - window_ms` is the standard time-cutoff pattern;
            // underflow ("clamp to before time zero") is a meaningful
            // semantic for the eviction check below.
            let cutoff = ts.saturating_sub(self.window_ms);
            while let Some(&(old_ts, old_gain, old_loss)) = self.buffer.front() {
                if old_ts < cutoff {
                    let _ = self.buffer.pop_front();
                    self.sum_gains -= old_gain;
                    self.sum_losses -= old_loss;
                } else {
                    break;
                }
            }
        }

        self.prev_value = Some(value);
    }

    /// Get the number of changes in the window.
    #[must_use]
    pub fn count(&self) -> usize {
        self.buffer.len()
    }

    /// Get the current RSI value.
    #[must_use]
    pub fn rsi(&self) -> f64 {
        if self.buffer.is_empty() {
            return f64::NAN;
        }

        let n = self.buffer.len() as f64;
        let avg_gain = self.sum_gains / n;
        let avg_loss = self.sum_losses / n;

        if avg_loss == 0.0 {
            if avg_gain == 0.0 {
                return 50.0;
            }
            return 100.0;
        }

        let rs = avg_gain / avg_loss;
        100.0 - (100.0 / (1.0 + rs))
    }

    /// Clear the window.
    pub fn clear(&mut self) {
        self.buffer.clear();
        self.prev_value = None;
        self.sum_gains = 0.0;
        self.sum_losses = 0.0;
    }
}

impl WindowPrimitive for RsiCountWindow {
    fn push(&mut self, _ts: i64, value: f64) {
        self.push(value);
    }

    fn len(&self) -> usize {
        self.count()
    }
}

impl WindowPrimitive for RsiTimeWindow {
    fn push(&mut self, ts: i64, value: f64) {
        self.push(ts, value);
    }

    fn len(&self) -> usize {
        self.count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rsi_bounds() {
        let mut rsi = RsiCountWindow::new(14);

        // Push various prices
        for price in [
            44.0, 44.25, 44.5, 43.75, 44.5, 44.25, 44.0, 43.5, 43.25, 43.0, 43.25, 43.5, 43.75,
            44.0, 44.25,
        ] {
            rsi.push(price);
        }

        let value = rsi.rsi();
        assert!((0.0..=100.0).contains(&value), "RSI {value} out of bounds");
    }

    #[test]
    fn test_rsi_all_gains() {
        let mut rsi = RsiCountWindow::new(5);

        // All gains: 10, 11, 12, 13, 14, 15
        for price in [10.0, 11.0, 12.0, 13.0, 14.0, 15.0] {
            rsi.push(price);
        }

        assert!((rsi.rsi() - 100.0).abs() < 0.001);
    }

    #[test]
    fn test_rsi_all_losses() {
        let mut rsi = RsiCountWindow::new(5);

        // All losses: 15, 14, 13, 12, 11, 10
        for price in [15.0, 14.0, 13.0, 12.0, 11.0, 10.0] {
            rsi.push(price);
        }

        assert!((rsi.rsi() - 0.0).abs() < 0.001);
    }

    #[test]
    fn test_rsi_balanced() {
        let mut rsi = RsiCountWindow::new(4);

        // Alternating gains and losses of same magnitude
        // 10 -> 11 (gain 1) -> 10 (loss 1) -> 11 (gain 1) -> 10 (loss 1)
        for price in [10.0, 11.0, 10.0, 11.0, 10.0] {
            rsi.push(price);
        }

        // Equal gains and losses should give RSI of 50
        assert!((rsi.rsi() - 50.0).abs() < 0.001);
    }

    #[test]
    fn test_rsi_eviction() {
        let mut rsi = RsiCountWindow::new(3);

        // Push 5 values, only last 3 changes should count
        for price in [10.0, 11.0, 12.0, 13.0, 14.0] {
            rsi.push(price);
        }

        assert_eq!(rsi.count(), 3);
    }

    #[test]
    fn test_rsi_empty() {
        let rsi = RsiCountWindow::new(14);
        assert!(rsi.rsi().is_nan());
    }

    #[test]
    fn test_rsi_single_value() {
        let mut rsi = RsiCountWindow::new(14);
        rsi.push(100.0);
        assert!(rsi.rsi().is_nan()); // Need at least one change
    }

    #[test]
    fn test_rsi_time_window() {
        let mut rsi = RsiTimeWindow::new(Duration::from_secs(5));

        rsi.push(1000, 100.0);
        rsi.push(2000, 101.0);
        rsi.push(3000, 102.0);
        rsi.push(4000, 101.0);

        assert!(!rsi.rsi().is_nan());
        assert!(rsi.rsi() >= 0.0 && rsi.rsi() <= 100.0);

        // Push value that evicts first change
        rsi.push(7500, 102.0);
        assert_eq!(rsi.count(), 3);
    }
}
