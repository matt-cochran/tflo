//! The [`StatefulTracker<S, Step>`] generic operator shape.
//!
//! A [`StatefulTracker`] pairs mutable tracker state `S` with a [`TrackStep<S>`]
//! step `Step`. It implements [`Operator`] once so that the single-state step
//! trackers (prev, lag, delta, cumulative sum, …) become thin constructors
//! over it.

use crate::checkpoint;
use crate::shapes::TrackStep;
use serde::{Deserialize, Serialize};
use tflo_core::compile::{Computed, NodeOutput};
use tflo_core::operator::{Operator, OperatorLoadError, require};

/// Generic single-state step tracker: fold each input value (and its
/// timestamp) into mutable state `S` with step `Step`.
///
/// The step `Step` is a zero-sized named unit struct implementing
/// [`TrackStep<S>`]; it carries no state, so it is `#[serde(skip)]`-ped from
/// the checkpoint and restored via `Step::default()` on deserialize. Only the
/// state `S` is serialized.
#[derive(Serialize, Deserialize)]
pub struct StatefulTracker<S, Step> {
    state: S,
    #[serde(skip)]
    step: Step,
}

impl<S, Step> StatefulTracker<S, Step> {
    /// Construct from initial tracker state and a step.
    pub const fn new(state: S, step: Step) -> Self {
        Self { state, step }
    }
}

impl<S, Step> Operator for StatefulTracker<S, Step>
where
    S: Serialize + serde::de::DeserializeOwned + Send + Sync + 'static,
    Step: TrackStep<S>,
{
    fn eval(&mut self, inputs: &[Computed], ts: i64) -> NodeOutput {
        let v = match require(inputs, 0) {
            Ok(v) => v,
            Err(e) => return NodeOutput::computed(Err(e)), // absent input: skip the step
        };
        NodeOutput::computed(self.step.step(&mut self.state, v, ts))
    }

    fn name(&self) -> &str {
        "stateful_tracker"
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
    use tflo_core::operator::Operator;

    #[derive(Default)]
    struct TestCumSum;
    impl TrackStep<f64> for TestCumSum {
        fn step(&self, state: &mut f64, value: f64, _ts: i64) -> Computed {
            *state += value;
            Ok(*state)
        }
    }

    #[test]
    fn tracker_cumulative_sum() {
        let mut op = StatefulTracker::new(0.0_f64, TestCumSum);
        assert_eq!(op.eval(&[Ok(10.0)], 0).as_computed().unwrap(), Ok(10.0));
        assert_eq!(op.eval(&[Ok(20.0)], 0).as_computed().unwrap(), Ok(30.0));
        assert_eq!(op.eval(&[Ok(30.0)], 0).as_computed().unwrap(), Ok(60.0));
    }

    #[test]
    fn tracker_absent_input_skips_step() {
        let mut op = StatefulTracker::new(0.0_f64, TestCumSum);
        let _ = op.eval(&[Ok(10.0)], 0);
        // An absent input must not advance the running total.
        let out = op.eval(&[Err(tflo_core::compile::Absent::WarmingUp)], 0);
        assert!(out.as_computed().unwrap().is_err());
        // The next real value continues from 10.0, not from a skipped state.
        assert_eq!(op.eval(&[Ok(5.0)], 0).as_computed().unwrap(), Ok(15.0));
    }

    #[test]
    fn tracker_checkpoint_round_trip() {
        let mut op = StatefulTracker::new(0.0_f64, TestCumSum);
        let _ = op.eval(&[Ok(10.0)], 0);
        let _ = op.eval(&[Ok(20.0)], 0);
        let bytes = op.save().expect("save should succeed");

        let mut restored = StatefulTracker::new(0.0_f64, TestCumSum);
        restored.load(&bytes).expect("load should succeed");

        // The restored tracker continues from a running total of 30.0.
        assert_eq!(
            restored.eval(&[Ok(30.0)], 0).as_computed().unwrap(),
            Ok(60.0)
        );
    }
}
