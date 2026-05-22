//! The [`BivariateWindowed<W, R>`] generic operator shape.
//!
//! A [`BivariateWindowed`] pairs a two-input [`BivariateWindow`] `W` with a
//! [`Reduce<W>`] reduction `R`. It implements [`Operator`] once so that the
//! two-input windowed operators (rolling correlation, rolling covariance, …)
//! become thin constructors over it.

use crate::checkpoint;
use crate::shapes::Reduce;
use serde::{Deserialize, Serialize};
use tflo_core::BivariateWindow;
use tflo_core::compile::{Computed, NodeOutput, finite_or_warming};
use tflo_core::operator::{Operator, OperatorLoadError, require};

/// Generic two-input windowed-reduction operator: push each input pair into a
/// bivariate window primitive `W`, then collapse the window with reduction `R`.
///
/// The reduction `R` is a zero-sized named unit struct implementing
/// [`Reduce<W>`]; it carries no state, so it is `#[serde(skip)]`-ped from the
/// checkpoint and restored via `R::default()` on deserialize. Only the window
/// `W` is serialized.
#[derive(Serialize, Deserialize)]
pub struct BivariateWindowed<W, R> {
    window: W,
    #[serde(skip)]
    reduce: R,
}

impl<W, R> BivariateWindowed<W, R> {
    /// Construct from a bivariate window primitive and a reduction.
    pub fn new(window: W, reduce: R) -> Self {
        Self { window, reduce }
    }
}

impl<W, R> Operator for BivariateWindowed<W, R>
where
    W: BivariateWindow + Serialize + serde::de::DeserializeOwned + Send + Sync + 'static,
    R: Reduce<W>,
{
    fn eval(&mut self, inputs: &[Computed], ts: i64) -> NodeOutput {
        let a = match require(inputs, 0) {
            Ok(v) => v,
            Err(e) => return NodeOutput::computed(Err(e)), // absent input: skip the push
        };
        let b = match require(inputs, 1) {
            Ok(v) => v,
            Err(e) => return NodeOutput::computed(Err(e)), // absent input: skip the push
        };
        self.window.push(ts, a, b);
        NodeOutput::computed(finite_or_warming(self.reduce.reduce(&self.window)))
    }

    fn name(&self) -> &str {
        "bivariate_windowed"
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
    use crate::primitives::CorrelationCountWindow;
    use tflo_core::operator::Operator;

    #[derive(Default)]
    struct TestCovariance;
    impl Reduce<CorrelationCountWindow> for TestCovariance {
        fn reduce(&self, w: &CorrelationCountWindow) -> f64 {
            w.covariance()
        }
    }

    #[test]
    fn bivariate_covariance_over_count_window() {
        let mut op = BivariateWindowed::new(CorrelationCountWindow::new(4), TestCovariance);
        // First pair: only one observation, covariance is NaN -> warming up.
        assert!(
            op.eval(&[Ok(1.0), Ok(2.0)], 0)
                .as_computed()
                .unwrap()
                .is_err()
        );
        let _ = op.eval(&[Ok(2.0), Ok(4.0)], 0);
        let _ = op.eval(&[Ok(3.0), Ok(6.0)], 0);
        let out = op.eval(&[Ok(4.0), Ok(8.0)], 0);
        // A = [1,2,3,4], B = [2,4,6,8]: population covariance =
        //   sum_xy/n - mean_x*mean_y = 60/4 - 2.5*5.0 = 15.0 - 12.5 = 2.5
        assert_eq!(out.as_computed().unwrap(), Ok(2.5));
    }

    #[test]
    fn bivariate_absent_input_skips_push() {
        use tflo_core::compile::Absent;
        let mut op = BivariateWindowed::new(CorrelationCountWindow::new(4), TestCovariance);
        let _ = op.eval(&[Ok(1.0), Ok(2.0)], 0);
        let _ = op.eval(&[Ok(2.0), Ok(4.0)], 0);
        // An absent second input must not advance the window.
        let out = op.eval(&[Ok(3.0), Err(Absent::WarmingUp)], 0);
        assert!(out.as_computed().unwrap().is_err());
        // An absent first input must likewise not advance the window.
        let out = op.eval(&[Err(Absent::WarmingUp), Ok(6.0)], 0);
        assert!(out.as_computed().unwrap().is_err());
        // Continuing with two real pairs still yields the 4-point covariance.
        let _ = op.eval(&[Ok(3.0), Ok(6.0)], 0);
        let out = op.eval(&[Ok(4.0), Ok(8.0)], 0);
        assert_eq!(out.as_computed().unwrap(), Ok(2.5));
    }

    #[test]
    fn bivariate_checkpoint_round_trip() {
        let mut op = BivariateWindowed::new(CorrelationCountWindow::new(4), TestCovariance);
        let _ = op.eval(&[Ok(1.0), Ok(2.0)], 0);
        let _ = op.eval(&[Ok(2.0), Ok(4.0)], 0);
        let bytes = op.save().expect("save should succeed");

        let mut restored = BivariateWindowed::new(CorrelationCountWindow::new(4), TestCovariance);
        restored.load(&bytes).expect("load should succeed");

        // Both ops continue from the same window state: feeding the remaining
        // two pairs reproduces the 4-point covariance of 2.5.
        let _ = restored.eval(&[Ok(3.0), Ok(6.0)], 0);
        let out = restored.eval(&[Ok(4.0), Ok(8.0)], 0);
        assert_eq!(out.as_computed().unwrap(), Ok(2.5));
    }
}
