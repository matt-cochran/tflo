//! Finance-named aliases for generic outlier/trend operations in `tflo-core`.
//!
//! The underlying math lives in `tflo-core` under domain-neutral names because
//! it is useful well beyond finance. These aliases let finance-oriented code
//! keep its familiar vocabulary.

use tflo_core::comp::Comp;
use tflo_core::window::Window;

/// Finance-named aliases for `tflo-core`'s domain-neutral operations.
///
/// Each method is a thin pass-through — no duplicated math.
pub trait FintechAliases<R> {
    /// Bollinger Bands — alias for [`Comp::deviation_band`](tflo_core::comp::Comp::deviation_band).
    ///
    /// Returns `(middle, upper, lower)`. Standard parameters: window 20, k 2.0.
    fn bollinger_bands(&self, window: impl Into<Window>, k: f64) -> (Comp<R>, Comp<R>, Comp<R>);

    /// Drawdown — alias for [`Comp::peak_decline`](tflo_core::comp::Comp::peak_decline).
    fn drawdown(&self) -> Comp<R>;

    /// Momentum — alias for [`Comp::momentum`](tflo_core::comp::Comp::momentum).
    fn mom_n(&self, period: usize) -> Comp<R>;

    /// Rate of change — alias for [`Comp::rate_of_change`](tflo_core::comp::Comp::rate_of_change).
    fn roc_n(&self, period: usize) -> Comp<R>;
}

impl<R: 'static> FintechAliases<R> for Comp<R, f64> {
    fn bollinger_bands(&self, window: impl Into<Window>, k: f64) -> (Comp<R>, Comp<R>, Comp<R>) {
        self.deviation_band(window, k)
    }

    fn drawdown(&self) -> Comp<R> {
        self.peak_decline()
    }

    fn mom_n(&self, period: usize) -> Comp<R> {
        self.momentum(period)
    }

    fn roc_n(&self, period: usize) -> Comp<R> {
        self.rate_of_change(period)
    }
}
