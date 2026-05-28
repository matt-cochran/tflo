//! Pulse-width detector operator. Moved out of the parent module by
//! `StructureOS` `move` action; see `mod.rs` for the public extension
//! trait method ([`DetectorOps::pulse_width`](super::DetectorOps::pulse_width)).

use crate::checkpoint;
use crate::events::PulseWidthResult;
use crate::primitives::PulseWidthDetector;
use serde::{Deserialize, Serialize};
use tflo_core::compile::{Computed, NodeOutput};
use tflo_core::operator::{Operator, OperatorLoadError, require};
use tflo_core::timer::TimerCtx;

/// Pulse-width detector — `pulse_width`.
#[derive(Serialize, Deserialize)]
pub(crate) struct PulseWidthOp {
    detector: PulseWidthDetector,
    /// Event-time of the timer this op has registered with the engine for
    /// the *currently open* pulse, if any. `None` between pulses. Tracked
    /// here so the op can delete the timer via [`TimerCtx::delete_event_time_timer`]
    /// when the pulse closes cleanly, and so `on_timer` can recognize
    /// stale registrations (engine fires every entry; the op must guard
    /// against firing for a pulse that already closed).
    pending_timer_fire_ts: Option<i64>,
}

impl PulseWidthOp {
    pub(crate) const fn new(detector: PulseWidthDetector) -> Self {
        Self {
            detector,
            pending_timer_fire_ts: None,
        }
    }
}

impl Operator for PulseWidthOp {
    fn eval(&mut self, inputs: &[Computed], ts: i64) -> NodeOutput {
        // Absent input → no event this tick. Skip the detector update entirely
        // so the next present record sees the prior level, not a NaN-polluted
        // state.
        let Ok(value) = require(inputs, 0) else {
            return NodeOutput::other::<Option<PulseWidthResult>>(None);
        };
        let result: Option<PulseWidthResult> = self.detector.update(value, ts).map(to_pulse_width);
        NodeOutput::other(result)
    }

    /// Event-time-aware eval. Same data path as [`eval`](Self::eval), plus
    /// timer registration on the rising edge (the timer fires `TooLong` if
    /// no closing record arrives within `max_width_ms`) and timer deletion
    /// on the falling edge (the pulse closed cleanly; we cancel the
    /// fallback emission).
    fn eval_with_ctx(
        &mut self,
        inputs: &[Computed],
        ts: i64,
        ctx: &mut TimerCtx<'_>,
    ) -> NodeOutput {
        let was_in_pulse = self.detector.is_in_pulse();
        let Ok(value) = require(inputs, 0) else {
            return NodeOutput::other::<Option<PulseWidthResult>>(None);
        };
        let result_opt = self.detector.update(value, ts);
        let now_in_pulse = self.detector.is_in_pulse();
        match (was_in_pulse, now_in_pulse) {
            // Rising edge: register the TooLong timer.
            (false, true) => {
                // Fire timer at `ts + max_width_ms + 1` so a falling edge
                // *exactly* at `ts + max_width_ms` (which classifies as
                // `Valid`) still fires before the timer.
                let fire_ts = ts.saturating_add(self.detector.max_width_ms()).saturating_add(1);
                ctx.register_event_time_timer(fire_ts);
                self.pending_timer_fire_ts = Some(fire_ts);
            }
            // Falling edge: cancel the pending timer (pulse closed cleanly).
            (true, false) => {
                if let Some(fire_ts) = self.pending_timer_fire_ts.take() {
                    ctx.delete_event_time_timer(fire_ts);
                }
            }
            _ => {}
        }
        let result: Option<PulseWidthResult> = result_opt.map(to_pulse_width);
        NodeOutput::other(result)
    }

    /// Called when the absence-of-event timer registered on the rising
    /// edge fires. Emits `TooLong { width_ms: max_width_ms }` and forces
    /// the detector back to Low so the next record sees a clean slate.
    /// A no-op (`None`) if the pulse has already closed since registration
    /// (the timer was deleted but the engine may still fire stale entries
    /// from `flush`).
    fn on_timer(&mut self, fire_ts: i64, _ctx: &mut TimerCtx<'_>) -> NodeOutput {
        if self.pending_timer_fire_ts == Some(fire_ts) && self.detector.is_in_pulse() {
            let width_ms = self.detector.max_width_ms();
            self.detector.force_close();
            self.pending_timer_fire_ts = None;
            NodeOutput::other::<Option<PulseWidthResult>>(Some(PulseWidthResult::TooLong {
                width_ms,
            }))
        } else {
            NodeOutput::other::<Option<PulseWidthResult>>(None)
        }
    }

    fn name(&self) -> &str {
        "pulse_width"
    }

    fn save(&self) -> Option<Vec<u8>> {
        checkpoint::save(self)
    }

    fn load(&mut self, bytes: &[u8]) -> Result<(), OperatorLoadError> {
        checkpoint::load(self, bytes)
    }
}

/// Map the `tflo-core` primitive's `PulseWidthResult` to the `tflo-ops` copy.
pub(crate) const fn to_pulse_width(
    result: crate::primitives::PulseWidthResult,
) -> PulseWidthResult {
    use crate::primitives::PulseWidthResult as Core;
    match result {
        Core::TooShort { width_ms } => PulseWidthResult::TooShort { width_ms },
        Core::Valid { width_ms } => PulseWidthResult::Valid { width_ms },
        Core::TooLong { width_ms } => PulseWidthResult::TooLong { width_ms },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tflo_core::compile::Absent;

    /// Confirms the OPS-001 fix: an absent input emits `None` without feeding
    /// `NaN` to the pulse-width detector and corrupting its in-pulse state.
    #[test]
    fn pulse_width_skips_on_absent_input() {
        // Threshold 100, valid range 5..=15ms.
        let mut op = PulseWidthOp::new(PulseWidthDetector::new(100.0, 5, 15));

        // Pulse starts.
        let out0 = op.eval(&[Ok(110.0)], 0);
        assert_eq!(
            out0.as_any().downcast_ref::<Option<PulseWidthResult>>(),
            Some(&None),
        );

        // Absent mid-pulse → no event, detector state untouched.
        let out1 = op.eval(&[Err(Absent::WarmingUp)], 5);
        assert_eq!(
            out1.as_any().downcast_ref::<Option<PulseWidthResult>>(),
            Some(&None),
        );

        // Pulse ends at 10ms (Valid range 5..=15). Legacy NaN-substitution
        // would have killed the pulse on the absent tick.
        let out2 = op.eval(&[Ok(90.0)], 10);
        assert_eq!(
            out2.as_any()
                .downcast_ref::<Option<PulseWidthResult>>()
                .copied()
                .flatten(),
            Some(PulseWidthResult::Valid { width_ms: 10 }),
        );
    }
}
