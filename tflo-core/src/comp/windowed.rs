//! Windowed aggregation and statistical operations on `Comp`.
//!
//! All methods accept `impl Into<Window>`, supporting both
//! `Duration` (time-based) and `usize` (count-based) windows.

use super::{Comp, Node};
use crate::window::Window;

impl<R: 'static> Comp<R, f64> {
    // ========================================================================
    // AGGREGATIONS
    // ========================================================================

    /// Simple moving average over a window.
    ///
    /// Accepts either a `Duration` (time-based) or `usize` (count-based).
    ///
    /// ```ignore
    /// let time_sma = price.sma(Duration::from_secs(300));
    /// let count_sma = price.sma(20usize);
    /// ```
    #[must_use]
    pub fn sma(&self, window: impl Into<Window>) -> Comp<R> {
        Self::add_node_to_state(&self.state, Node::Sma(self.id, window.into()))
    }

    /// Exponential moving average with time-based or count-based decay.
    #[must_use]
    pub fn ema(&self, window: impl Into<Window>) -> Comp<R> {
        Self::add_node_to_state(&self.state, Node::Ema(self.id, window.into()))
    }

    /// Standard deviation over a window.
    #[must_use]
    pub fn std(&self, window: impl Into<Window>) -> Comp<R> {
        Self::add_node_to_state(&self.state, Node::Std(self.id, window.into()))
    }

    /// Variance over a window.
    #[must_use]
    pub fn variance(&self, window: impl Into<Window>) -> Comp<R> {
        Self::add_node_to_state(&self.state, Node::Variance(self.id, window.into()))
    }

    /// Maximum value over a window.
    #[must_use]
    pub fn max(&self, window: impl Into<Window>) -> Comp<R> {
        Self::add_node_to_state(&self.state, Node::Max(self.id, window.into()))
    }

    /// Minimum value over a window.
    #[must_use]
    pub fn min(&self, window: impl Into<Window>) -> Comp<R> {
        Self::add_node_to_state(&self.state, Node::Min(self.id, window.into()))
    }

    /// Sum of values over a window.
    #[must_use]
    pub fn sum(&self, window: impl Into<Window>) -> Comp<R> {
        Self::add_node_to_state(&self.state, Node::Sum(self.id, window.into()))
    }

    /// Count of values in a window.
    #[must_use]
    pub fn count(&self, window: impl Into<Window>) -> Comp<R> {
        Self::add_node_to_state(&self.state, Node::Count(self.id, window.into()))
    }

    /// Weighted moving average (linearly increasing weights for more recent values).
    #[must_use]
    pub fn wma(&self, window: impl Into<Window>) -> Comp<R> {
        Self::add_node_to_state(&self.state, Node::Wma(self.id, window.into()))
    }

    /// Relative Strength Index over a window.
    ///
    /// Returns a value between 0 and 100. Values > 70 typically indicate
    /// overbought conditions, < 30 indicates oversold.
    #[must_use]
    pub fn rsi(&self, window: impl Into<Window>) -> Comp<R> {
        Self::add_node_to_state(&self.state, Node::Rsi(self.id, window.into()))
    }

    /// RSI using Wilder's smoothing (count-based only).
    ///
    /// Uses EMA with alpha = 1/period for gain/loss averaging,
    /// matching TradingView's RSI implementation.
    #[must_use]
    pub fn rsi_wilder_n(&self, n: usize) -> Comp<R> {
        Self::add_node_to_state(&self.state, Node::RsiWilder(self.id, n))
    }

    // ========================================================================
    // STATISTICAL
    // ========================================================================

    /// Rolling median over a window.
    #[must_use]
    pub fn median(&self, window: impl Into<Window>) -> Comp<R> {
        Self::add_node_to_state(&self.state, Node::Median(self.id, window.into()))
    }

    /// Rolling quantile over a window.
    ///
    /// `q` should be between 0.0 (minimum) and 1.0 (maximum).
    /// 0.5 gives the median.
    #[must_use]
    pub fn quantile(&self, window: impl Into<Window>, q: f64) -> Comp<R> {
        Self::add_node_to_state(&self.state, Node::Quantile(self.id, window.into(), q))
    }

    /// Rolling Pearson correlation with another value over a window.
    #[must_use]
    pub fn correlation(&self, other: &Comp<R>, window: impl Into<Window>) -> Comp<R> {
        Self::add_node_to_state(
            &self.state,
            Node::Correlation(self.id, other.id, window.into()),
        )
    }

    /// Rolling covariance with another value over a window.
    #[must_use]
    pub fn covariance(&self, other: &Comp<R>, window: impl Into<Window>) -> Comp<R> {
        Self::add_node_to_state(
            &self.state,
            Node::Covariance(self.id, other.id, window.into()),
        )
    }

    /// Rolling skewness over a window.
    #[must_use]
    pub fn skewness(&self, window: impl Into<Window>) -> Comp<R> {
        Self::add_node_to_state(&self.state, Node::Skewness(self.id, window.into()))
    }

    /// Rolling excess kurtosis over a window.
    #[must_use]
    pub fn kurtosis(&self, window: impl Into<Window>) -> Comp<R> {
        Self::add_node_to_state(&self.state, Node::Kurtosis(self.id, window.into()))
    }

    /// Rolling rank (percentile of current value within window).
    ///
    /// Returns a value between 0.0 (smallest in window) and 1.0 (largest in window).
    #[must_use]
    pub fn rank(&self, window: impl Into<Window>) -> Comp<R> {
        Self::add_node_to_state(&self.state, Node::Rank(self.id, window.into()))
    }
}
