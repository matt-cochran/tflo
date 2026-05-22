//! Statistical reductions: distribution shape and bivariate measures.
//!
//! These [`Reduce`] units are paired with the distribution window primitives
//! ([`MedianTimeWindow`]/[`MedianCountWindow`],
//! [`MomentsTimeWindow`]/[`MomentsCountWindow`]) and the bivariate windows
//! ([`CorrelationTimeWindow`]/[`CorrelationCountWindow`]) by the
//! [`WindowOps`](super::windows::WindowOps) extension trait in
//! [`super::windows`]. They are zero-sized and carry no state, mirroring the
//! basic reductions in [`super::windows`].
//!
//! The one parameterised operator — `quantile`, which carries the fraction
//! `q` — is **not** a `Reduce` unit: it is the hand-written [`QuantileOp`]
//! [`Operator`] so that `q` survives checkpoint `save`/`load` (see its docs).

use crate::checkpoint;
use crate::primitives::{
    CorrelationCountWindow, CorrelationTimeWindow, MedianCountWindow, MedianTimeWindow,
    MomentsCountWindow, MomentsTimeWindow,
};
use crate::shapes::Reduce;
use serde::{Deserialize, Serialize};
use tflo_core::compile::{Computed, NodeOutput, finite_or_warming};
use tflo_core::operator::{Operator, OperatorLoadError, require};

/// Rolling median (`q = 0.5` quantile) of a distribution window.
#[derive(Default)]
pub struct Median;

/// Rank of the most recent value within its distribution window
/// (0.0 = smallest, 1.0 = largest).
#[derive(Default)]
pub struct Rank;

/// Rolling skewness of a moments window.
#[derive(Default)]
pub struct Skewness;

/// Rolling excess kurtosis of a moments window.
#[derive(Default)]
pub struct Kurtosis;

/// Rolling Pearson correlation of a bivariate window.
#[derive(Default)]
pub struct Correlation;

/// Rolling population covariance of a bivariate window.
#[derive(Default)]
pub struct Covariance;

/// Generate the time- and count-window `Reduce` impls for one reduction.
///
/// Each statistical reduction has identical bodies across its time- and
/// count-window primitives, so the two impls are generated from one
/// `accessor` expression rather than hand-written twice.
macro_rules! impl_reduce {
    ($reduce:ty, $time:ty, $count:ty, $accessor:expr) => {
        impl Reduce<$time> for $reduce {
            fn reduce(&self, w: &$time) -> f64 {
                let f: fn(&Self, &$time) -> f64 = $accessor;
                f(self, w)
            }
        }
        impl Reduce<$count> for $reduce {
            fn reduce(&self, w: &$count) -> f64 {
                let f: fn(&Self, &$count) -> f64 = $accessor;
                f(self, w)
            }
        }
    };
}

impl_reduce!(Median, MedianTimeWindow, MedianCountWindow, |_, w| w
    .median());
impl_reduce!(Rank, MedianTimeWindow, MedianCountWindow, |_, w| w
    .current_rank());
impl_reduce!(Skewness, MomentsTimeWindow, MomentsCountWindow, |_, w| w
    .skewness());
impl_reduce!(Kurtosis, MomentsTimeWindow, MomentsCountWindow, |_, w| w
    .kurtosis());
impl_reduce!(
    Correlation,
    CorrelationTimeWindow,
    CorrelationCountWindow,
    |_, w| w.correlation()
);
impl_reduce!(
    Covariance,
    CorrelationTimeWindow,
    CorrelationCountWindow,
    |_, w| w.covariance()
);

// ============================================================================
// Hand-written quantile operator
// ============================================================================

/// Rolling quantile over a time- or count-based distribution window.
///
/// Unlike the other distribution reductions, the quantile carries a parameter
/// — the fraction `q` (0.0 = minimum, 1.0 = maximum, 0.5 = median). The generic
/// [`Windowed`](crate::shapes::Windowed) shape `#[serde(skip)]`s its reduction
/// (sound only for zero-sized reductions, which are restored via `Default`), so
/// a `Windowed`-backed quantile would silently reset `q` to `0.0` on checkpoint
/// restore. To keep `q` durable across `save`/`load`, the quantile is a
/// hand-written [`Operator`] that serializes **both** the window and `q`.
///
/// Mirrors the structure of [`Ema`](crate::ops::windows) — an enum over the
/// time- and count-window variants.
#[derive(Serialize, Deserialize)]
pub(crate) enum QuantileOp {
    /// Time-based quantile window plus its quantile fraction.
    Time {
        /// Rolling distribution window.
        window: MedianTimeWindow,
        /// Quantile fraction in `[0.0, 1.0]`.
        q: f64,
    },
    /// Count-based quantile window plus its quantile fraction.
    Count {
        /// Rolling distribution window.
        window: MedianCountWindow,
        /// Quantile fraction in `[0.0, 1.0]`.
        q: f64,
    },
}

impl Operator for QuantileOp {
    fn eval(&mut self, inputs: &[Computed], ts: i64) -> NodeOutput {
        let v = match require(inputs, 0) {
            Ok(v) => v,
            Err(e) => return NodeOutput::computed(Err(e)), // absent input: skip the push
        };
        let out = match self {
            Self::Time { window, q } => {
                window.push(ts, v);
                window.quantile(*q)
            }
            Self::Count { window, q } => {
                window.push(v);
                window.quantile(*q)
            }
        };
        NodeOutput::computed(finite_or_warming(out))
    }

    fn name(&self) -> &str {
        "quantile"
    }

    fn save(&self) -> Option<Vec<u8>> {
        checkpoint::save(self)
    }

    fn load(&mut self, bytes: &[u8]) -> Result<(), OperatorLoadError> {
        checkpoint::load(self, bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    /// Feed `values` through `op` via `eval`, collecting the per-step outputs
    /// (absent results become `NaN`).
    fn drive(op: &mut QuantileOp, values: &[(i64, f64)]) -> Vec<f64> {
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

    #[test]
    fn quantile_count_checkpoint_round_trip_preserves_q() {
        let series = [(1, 3.0), (2, 1.0), (3, 4.0), (4, 1.0), (5, 5.0), (6, 9.0)];

        // Uninterrupted reference run with q = 0.9.
        let mut reference = QuantileOp::Count {
            window: MedianCountWindow::new(5),
            q: 0.9,
        };
        let reference_out = drive(&mut reference, &series);

        // Run the first half, then checkpoint.
        let mut original = QuantileOp::Count {
            window: MedianCountWindow::new(5),
            q: 0.9,
        };
        let first_half = drive(&mut original, &series[..3]);
        let bytes = original.save().expect("save should succeed");

        // Restore into a FRESH op built with a DIFFERENT q (0.0 = minimum).
        // If `load` did not carry `q`, the resumed run would compute the
        // minimum instead of the 0.9 quantile.
        let mut restored = QuantileOp::Count {
            window: MedianCountWindow::new(5),
            q: 0.0,
        };
        restored.load(&bytes).expect("load should succeed");

        let second_half = drive(&mut restored, &series[3..]);

        let resumed: Vec<f64> = first_half.into_iter().chain(second_half).collect();
        assert_eq!(resumed, reference_out);

        // Concretely prove q == 0.9 survived: the last window is
        // sorted [1, 1, 4, 5, 9]; the 0.9 quantile is 5 + 0.6*(9-5) = 7.4,
        // whereas q = 0.0 would have yielded the minimum 1.0.
        assert!((resumed[5] - 7.4).abs() < 1e-9, "got {}", resumed[5]);
    }

    #[test]
    fn quantile_time_checkpoint_round_trip_preserves_q() {
        let series = [
            (1000, 3.0),
            (2000, 1.0),
            (3000, 4.0),
            (4000, 1.0),
            (5000, 5.0),
        ];

        let mut reference = QuantileOp::Time {
            window: MedianTimeWindow::new(Duration::from_secs(60)),
            q: 0.75,
        };
        let reference_out = drive(&mut reference, &series);

        let mut original = QuantileOp::Time {
            window: MedianTimeWindow::new(Duration::from_secs(60)),
            q: 0.75,
        };
        let first_half = drive(&mut original, &series[..2]);
        let bytes = original.save().expect("save should succeed");

        // Fresh op with a different q (0.0); load must overwrite it with 0.75.
        let mut restored = QuantileOp::Time {
            window: MedianTimeWindow::new(Duration::from_secs(60)),
            q: 0.0,
        };
        restored.load(&bytes).expect("load should succeed");
        let second_half = drive(&mut restored, &series[2..]);

        let resumed: Vec<f64> = first_half.into_iter().chain(second_half).collect();
        assert_eq!(resumed, reference_out);

        // Last window sorted [1, 1, 3, 4, 5]; 0.75 quantile is
        // 3 + 0.0*... at pos 0.75*4 = 3.0 -> index 3 = 4.0. q = 0.0 would
        // have given 1.0.
        assert!((resumed[4] - 4.0).abs() < 1e-9, "got {}", resumed[4]);
    }
}
