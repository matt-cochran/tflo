//! The [`Windowed<W, R>`] generic operator shape.
//!
//! A [`Windowed`] pairs a [`WindowPrimitive`] `W` with a [`Reduce<W>`]
//! reduction `R`. It implements [`Operator`] once so that dozens of concrete
//! windowed operators (SMA, rolling std, rolling max, …) become thin
//! constructors over it.

use crate::checkpoint;
use crate::shapes::Reduce;
use serde::{Deserialize, Serialize};
use tflo_core::WindowPrimitive;
use tflo_core::compile::{Computed, NodeOutput, finite_or_warming};
use tflo_core::operator::{Operator, OperatorLoadError, require};

/// Generic windowed-reduction operator: push each input into a window
/// primitive `W`, then collapse the window with reduction `R`.
///
/// The reduction `R` is a zero-sized named unit struct implementing
/// [`Reduce<W>`]; it carries no state, so it is `#[serde(skip)]`-ped from the
/// checkpoint and restored via `R::default()` on deserialize. Only the window
/// `W` is serialized.
#[derive(Serialize, Deserialize)]
pub struct Windowed<W, R> {
    window: W,
    #[serde(skip)]
    reduce: R,
}

impl<W, R> Windowed<W, R> {
    /// Construct from a window primitive and a reduction.
    pub const fn new(window: W, reduce: R) -> Self {
        Self { window, reduce }
    }
}

impl<W, R> Operator for Windowed<W, R>
where
    W: WindowPrimitive + Serialize + serde::de::DeserializeOwned + Send + Sync + 'static,
    R: Reduce<W>,
{
    fn eval(&mut self, inputs: &[Computed], ts: i64) -> NodeOutput {
        let v = match require(inputs, 0) {
            Ok(v) => v,
            Err(e) => return NodeOutput::computed(Err(e)), // absent input: skip the push
        };
        self.window.push(ts, v);
        NodeOutput::computed(finite_or_warming(self.reduce.reduce(&self.window)))
    }

    fn name(&self) -> &str {
        "windowed"
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
    use crate::primitives::CountWindow;
    use tflo_core::operator::Operator;

    #[derive(Default)]
    struct TestMean;
    impl Reduce<CountWindow> for TestMean {
        fn reduce(&self, w: &CountWindow) -> f64 {
            w.mean()
        }
    }

    #[test]
    fn windowed_mean_over_count_window() {
        let mut op = Windowed::new(CountWindow::new(3), TestMean);
        // partial mean after one value is still a present (Ok) result:
        assert!(op.eval(&[Ok(10.0)], 0).as_computed().unwrap().is_ok());
        let _ = op.eval(&[Ok(20.0)], 0);
        let out = op.eval(&[Ok(30.0)], 0);
        assert_eq!(out.as_computed().unwrap(), Ok(20.0)); // mean(10,20,30)
    }

    #[test]
    fn windowed_checkpoint_round_trip() {
        let mut op = Windowed::new(CountWindow::new(3), TestMean);
        let _ = op.eval(&[Ok(10.0)], 0);
        let _ = op.eval(&[Ok(20.0)], 0);
        let bytes = op.save().expect("save should succeed");

        let mut restored = Windowed::new(CountWindow::new(3), TestMean);
        restored.load(&bytes).expect("load should succeed");

        // Both ops continue from the same window state: next value 30 gives
        // mean(10, 20, 30) == 20.0.
        let out = restored.eval(&[Ok(30.0)], 0);
        assert_eq!(out.as_computed().unwrap(), Ok(20.0));
    }
}
