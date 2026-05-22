//! Windowed-aggregation operators and the [`WindowOps`] extension trait.
//!
//! Most windowed operators are a [`Windowed<W, R>`] pairing a window primitive
//! with a [`Reduce`] unit: the unit's body just calls the matching window
//! accessor (`w.mean()`, `w.std()`, …). Three operators are written as plain
//! [`Operator`] structs instead: [`Ema`] and [`RsiWilder`] do not fit the
//! reduce-the-buffer shape (they keep a recursively smoothed scalar, not a
//! sliding buffer); and [`QuantileOp`](crate::ops::stats) carries a parameter
//! (`q`) that must be serialized, which the `#[serde(skip)]`-ped `Windowed`
//! reduction slot cannot do.
//!
//! Every method is exposed on `Comp<R, f64>` through the single [`WindowOps`]
//! extension trait so call sites read naturally — e.g. `price.sma(20)`.

use crate::checkpoint;
use crate::ops::stats::{Correlation, Covariance, Kurtosis, Median, QuantileOp, Rank, Skewness};
use crate::primitives::{
    CorrelationCountWindow, CorrelationTimeWindow, CountEma, CountWindow, MedianCountWindow,
    MedianTimeWindow, MomentsCountWindow, MomentsTimeWindow, RsiCountWindow, RsiTimeWindow,
    TimeEma, TimeWindow, WmaCountWindow, WmaTimeWindow,
};
use crate::shapes::{BivariateWindowed, Reduce, Windowed};
use serde::{Deserialize, Serialize};
use tflo_core::comp::Comp;
use tflo_core::compile::{Computed, NodeOutput, finite_or_warming};
use tflo_core::operator::{BoxedOperator, Operator, OperatorLoadError, require};
use tflo_core::window::Window;

// ============================================================================
// Basic reductions
// ============================================================================

/// Arithmetic mean of a window — the simple moving average.
#[derive(Default)]
pub struct Mean;

/// Population standard deviation of a window.
#[derive(Default)]
pub struct Std;

/// Population variance of a window.
#[derive(Default)]
pub struct Variance;

/// Maximum value in a window.
#[derive(Default)]
pub struct Max;

/// Minimum value in a window.
#[derive(Default)]
pub struct Min;

/// Sum of the values in a window.
#[derive(Default)]
pub struct Sum;

/// Count of the values in a window.
#[derive(Default)]
pub struct Count;

/// Weighted moving average (linearly increasing weights toward recent values).
#[derive(Default)]
pub struct Wma;

/// Relative Strength Index over a window.
#[derive(Default)]
pub struct Rsi;

/// Generate the time- and count-window `Reduce` impls for one basic reduction.
///
/// The bodies are identical across a reduction's time- and count-window types,
/// so both impls come from one `accessor` expression.
macro_rules! impl_reduce {
    ($reduce:ty, $time:ty, $count:ty, $accessor:expr) => {
        impl Reduce<$time> for $reduce {
            fn reduce(&self, w: &$time) -> f64 {
                let f: fn(&$time) -> f64 = $accessor;
                f(w)
            }
        }
        impl Reduce<$count> for $reduce {
            fn reduce(&self, w: &$count) -> f64 {
                let f: fn(&$count) -> f64 = $accessor;
                f(w)
            }
        }
    };
}

impl_reduce!(Mean, TimeWindow, CountWindow, |w| w.mean());
impl_reduce!(Std, TimeWindow, CountWindow, |w| w.std());
impl_reduce!(Variance, TimeWindow, CountWindow, |w| w.variance());
impl_reduce!(Max, TimeWindow, CountWindow, |w| w.max());
impl_reduce!(Min, TimeWindow, CountWindow, |w| w.min());
impl_reduce!(Sum, TimeWindow, CountWindow, |w| w.sum());
impl_reduce!(Count, TimeWindow, CountWindow, |w| count_as_f64(w.count()));
impl_reduce!(Wma, WmaTimeWindow, WmaCountWindow, |w| w.wma());
impl_reduce!(Rsi, RsiTimeWindow, RsiCountWindow, |w| w.rsi());

/// Widen a window's element count to `f64` for the [`Count`] reduction.
///
/// Window sizes never exceed `2^53`, so the cast is exact in practice.
#[allow(clippy::cast_precision_loss)]
fn count_as_f64(n: usize) -> f64 {
    n as f64
}

// ============================================================================
// Hand-written operators
// ============================================================================

/// Exponential moving average over a time- or count-based window.
///
/// EMA keeps a single recursively smoothed value rather than a sliding buffer,
/// so it is not a [`Windowed`] reduction. It wraps the [`TimeEma`]/[`CountEma`]
/// primitives — the same primitives the legacy `tflo-core` catalog used — so
/// results are bit-identical.
#[derive(Serialize, Deserialize)]
enum Ema {
    /// Time-decayed EMA (halflife-based).
    Time(TimeEma),
    /// Count-based EMA (period-based smoothing factor).
    Count(CountEma),
}

impl Operator for Ema {
    fn eval(&mut self, inputs: &[Computed], ts: i64) -> NodeOutput {
        let v = match require(inputs, 0) {
            Ok(v) => v,
            Err(e) => return NodeOutput::computed(Err(e)), // absent input: skip the push
        };
        let out = match self {
            Self::Time(e) => e.push(ts, v),
            Self::Count(e) => e.push(v),
        };
        NodeOutput::computed(finite_or_warming(out))
    }

    fn name(&self) -> &str {
        "ema"
    }

    fn save(&self) -> Option<Vec<u8>> {
        checkpoint::save(self)
    }

    fn load(&mut self, bytes: &[u8]) -> Result<(), OperatorLoadError> {
        checkpoint::load(self, bytes)
    }
}

/// RSI using Wilder's smoothing (count-based only).
///
/// Uses an EMA with `alpha = 1/period` for gain/loss averaging, matching
/// TradingView's RSI. This is a hand-written port of the legacy `tflo-core`
/// `RsiWilderState` evaluation logic.
#[derive(Serialize, Deserialize)]
struct RsiWilder {
    period: usize,
    prev: Option<f64>,
    count: usize,
    sum_gain: f64,
    sum_loss: f64,
    avg_gain: f64,
    avg_loss: f64,
    initialized: bool,
}

impl RsiWilder {
    fn new(period: usize) -> Self {
        Self {
            period,
            prev: None,
            count: 0,
            sum_gain: 0.0,
            sum_loss: 0.0,
            avg_gain: 0.0,
            avg_loss: 0.0,
            initialized: false,
        }
    }

    /// Fold one value into the Wilder-smoothed RSI state.
    ///
    /// Ported verbatim from `tflo-core`'s `compute_rsi_wilder`; returns `NaN`
    /// while warming up so the caller routes it through [`finite_or_warming`].
    #[allow(clippy::cast_precision_loss)]
    fn update(&mut self, value: f64) -> f64 {
        if self.period == 0 {
            return f64::NAN;
        }

        let Some(prev) = self.prev else {
            self.prev = Some(value);
            return f64::NAN;
        };

        let change = value - prev;
        let gain = if change > 0.0 { change } else { 0.0 };
        let loss = if change < 0.0 { -change } else { 0.0 };
        self.prev = Some(value);

        if !self.initialized {
            self.count += 1;
            self.sum_gain += gain;
            self.sum_loss += loss;
            if self.count < self.period {
                return f64::NAN;
            }
            self.avg_gain = self.sum_gain / self.period as f64;
            self.avg_loss = self.sum_loss / self.period as f64;
            self.initialized = true;
        } else {
            self.avg_gain = (self.avg_gain * (self.period - 1) as f64 + gain) / self.period as f64;
            self.avg_loss = (self.avg_loss * (self.period - 1) as f64 + loss) / self.period as f64;
        }

        if self.avg_loss == 0.0 {
            if self.avg_gain == 0.0 { 50.0 } else { 100.0 }
        } else {
            100.0 - 100.0 / (1.0 + self.avg_gain / self.avg_loss)
        }
    }
}

impl Operator for RsiWilder {
    fn eval(&mut self, inputs: &[Computed], _ts: i64) -> NodeOutput {
        let v = match require(inputs, 0) {
            Ok(v) => v,
            Err(e) => return NodeOutput::computed(Err(e)), // absent input: skip the push
        };
        NodeOutput::computed(finite_or_warming(self.update(v)))
    }

    fn name(&self) -> &str {
        "rsi_wilder"
    }

    fn save(&self) -> Option<Vec<u8>> {
        checkpoint::save(self)
    }

    fn load(&mut self, bytes: &[u8]) -> Result<(), OperatorLoadError> {
        checkpoint::load(self, bytes)
    }
}

// ============================================================================
// WindowOps extension trait
// ============================================================================

/// Box an operator into a [`BoxedOperator`] — a touch terser than an `as` cast
/// at each `match` arm of a `_dyn` factory.
fn boxed<O: Operator>(op: O) -> BoxedOperator {
    Box::new(op)
}

/// Windowed aggregation and statistical operations on `Comp`.
///
/// All windowed methods accept `impl Into<Window>`, supporting both `Duration`
/// (time-based) and `usize` (count-based) windows. The single blanket impl
/// below adds every method to `Comp<R, f64>`.
pub trait WindowOps<R> {
    /// Simple moving average over a window.
    fn sma(&self, window: impl Into<Window>) -> Comp<R, f64>;
    /// Exponential moving average with time-based or count-based decay.
    fn ema(&self, window: impl Into<Window>) -> Comp<R, f64>;
    /// Population standard deviation over a window.
    fn std(&self, window: impl Into<Window>) -> Comp<R, f64>;
    /// Population variance over a window.
    fn variance(&self, window: impl Into<Window>) -> Comp<R, f64>;
    /// Maximum value over a window.
    fn max(&self, window: impl Into<Window>) -> Comp<R, f64>;
    /// Minimum value over a window.
    fn min(&self, window: impl Into<Window>) -> Comp<R, f64>;
    /// Sum of values over a window.
    fn sum(&self, window: impl Into<Window>) -> Comp<R, f64>;
    /// Count of values in a window.
    fn count(&self, window: impl Into<Window>) -> Comp<R, f64>;
    /// Weighted moving average (linearly increasing weights for recent values).
    fn wma(&self, window: impl Into<Window>) -> Comp<R, f64>;
    /// Relative Strength Index over a window (0–100).
    fn rsi(&self, window: impl Into<Window>) -> Comp<R, f64>;
    /// RSI using Wilder's smoothing over `n` periods (count-based only).
    fn rsi_wilder_n(&self, n: usize) -> Comp<R, f64>;
    /// Rolling median over a window.
    fn median(&self, window: impl Into<Window>) -> Comp<R, f64>;
    /// Rolling quantile over a window (`q` in `[0.0, 1.0]`; 0.5 = median).
    fn quantile(&self, window: impl Into<Window>, q: f64) -> Comp<R, f64>;
    /// Rolling rank (percentile of the current value within the window).
    fn rank(&self, window: impl Into<Window>) -> Comp<R, f64>;
    /// Rolling skewness over a window.
    fn skewness(&self, window: impl Into<Window>) -> Comp<R, f64>;
    /// Rolling excess kurtosis over a window.
    fn kurtosis(&self, window: impl Into<Window>) -> Comp<R, f64>;
    /// Rolling Pearson correlation with another value over a window.
    fn correlation(&self, other: &Comp<R, f64>, window: impl Into<Window>) -> Comp<R, f64>;
    /// Rolling covariance with another value over a window.
    fn covariance(&self, other: &Comp<R, f64>, window: impl Into<Window>) -> Comp<R, f64>;
}

impl<R: 'static> WindowOps<R> for Comp<R, f64> {
    fn sma(&self, window: impl Into<Window>) -> Comp<R, f64> {
        let w: Window = window.into();
        Comp::custom_node1_dyn(self, move || match w {
            Window::Time(d) => boxed(Windowed::new(TimeWindow::new(d), Mean)),
            Window::Count(n) => boxed(Windowed::new(CountWindow::new(n), Mean)),
        })
    }

    fn ema(&self, window: impl Into<Window>) -> Comp<R, f64> {
        let w: Window = window.into();
        Comp::custom_node1_dyn(self, move || match w {
            Window::Time(d) => boxed(Ema::Time(TimeEma::new(d))),
            Window::Count(n) => boxed(Ema::Count(CountEma::new(n))),
        })
    }

    fn std(&self, window: impl Into<Window>) -> Comp<R, f64> {
        let w: Window = window.into();
        Comp::custom_node1_dyn(self, move || match w {
            Window::Time(d) => boxed(Windowed::new(TimeWindow::new(d), Std)),
            Window::Count(n) => boxed(Windowed::new(CountWindow::new(n), Std)),
        })
    }

    fn variance(&self, window: impl Into<Window>) -> Comp<R, f64> {
        let w: Window = window.into();
        Comp::custom_node1_dyn(self, move || match w {
            Window::Time(d) => boxed(Windowed::new(TimeWindow::new(d), Variance)),
            Window::Count(n) => boxed(Windowed::new(CountWindow::new(n), Variance)),
        })
    }

    fn max(&self, window: impl Into<Window>) -> Comp<R, f64> {
        let w: Window = window.into();
        Comp::custom_node1_dyn(self, move || match w {
            Window::Time(d) => boxed(Windowed::new(TimeWindow::new(d), Max)),
            Window::Count(n) => boxed(Windowed::new(CountWindow::new(n), Max)),
        })
    }

    fn min(&self, window: impl Into<Window>) -> Comp<R, f64> {
        let w: Window = window.into();
        Comp::custom_node1_dyn(self, move || match w {
            Window::Time(d) => boxed(Windowed::new(TimeWindow::new(d), Min)),
            Window::Count(n) => boxed(Windowed::new(CountWindow::new(n), Min)),
        })
    }

    fn sum(&self, window: impl Into<Window>) -> Comp<R, f64> {
        let w: Window = window.into();
        Comp::custom_node1_dyn(self, move || match w {
            Window::Time(d) => boxed(Windowed::new(TimeWindow::new(d), Sum)),
            Window::Count(n) => boxed(Windowed::new(CountWindow::new(n), Sum)),
        })
    }

    fn count(&self, window: impl Into<Window>) -> Comp<R, f64> {
        let w: Window = window.into();
        Comp::custom_node1_dyn(self, move || match w {
            Window::Time(d) => boxed(Windowed::new(TimeWindow::new(d), Count)),
            Window::Count(n) => boxed(Windowed::new(CountWindow::new(n), Count)),
        })
    }

    fn wma(&self, window: impl Into<Window>) -> Comp<R, f64> {
        let w: Window = window.into();
        Comp::custom_node1_dyn(self, move || match w {
            Window::Time(d) => boxed(Windowed::new(WmaTimeWindow::new(d), Wma)),
            Window::Count(n) => boxed(Windowed::new(WmaCountWindow::new(n), Wma)),
        })
    }

    fn rsi(&self, window: impl Into<Window>) -> Comp<R, f64> {
        let w: Window = window.into();
        Comp::custom_node1_dyn(self, move || match w {
            Window::Time(d) => boxed(Windowed::new(RsiTimeWindow::new(d), Rsi)),
            Window::Count(n) => boxed(Windowed::new(RsiCountWindow::new(n), Rsi)),
        })
    }

    fn rsi_wilder_n(&self, n: usize) -> Comp<R, f64> {
        Comp::custom_node1_dyn(self, move || boxed(RsiWilder::new(n)))
    }

    fn median(&self, window: impl Into<Window>) -> Comp<R, f64> {
        let w: Window = window.into();
        Comp::custom_node1_dyn(self, move || match w {
            Window::Time(d) => boxed(Windowed::new(MedianTimeWindow::new(d), Median)),
            Window::Count(n) => boxed(Windowed::new(MedianCountWindow::new(n), Median)),
        })
    }

    fn quantile(&self, window: impl Into<Window>, q: f64) -> Comp<R, f64> {
        let w: Window = window.into();
        Comp::custom_node1_dyn(self, move || match w {
            Window::Time(d) => boxed(QuantileOp::Time {
                window: MedianTimeWindow::new(d),
                q,
            }),
            Window::Count(n) => boxed(QuantileOp::Count {
                window: MedianCountWindow::new(n),
                q,
            }),
        })
    }

    fn rank(&self, window: impl Into<Window>) -> Comp<R, f64> {
        let w: Window = window.into();
        Comp::custom_node1_dyn(self, move || match w {
            Window::Time(d) => boxed(Windowed::new(MedianTimeWindow::new(d), Rank)),
            Window::Count(n) => boxed(Windowed::new(MedianCountWindow::new(n), Rank)),
        })
    }

    fn skewness(&self, window: impl Into<Window>) -> Comp<R, f64> {
        let w: Window = window.into();
        Comp::custom_node1_dyn(self, move || match w {
            Window::Time(d) => boxed(Windowed::new(MomentsTimeWindow::new(d), Skewness)),
            Window::Count(n) => boxed(Windowed::new(MomentsCountWindow::new(n), Skewness)),
        })
    }

    fn kurtosis(&self, window: impl Into<Window>) -> Comp<R, f64> {
        let w: Window = window.into();
        Comp::custom_node1_dyn(self, move || match w {
            Window::Time(d) => boxed(Windowed::new(MomentsTimeWindow::new(d), Kurtosis)),
            Window::Count(n) => boxed(Windowed::new(MomentsCountWindow::new(n), Kurtosis)),
        })
    }

    fn correlation(&self, other: &Comp<R, f64>, window: impl Into<Window>) -> Comp<R, f64> {
        let w: Window = window.into();
        Comp::custom_node_dyn(self, &[other], move || match w {
            Window::Time(d) => boxed(BivariateWindowed::new(
                CorrelationTimeWindow::new(d),
                Correlation,
            )),
            Window::Count(n) => boxed(BivariateWindowed::new(
                CorrelationCountWindow::new(n),
                Correlation,
            )),
        })
    }

    fn covariance(&self, other: &Comp<R, f64>, window: impl Into<Window>) -> Comp<R, f64> {
        let w: Window = window.into();
        Comp::custom_node_dyn(self, &[other], move || match w {
            Window::Time(d) => boxed(BivariateWindowed::new(
                CorrelationTimeWindow::new(d),
                Covariance,
            )),
            Window::Count(n) => boxed(BivariateWindowed::new(
                CorrelationCountWindow::new(n),
                Covariance,
            )),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    /// Feed `values` through `op` via `eval`, collecting per-step outputs
    /// (absent results become `NaN`).
    fn drive(op: &mut dyn Operator, values: &[(i64, f64)]) -> Vec<f64> {
        values
            .iter()
            .map(|&(ts, v)| {
                op.eval(&[Ok(v)], ts)
                    .as_computed()
                    .unwrap()
                    .unwrap_or(f64::NAN)
            })
            .collect()
    }

    /// Assert two output sequences match, treating `NaN`/`NaN` as equal
    /// (warm-up steps yield `NaN`, which `assert_eq!` would reject).
    fn assert_series_eq(got: &[f64], want: &[f64]) {
        assert_eq!(
            got.len(),
            want.len(),
            "length mismatch: {got:?} vs {want:?}"
        );
        for (i, (&g, &w)) in got.iter().zip(want).enumerate() {
            assert!(
                (g.is_nan() && w.is_nan()) || (g - w).abs() < 1e-9,
                "step {i}: got {g}, want {w}"
            );
        }
    }

    #[test]
    fn ema_count_checkpoint_round_trip() {
        let series = [(1, 10.0), (2, 12.0), (3, 11.0), (4, 13.0), (5, 14.0)];

        let mut reference = Ema::Count(CountEma::new(3));
        let reference_out = drive(&mut reference, &series);

        let mut original = Ema::Count(CountEma::new(3));
        let first_half = drive(&mut original, &series[..2]);
        let bytes = original.save().expect("save should succeed");

        let mut restored = Ema::Count(CountEma::new(3));
        restored.load(&bytes).expect("load should succeed");
        let second_half = drive(&mut restored, &series[2..]);

        let resumed: Vec<f64> = first_half.into_iter().chain(second_half).collect();
        assert_series_eq(&resumed, &reference_out);
    }

    #[test]
    fn ema_time_checkpoint_round_trip() {
        let series = [
            (1000, 10.0),
            (2000, 12.0),
            (3000, 11.0),
            (4000, 13.0),
            (5000, 14.0),
        ];

        let mut reference = Ema::Time(TimeEma::new(Duration::from_secs(2)));
        let reference_out = drive(&mut reference, &series);

        let mut original = Ema::Time(TimeEma::new(Duration::from_secs(2)));
        let first_half = drive(&mut original, &series[..2]);
        let bytes = original.save().expect("save should succeed");

        let mut restored = Ema::Time(TimeEma::new(Duration::from_secs(2)));
        restored.load(&bytes).expect("load should succeed");
        let second_half = drive(&mut restored, &series[2..]);

        let resumed: Vec<f64> = first_half.into_iter().chain(second_half).collect();
        assert_series_eq(&resumed, &reference_out);
    }

    #[test]
    fn rsi_wilder_checkpoint_round_trip() {
        // A series long enough to cross out of the warm-up phase (period 3).
        let series = [
            (1, 44.0),
            (2, 44.5),
            (3, 43.75),
            (4, 44.25),
            (5, 45.0),
            (6, 44.0),
            (7, 46.0),
        ];

        let mut reference = RsiWilder::new(3);
        let reference_out = drive(&mut reference, &series);

        let mut original = RsiWilder::new(3);
        let first_half = drive(&mut original, &series[..4]);
        let bytes = original.save().expect("save should succeed");

        let mut restored = RsiWilder::new(3);
        restored.load(&bytes).expect("load should succeed");
        let second_half = drive(&mut restored, &series[4..]);

        let resumed: Vec<f64> = first_half.into_iter().chain(second_half).collect();
        assert_series_eq(&resumed, &reference_out);
    }
}
