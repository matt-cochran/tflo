//! Windowed-aggregation operators and the [`WindowOps`] extension trait.
//!
//! Most windowed operators are a [`Windowed<W, R>`] pairing a window primitive
//! with a [`Reduce`] unit: the unit's body just calls the matching window
//! accessor (`w.mean()`, `w.std()`, …). Three operators are written as plain
//! [`Operator`] structs instead: `Ema` and `RsiWilder` do not fit the
//! reduce-the-buffer shape (they keep a recursively smoothed scalar, not a
//! sliding buffer); and `QuantileOp` (in `crate::ops::stats`) carries a parameter
//! (`q`) that must be serialized, which the `#[serde(skip)]`-ped `Windowed`
//! reduction slot cannot do.
//!
//! Every method is exposed on `Comp<R, f64>` through the single [`WindowOps`]
//! extension trait so call sites read naturally — e.g. `price.sma(20)`.

use crate::ops::stats::{Correlation, Covariance, Kurtosis, Median, QuantileOp, Rank, Skewness};
use crate::primitives::{
    CorrelationCountWindow, CorrelationTimeWindow, CountEma, CountWindow, MedianCountWindow,
    MedianTimeWindow, MomentsCountWindow, MomentsTimeWindow, RsiCountWindow, RsiTimeWindow,
    TimeEma, TimeWindow, WmaCountWindow, WmaTimeWindow,
};
use crate::shapes::{BivariateWindowed, Reduce, Windowed};
use tflo_core::comp::Comp;
use tflo_core::operator::{BoxedOperator, Operator};
use tflo_core::window::Window;

mod ema;
mod rsi_wilder;

use ema::Ema;
use rsi_wilder::RsiWilder;

/// Fail-fast helper for sliding-aggregation operators that receive an
/// emit-trigger window variant ([`Window::Session`] or [`Window::Tumbling`]).
///
/// The two window categories are semantically distinct: sliding
/// aggregations (`sma`, `ema`, …) emit on every record over the values
/// currently inside the window; emit-trigger windows (`Session`,
/// `Tumbling`) emit only on a gap or bucket-edge fire. Crossing the
/// boundary silently would either produce wrong outputs or starve the
/// stream. Panicking with a clear message at construction time is the
/// fail-fast choice; the corresponding `session_*` / `tumbling_*`
/// operators (Phase 2 of the closure plan) accept the emit-trigger
/// variants.
#[allow(
    clippy::panic,
    reason = "builder-time validation: a misuse means the graph wiring is wrong, \
              not that a runtime input is bad. Fail-fast is correct here."
)]
fn panic_emit_trigger_on_sliding(operator: &'static str) -> ! {
    panic!(
        "sliding-aggregation operator `{operator}` does not accept \
         emit-trigger `Window::Session` or `Window::Tumbling` variants. \
         Use the matching `session_*` / `tumbling_*` operator instead."
    )
}

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
const fn count_as_f64(n: usize) -> f64 {
    n as f64
}

// ============================================================================
// Hand-written operators
// ============================================================================

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
    fn sma(&self, window: impl Into<Window>) -> Self {
        let w: Window = window.into();
        Self::custom_node1_dyn(self, move || match w {
            Window::Time(d) => boxed(Windowed::new(TimeWindow::new(d), Mean)),
            Window::Count(n) => boxed(Windowed::new(CountWindow::new(n), Mean)),
            Window::Session { .. } | Window::Tumbling { .. } => panic_emit_trigger_on_sliding("sliding-aggregation"),
        })
    }

    fn ema(&self, window: impl Into<Window>) -> Self {
        let w: Window = window.into();
        Self::custom_node1_dyn(self, move || match w {
            Window::Time(d) => boxed(Ema::Time(TimeEma::new(d))),
            Window::Count(n) => boxed(Ema::Count(CountEma::new(n))),
            Window::Session { .. } | Window::Tumbling { .. } => panic_emit_trigger_on_sliding("sliding-aggregation"),
        })
    }

    fn std(&self, window: impl Into<Window>) -> Self {
        let w: Window = window.into();
        Self::custom_node1_dyn(self, move || match w {
            Window::Time(d) => boxed(Windowed::new(TimeWindow::new(d), Std)),
            Window::Count(n) => boxed(Windowed::new(CountWindow::new(n), Std)),
            Window::Session { .. } | Window::Tumbling { .. } => panic_emit_trigger_on_sliding("sliding-aggregation"),
        })
    }

    fn variance(&self, window: impl Into<Window>) -> Self {
        let w: Window = window.into();
        Self::custom_node1_dyn(self, move || match w {
            Window::Time(d) => boxed(Windowed::new(TimeWindow::new(d), Variance)),
            Window::Count(n) => boxed(Windowed::new(CountWindow::new(n), Variance)),
            Window::Session { .. } | Window::Tumbling { .. } => panic_emit_trigger_on_sliding("sliding-aggregation"),
        })
    }

    fn max(&self, window: impl Into<Window>) -> Self {
        let w: Window = window.into();
        Self::custom_node1_dyn(self, move || match w {
            Window::Time(d) => boxed(Windowed::new(TimeWindow::new(d), Max)),
            Window::Count(n) => boxed(Windowed::new(CountWindow::new(n), Max)),
            Window::Session { .. } | Window::Tumbling { .. } => panic_emit_trigger_on_sliding("sliding-aggregation"),
        })
    }

    fn min(&self, window: impl Into<Window>) -> Self {
        let w: Window = window.into();
        Self::custom_node1_dyn(self, move || match w {
            Window::Time(d) => boxed(Windowed::new(TimeWindow::new(d), Min)),
            Window::Count(n) => boxed(Windowed::new(CountWindow::new(n), Min)),
            Window::Session { .. } | Window::Tumbling { .. } => panic_emit_trigger_on_sliding("sliding-aggregation"),
        })
    }

    fn sum(&self, window: impl Into<Window>) -> Self {
        let w: Window = window.into();
        Self::custom_node1_dyn(self, move || match w {
            Window::Time(d) => boxed(Windowed::new(TimeWindow::new(d), Sum)),
            Window::Count(n) => boxed(Windowed::new(CountWindow::new(n), Sum)),
            Window::Session { .. } | Window::Tumbling { .. } => panic_emit_trigger_on_sliding("sliding-aggregation"),
        })
    }

    fn count(&self, window: impl Into<Window>) -> Self {
        let w: Window = window.into();
        Self::custom_node1_dyn(self, move || match w {
            Window::Time(d) => boxed(Windowed::new(TimeWindow::new(d), Count)),
            Window::Count(n) => boxed(Windowed::new(CountWindow::new(n), Count)),
            Window::Session { .. } | Window::Tumbling { .. } => panic_emit_trigger_on_sliding("sliding-aggregation"),
        })
    }

    fn wma(&self, window: impl Into<Window>) -> Self {
        let w: Window = window.into();
        Self::custom_node1_dyn(self, move || match w {
            Window::Time(d) => boxed(Windowed::new(WmaTimeWindow::new(d), Wma)),
            Window::Count(n) => boxed(Windowed::new(WmaCountWindow::new(n), Wma)),
            Window::Session { .. } | Window::Tumbling { .. } => panic_emit_trigger_on_sliding("sliding-aggregation"),
        })
    }

    fn rsi(&self, window: impl Into<Window>) -> Self {
        let w: Window = window.into();
        Self::custom_node1_dyn(self, move || match w {
            Window::Time(d) => boxed(Windowed::new(RsiTimeWindow::new(d), Rsi)),
            Window::Count(n) => boxed(Windowed::new(RsiCountWindow::new(n), Rsi)),
            Window::Session { .. } | Window::Tumbling { .. } => panic_emit_trigger_on_sliding("sliding-aggregation"),
        })
    }

    fn rsi_wilder_n(&self, n: usize) -> Self {
        Self::custom_node1_dyn(self, move || boxed(RsiWilder::new(n)))
    }

    fn median(&self, window: impl Into<Window>) -> Self {
        let w: Window = window.into();
        Self::custom_node1_dyn(self, move || match w {
            Window::Time(d) => boxed(Windowed::new(MedianTimeWindow::new(d), Median)),
            Window::Count(n) => boxed(Windowed::new(MedianCountWindow::new(n), Median)),
            Window::Session { .. } | Window::Tumbling { .. } => panic_emit_trigger_on_sliding("sliding-aggregation"),
        })
    }

    fn quantile(&self, window: impl Into<Window>, q: f64) -> Self {
        let w: Window = window.into();
        Self::custom_node1_dyn(self, move || match w {
            Window::Time(d) => boxed(QuantileOp::Time {
                window: MedianTimeWindow::new(d),
                q,
            }),
            Window::Count(n) => boxed(QuantileOp::Count {
                window: MedianCountWindow::new(n),
                q,
            }),
            Window::Session { .. } | Window::Tumbling { .. } => panic_emit_trigger_on_sliding("sliding-aggregation"),
        })
    }

    fn rank(&self, window: impl Into<Window>) -> Self {
        let w: Window = window.into();
        Self::custom_node1_dyn(self, move || match w {
            Window::Time(d) => boxed(Windowed::new(MedianTimeWindow::new(d), Rank)),
            Window::Count(n) => boxed(Windowed::new(MedianCountWindow::new(n), Rank)),
            Window::Session { .. } | Window::Tumbling { .. } => panic_emit_trigger_on_sliding("sliding-aggregation"),
        })
    }

    fn skewness(&self, window: impl Into<Window>) -> Self {
        let w: Window = window.into();
        Self::custom_node1_dyn(self, move || match w {
            Window::Time(d) => boxed(Windowed::new(MomentsTimeWindow::new(d), Skewness)),
            Window::Count(n) => boxed(Windowed::new(MomentsCountWindow::new(n), Skewness)),
            Window::Session { .. } | Window::Tumbling { .. } => panic_emit_trigger_on_sliding("sliding-aggregation"),
        })
    }

    fn kurtosis(&self, window: impl Into<Window>) -> Self {
        let w: Window = window.into();
        Self::custom_node1_dyn(self, move || match w {
            Window::Time(d) => boxed(Windowed::new(MomentsTimeWindow::new(d), Kurtosis)),
            Window::Count(n) => boxed(Windowed::new(MomentsCountWindow::new(n), Kurtosis)),
            Window::Session { .. } | Window::Tumbling { .. } => panic_emit_trigger_on_sliding("sliding-aggregation"),
        })
    }

    fn correlation(&self, other: &Self, window: impl Into<Window>) -> Self {
        let w: Window = window.into();
        Self::custom_node_dyn(self, &[other], move || match w {
            Window::Time(d) => boxed(BivariateWindowed::new(
                CorrelationTimeWindow::new(d),
                Correlation,
            )),
            Window::Count(n) => boxed(BivariateWindowed::new(
                CorrelationCountWindow::new(n),
                Correlation,
            )),
            Window::Session { .. } | Window::Tumbling { .. } => panic_emit_trigger_on_sliding("sliding-aggregation"),
        })
    }

    fn covariance(&self, other: &Self, window: impl Into<Window>) -> Self {
        let w: Window = window.into();
        Self::custom_node_dyn(self, &[other], move || match w {
            Window::Time(d) => boxed(BivariateWindowed::new(
                CorrelationTimeWindow::new(d),
                Covariance,
            )),
            Window::Count(n) => boxed(BivariateWindowed::new(
                CorrelationCountWindow::new(n),
                Covariance,
            )),
            Window::Session { .. } | Window::Tumbling { .. } => panic_emit_trigger_on_sliding("sliding-aggregation"),
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
