//! Emit-trigger window operators: `session_sum` and `tumbling_sum`.
//!
//! Both are `TimerAware` operators that emit a typed `Option<f64>`:
//!
//! - **Session**: registers a timer at `current_ts + gap` on every
//!   record; the timer is reset (deleted + re-registered) when a new
//!   record arrives within `gap`. On timer fire, emits the accumulated
//!   sum since the session opened and resets state.
//! - **Tumbling**: registers a timer at the *next* bucket-edge
//!   (`ceil(current_ts / size) * size`) on the first record per bucket;
//!   on fire, emits the accumulated bucket sum and re-registers for the
//!   next bucket edge.
//!
//! Each record's per-step output is `None` until the timer fires; the
//! actual aggregate value is emitted from `on_timer`. This matches the
//! Flink-style emit-trigger semantics described in
//! `docs/non-goals.md` (recognition vs. transformation distinction).

use crate::checkpoint;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tflo_core::comp::Comp;
use tflo_core::compile::{Computed, NodeOutput};
use tflo_core::operator::{BoxedOperator, Operator, OperatorLoadError, require};
use tflo_core::timer::TimerCtx;
use tflo_core::window::Window;

/// Session-window sum operator.
///
/// Sums every value within a session; emits the running sum and
/// resets when the inactivity gap elapses.
#[derive(Serialize, Deserialize)]
pub(crate) struct SessionSumOp {
    gap_ms: i64,
    /// Currently-accumulated sum, since the session opened.
    acc: f64,
    /// Whether we currently have an open session.
    open: bool,
    /// The timer `fire_ts` we've registered with the engine; used to
    /// delete the previous registration when a new record arrives and
    /// to recognize stale fires after flush.
    pending_timer_fire_ts: Option<i64>,
}

impl SessionSumOp {
    pub(crate) const fn new(gap_ms: i64) -> Self {
        Self {
            gap_ms,
            acc: 0.0,
            open: false,
            pending_timer_fire_ts: None,
        }
    }
}

impl Operator for SessionSumOp {
    fn eval(&mut self, _inputs: &[Computed], _ts: i64) -> NodeOutput {
        // Without a TimerCtx we can't drive emit-trigger semantics; only
        // accumulate. The keyed-execution path always calls `eval_with_ctx`,
        // so this fallback is reached only by non-keyed callers (which
        // should not use session aggregators).
        NodeOutput::other::<Option<f64>>(None)
    }

    fn eval_with_ctx(
        &mut self,
        inputs: &[Computed],
        ts: i64,
        ctx: &mut TimerCtx<'_>,
    ) -> NodeOutput {
        let Ok(value) = require(inputs, 0) else {
            // Absent input: do not advance accumulator or timers; emit nothing.
            return NodeOutput::other::<Option<f64>>(None);
        };
        // Update accumulator.
        if !self.open {
            self.acc = 0.0;
            self.open = true;
        }
        self.acc += value;

        // (Re)schedule the close timer. Delete the previous registration
        // (if any) to avoid duplicate fires.
        if let Some(prev) = self.pending_timer_fire_ts.take() {
            ctx.delete_event_time_timer(prev);
        }
        let fire_ts = ts.saturating_add(self.gap_ms);
        ctx.register_event_time_timer(fire_ts);
        self.pending_timer_fire_ts = Some(fire_ts);

        NodeOutput::other::<Option<f64>>(None)
    }

    fn on_timer(&mut self, fire_ts: i64, _ctx: &mut TimerCtx<'_>) -> NodeOutput {
        // Only emit if this fire matches the currently-registered timer.
        // Older fires (after delete_event_time_timer + re-register) are
        // tombstoned in the engine's heap, but `flush` re-registers and
        // fires every entry; guard against stale fires here.
        if self.pending_timer_fire_ts == Some(fire_ts) && self.open {
            let emit = self.acc;
            self.acc = 0.0;
            self.open = false;
            self.pending_timer_fire_ts = None;
            NodeOutput::other::<Option<f64>>(Some(emit))
        } else {
            NodeOutput::other::<Option<f64>>(None)
        }
    }

    fn name(&self) -> &str {
        "session_sum"
    }

    fn save(&self) -> Option<Vec<u8>> {
        checkpoint::save(self)
    }

    fn load(&mut self, bytes: &[u8]) -> Result<(), OperatorLoadError> {
        checkpoint::load(self, bytes)
    }
}

/// Tumbling-window sum operator.
///
/// Sums every value within a fixed-width bucket; emits the bucket sum on
/// the bucket-edge timer and resets the accumulator.
#[derive(Serialize, Deserialize)]
pub(crate) struct TumblingSumOp {
    bucket_size_ms: i64,
    /// Currently-accumulated sum for the in-progress bucket.
    acc: f64,
    /// True once at least one record has been admitted; gates `on_timer`
    /// emissions so an empty bucket (no records, only a timer fire on
    /// flush) does not emit a spurious `Some(0.0)`.
    open: bool,
    /// The timer `fire_ts` for the in-progress bucket edge.
    pending_timer_fire_ts: Option<i64>,
}

impl TumblingSumOp {
    pub(crate) const fn new(bucket_size_ms: i64) -> Self {
        Self {
            bucket_size_ms,
            acc: 0.0,
            open: false,
            pending_timer_fire_ts: None,
        }
    }

    /// Compute the next bucket edge `> ts` for the given bucket size.
    /// Buckets are aligned to multiples of `size` (epoch-origin); a record
    /// at `ts` belongs to the bucket ending at `ceil((ts + 1) / size) * size`.
    #[allow(
        clippy::arithmetic_side_effects,
        reason = "saturating_div/mul are total over i64; size==0 collapses to i64::MAX. \
                  Clippy flags the `size` argument as arithmetic-bearing even though \
                  the method contract handles it."
    )]
    const fn next_bucket_edge(ts: i64, size: i64) -> i64 {
        // Edge cases at i64::MIN/MAX collapse to the bound;
        // a 64-bit-second budget is geological scale.
        let next_bucket = ts.saturating_div(size).saturating_add(1);
        next_bucket.saturating_mul(size)
    }
}

impl Operator for TumblingSumOp {
    fn eval(&mut self, _inputs: &[Computed], _ts: i64) -> NodeOutput {
        NodeOutput::other::<Option<f64>>(None)
    }

    fn eval_with_ctx(
        &mut self,
        inputs: &[Computed],
        ts: i64,
        ctx: &mut TimerCtx<'_>,
    ) -> NodeOutput {
        let Ok(value) = require(inputs, 0) else {
            return NodeOutput::other::<Option<f64>>(None);
        };
        self.acc += value;
        if !self.open {
            self.open = true;
            // Register the bucket-edge timer (first record of the bucket).
            let fire_ts = Self::next_bucket_edge(ts, self.bucket_size_ms);
            ctx.register_event_time_timer(fire_ts);
            self.pending_timer_fire_ts = Some(fire_ts);
        }
        NodeOutput::other::<Option<f64>>(None)
    }

    fn on_timer(&mut self, fire_ts: i64, _ctx: &mut TimerCtx<'_>) -> NodeOutput {
        if self.pending_timer_fire_ts == Some(fire_ts) && self.open {
            let emit = self.acc;
            self.acc = 0.0;
            self.open = false;
            self.pending_timer_fire_ts = None;
            NodeOutput::other::<Option<f64>>(Some(emit))
        } else {
            NodeOutput::other::<Option<f64>>(None)
        }
    }

    fn name(&self) -> &str {
        "tumbling_sum"
    }

    fn save(&self) -> Option<Vec<u8>> {
        checkpoint::save(self)
    }

    fn load(&mut self, bytes: &[u8]) -> Result<(), OperatorLoadError> {
        checkpoint::load(self, bytes)
    }
}

/// Extension trait on `Comp<R, f64>` exposing the emit-trigger window
/// operators (`session_sum`, `tumbling_sum`).
pub trait EmitWindowOps<R> {
    /// Emit the running sum once on the inactivity gap defined by the
    /// [`Window::Session`] variant. Panics if `window` is not a
    /// `Window::Session`.
    fn session_sum(&self, window: Window) -> Comp<R, Option<f64>>;
    /// Emit the bucket sum on every bucket edge defined by the
    /// [`Window::Tumbling`] variant. Panics if `window` is not a
    /// `Window::Tumbling`.
    fn tumbling_sum(&self, window: Window) -> Comp<R, Option<f64>>;
}

impl<R: 'static> EmitWindowOps<R> for Comp<R, f64> {
    #[allow(
        clippy::panic,
        clippy::wildcard_enum_match_arm,
        reason = "builder-time validation; mirrors `panic_emit_trigger_on_sliding` \
                  in windows/mod.rs. Misuse means graph wiring is wrong."
    )]
    fn session_sum(&self, window: Window) -> Comp<R, Option<f64>> {
        let gap_ms = match window {
            Window::Session { gap } => duration_to_ms(gap),
            other => panic!(
                "session_sum requires Window::Session; got {other:?}. \
                 Use the matching sliding aggregator for Time/Count \
                 windows, or `tumbling_sum` for Tumbling windows."
            ),
        };
        Self::custom_node1_dyn(self, move || {
            let op: BoxedOperator = Box::new(SessionSumOp::new(gap_ms));
            op
        })
    }

    #[allow(
        clippy::panic,
        clippy::wildcard_enum_match_arm,
        reason = "builder-time validation; mirrors `panic_emit_trigger_on_sliding`."
    )]
    fn tumbling_sum(&self, window: Window) -> Comp<R, Option<f64>> {
        let size_ms = match window {
            Window::Tumbling { size } => duration_to_ms(size),
            other => panic!(
                "tumbling_sum requires Window::Tumbling; got {other:?}. \
                 Use the matching sliding aggregator for Time/Count \
                 windows, or `session_sum` for Session windows."
            ),
        };
        Self::custom_node1_dyn(self, move || {
            let op: BoxedOperator = Box::new(TumblingSumOp::new(size_ms));
            op
        })
    }
}

/// Convert a `Duration` to milliseconds, saturating at `i64::MAX` for
/// pathological inputs (which a temporal stream should never reach in
/// realistic deployments).
#[allow(clippy::cast_possible_wrap)] // saturating bounds keep us inside i64
fn duration_to_ms(d: Duration) -> i64 {
    let ms = d.as_millis();
    i64::try_from(ms).unwrap_or(i64::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tflo_core::keyed::KeyedGraphState;

    /// Build a single-key graph wrapping one `SessionSumOp` over a `prop`
    /// source. `gap_ms` configures the session gap.
    fn build_session_state(
        gap_ms: i64,
    ) -> KeyedGraphState<f64, Option<f64>, &'static str> {
        use tflo_core::builder::Compile;
        use tflo_core::comp::Comp;
        use tflo_core::prelude::*;
        let mut builder: TFlowBuilder<f64> = TFlowBuilder::new();
        builder.timestamp(|_v| 0_i64);
        let value = builder.prop(|v: &f64| *v);
        let comp: Comp<f64, Option<f64>> = value.custom_node1_dyn(move || {
            let op: BoxedOperator = Box::new(SessionSumOp::new(gap_ms));
            op
        });
        let output_ids = comp.output_ids();
        let timestamp_fn = builder.get_timestamp_fn().unwrap();
        let nodes = builder.into_nodes();
        let graph = tflo_core::compile::CompiledGraph::compile(timestamp_fn, nodes, output_ids);
        KeyedGraphState::new(graph, tflo_core::keyed::OutOfOrderPolicy::Error)
    }

    #[test]
    fn session_sum_emits_on_gap_close() {
        // gap=100ms: records at ts=0,50,90 form one session (sum=6.0)
        // closes at ts=190 (90+100); record at ts=200 (>= 190) opens a new
        // session (we drive the watermark via explicit advance after).
        let mut state = build_session_state(100);
        let _ = state.step(1.0, 0, "k").unwrap();
        let _ = state.step(2.0, 50, "k").unwrap();
        let _ = state.step(3.0, 90, "k").unwrap();

        // Advance the watermark past the close (90 + 100 = 190).
        let items = state
            .advance_event_time_watermark(
                tflo_core::timer::EventTimeMs::new(200),
                "k",
            )
            .unwrap();
        // Expect one item: Some(6.0) at fire_ts=190.
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].ctx.timestamp(), 190);
        assert_eq!(items[0].value, Some(6.0));
    }

    #[test]
    fn tumbling_sum_emits_on_bucket_edge() {
        use tflo_core::builder::Compile;
        use tflo_core::comp::Comp;
        use tflo_core::prelude::*;
        let mut builder: TFlowBuilder<f64> = TFlowBuilder::new();
        builder.timestamp(|_v| 0_i64);
        let value = builder.prop(|v: &f64| *v);
        let comp: Comp<f64, Option<f64>> = value.custom_node1_dyn(move || {
            let op: BoxedOperator = Box::new(TumblingSumOp::new(100));
            op
        });
        let output_ids = comp.output_ids();
        let timestamp_fn = builder.get_timestamp_fn().unwrap();
        let nodes = builder.into_nodes();
        let graph = tflo_core::compile::CompiledGraph::compile(timestamp_fn, nodes, output_ids);
        let mut state: KeyedGraphState<f64, Option<f64>, &'static str> =
            KeyedGraphState::new(graph, tflo_core::keyed::OutOfOrderPolicy::Error);

        // ts=10,30,60 → bucket [0,100), edge fires at fire_ts=100.
        let _ = state.step(1.0, 10, "k").unwrap();
        let _ = state.step(2.0, 30, "k").unwrap();
        let _ = state.step(3.0, 60, "k").unwrap();

        let items = state
            .advance_event_time_watermark(
                tflo_core::timer::EventTimeMs::new(150),
                "k",
            )
            .unwrap();
        // Expect one item: Some(6.0) at fire_ts=100.
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].ctx.timestamp(), 100);
        assert_eq!(items[0].value, Some(6.0));
    }

    #[test]
    fn next_bucket_edge_aligns_to_size() {
        assert_eq!(TumblingSumOp::next_bucket_edge(0, 100), 100);
        assert_eq!(TumblingSumOp::next_bucket_edge(50, 100), 100);
        assert_eq!(TumblingSumOp::next_bucket_edge(99, 100), 100);
        assert_eq!(TumblingSumOp::next_bucket_edge(100, 100), 200);
        assert_eq!(TumblingSumOp::next_bucket_edge(150, 100), 200);
    }
}
