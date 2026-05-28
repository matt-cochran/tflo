#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::wildcard_enum_match_arm
)]
//! End-to-end tests for the Phase 1 event-time timer service.
//!
//! Covers:
//! - Registered timers fire on record-driven watermark advance.
//! - Registered timers fire on idle (`advance_event_time_watermark`) advance.
//! - Multiple due timers fire in `(fire_ts, registration_seq)` order on
//!   `flush`.
//! - `advance_event_time_watermark` rejects non-monotonic advances with
//!   `ComputeError::NonMonotonicWatermark`.
//! - Operators that delete a registered timer (cancel-before-fire) do not
//!   receive an `on_timer` callback for the cancelled entry.
//!
//! Each test uses a small `TimerProbeOp` that records every `on_timer`
//! callback in a shared trace, and emits the timer's `fire_ts` as the
//! graph's `Option<i64>` output. The probe lets the assertions inspect
//! exactly which timers fired and in what order.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::{Arc, Mutex};

use tflo_core::compile::{Computed, NodeOutput};
use tflo_core::error::ComputeError;
use tflo_core::keyed::OutOfOrderPolicy;
use tflo_core::operator::{BoxedOperator, Operator};
use tflo_core::prelude::*;
use tflo_core::timer::{EventTimeMs, TimerCtx};

#[derive(Clone, Debug)]
struct Rec {
    ts: i64,
    value: f64,
    key: &'static str,
}

const fn rec(ts: i64, value: f64, key: &'static str) -> Rec {
    Rec { ts, value, key }
}

/// Operator that registers a single absolute-event-time timer on its
/// first `eval_with_ctx` call, optionally deletes it on a later call
/// (when the input is `Ok(0.0)`), and emits `Some(fire_ts)` from
/// `on_timer`. Used by the tests to inspect the dispatch path.
struct TimerProbeOp {
    registered: bool,
    fire_ts: i64,
    trace: Arc<Mutex<Vec<i64>>>,
}

impl Operator for TimerProbeOp {
    fn eval(&mut self, _inputs: &[Computed], _ts: i64) -> NodeOutput {
        NodeOutput::other::<Option<i64>>(None)
    }

    fn eval_with_ctx(
        &mut self,
        inputs: &[Computed],
        _ts: i64,
        ctx: &mut TimerCtx<'_>,
    ) -> NodeOutput {
        if !self.registered {
            ctx.register_event_time_timer(self.fire_ts);
            self.registered = true;
        } else if matches!(inputs.first(), Some(Ok(v)) if *v == 0.0) {
            // Marker value 0.0 → cancel the pending timer.
            ctx.delete_event_time_timer(self.fire_ts);
        }
        NodeOutput::other::<Option<i64>>(None)
    }

    fn on_timer(&mut self, fire_ts: i64, _ctx: &mut TimerCtx<'_>) -> NodeOutput {
        self.trace.lock().expect("trace lock").push(fire_ts);
        NodeOutput::other::<Option<i64>>(Some(fire_ts))
    }

    fn name(&self) -> &str {
        "timer_probe"
    }
}

/// Build a keyed iterator over `Rec` whose plugin node is a `TimerProbeOp`
/// configured to register one timer at `fire_ts`. Returns the iterator
/// and the shared trace.
fn build_probe_iter<I: Iterator<Item = Rec> + 'static>(
    records: I,
    fire_ts: i64,
    policy: OutOfOrderPolicy,
) -> (
    tflo_core::keyed::TFloKeyedIter<I, Rec, Option<i64>, &'static str, Comp<Rec, Option<i64>>>,
    Arc<Mutex<Vec<i64>>>,
) {
    let trace: Arc<Mutex<Vec<i64>>> = Arc::new(Mutex::new(Vec::new()));
    let trace_for_builder = Arc::clone(&trace);
    let iter = records.tflo_keyed(
        |r: &Rec| r.key,
        policy,
        move |t: &mut TFlowBuilder<Rec>| -> Comp<Rec, Option<i64>> {
            t.timestamp(|r: &Rec| r.ts);
            let value = t.prop(|r: &Rec| r.value);
            let trace = Arc::clone(&trace_for_builder);
            value.custom_node1_dyn(move || {
                let trace = Arc::clone(&trace);
                let op: BoxedOperator = Box::new(TimerProbeOp {
                    registered: false,
                    fire_ts,
                    trace,
                });
                op
            })
        },
    );
    (iter, trace)
}

#[test]
fn timer_fires_on_record_driven_watermark_advance() {
    let data = vec![
        rec(10, 1.0, "k"),
        rec(100, 2.0, "k"), // ts past fire_ts=50 → timer must fire before this step
    ];
    let (iter, trace) = build_probe_iter(data.into_iter(), 50, OutOfOrderPolicy::Error);
    let items: Vec<_> = iter.collect::<Result<Vec<_>, _>>().expect("no errors");

    assert_eq!(
        *trace.lock().unwrap(),
        vec![50_i64],
        "on_timer must be called once at fire_ts=50"
    );
    // Items: (step at ts=10, no timer fires) + (timer fire at fire_ts=50 + step at ts=100)
    // Step at ts=10: returns None (timer not yet fired); the graph still
    //   emits a PipelineItem with value=None.
    // Timer fire at fire_ts=50: emits Some(50).
    // Step at ts=100: emits None (the eval doesn't fire the timer).
    let values: Vec<_> = items.iter().map(|i| i.value).collect();
    assert!(
        values.contains(&Some(50_i64)),
        "values should include the timer-fired Some(50): got {values:?}",
    );
}

#[test]
fn timer_fires_on_idle_advance_event_time_watermark() {
    // Single record registers the timer; no further records.
    let (mut iter, trace) = build_probe_iter(
        vec![rec(10, 1.0, "k")].into_iter(),
        50,
        OutOfOrderPolicy::Error,
    );
    // Consume the first record to register the timer.
    let first = iter.next().expect("first item").expect("ok");
    assert_eq!(first.ctx.timestamp(), 10);
    assert_eq!(first.value, None);

    // No more records — manually advance the watermark.
    iter.advance_event_time_watermark(EventTimeMs::new(60))
        .expect("advance ok");
    assert_eq!(*trace.lock().unwrap(), vec![50_i64]);

    let next = iter.next().expect("timer-fired item").expect("ok");
    assert_eq!(next.ctx.timestamp(), 50, "timer fire carries fire_ts as ctx");
    assert_eq!(next.value, Some(50));
}

#[test]
fn advance_event_time_watermark_rejects_backward_moves() {
    // Two records seed last_ts at 200; the timer's fire_ts (500) stays
    // pending. Then we try to advance the watermark backward to 50 — that
    // must fail-fast with NonMonotonicWatermark { last: 200, attempted: 50 }.
    let (mut iter, _trace) = build_probe_iter(
        vec![rec(100, 1.0, "k"), rec(200, 1.0, "k")].into_iter(),
        500,
        OutOfOrderPolicy::Error,
    );
    let _ = iter.next().expect("first item").expect("ok");
    let _ = iter.next().expect("second item").expect("ok");

    let err = iter
        .advance_event_time_watermark(EventTimeMs::new(50))
        .expect_err("backward advance must fail-fast");
    match err {
        ComputeError::NonMonotonicWatermark { last, attempted } => {
            assert_eq!(last, 200);
            assert_eq!(attempted, 50);
        }
        other => panic!("expected NonMonotonicWatermark, got {other:?}"),
    }
}

#[test]
fn delete_event_time_timer_cancels_pending_fire() {
    let data = vec![
        rec(10, 1.0, "k"), // registers the timer at fire_ts=50
        rec(20, 0.0, "k"), // marker → eval calls delete_event_time_timer
        rec(100, 5.0, "k"), // watermark advances past fire_ts=50; no fire
    ];
    let (iter, trace) = build_probe_iter(data.into_iter(), 50, OutOfOrderPolicy::Error);
    let _items: Vec<_> = iter.collect::<Result<Vec<_>, _>>().expect("no errors");

    assert!(
        trace.lock().unwrap().is_empty(),
        "cancelled timer must not invoke on_timer; trace = {:?}",
        trace.lock().unwrap()
    );
}

#[test]
fn flush_fires_remaining_timers_in_fire_ts_order() {
    // Use a custom op that registers two timers at different fire_ts.
    let trace: Arc<Mutex<Vec<i64>>> = Arc::new(Mutex::new(Vec::new()));
    let trace_for_builder = Arc::clone(&trace);

    struct DualRegisterOp {
        registered: bool,
        trace: Arc<Mutex<Vec<i64>>>,
    }
    impl Operator for DualRegisterOp {
        fn eval(&mut self, _inputs: &[Computed], _ts: i64) -> NodeOutput {
            NodeOutput::other::<Option<i64>>(None)
        }
        fn eval_with_ctx(
            &mut self,
            _inputs: &[Computed],
            _ts: i64,
            ctx: &mut TimerCtx<'_>,
        ) -> NodeOutput {
            if !self.registered {
                ctx.register_event_time_timer(200);
                ctx.register_event_time_timer(100);
                self.registered = true;
            }
            NodeOutput::other::<Option<i64>>(None)
        }
        fn on_timer(&mut self, fire_ts: i64, _ctx: &mut TimerCtx<'_>) -> NodeOutput {
            self.trace.lock().unwrap().push(fire_ts);
            NodeOutput::other::<Option<i64>>(Some(fire_ts))
        }
        fn name(&self) -> &str {
            "dual_register"
        }
    }

    // One record at ts=10 registers both timers; the iterator's
    // end-of-stream flush should fire both in (fire_ts, seq) order.
    let data = vec![rec(10, 1.0, "k")];
    let iter = data.into_iter().tflo_keyed(
        |r: &Rec| r.key,
        OutOfOrderPolicy::Error,
        move |t: &mut TFlowBuilder<Rec>| -> Comp<Rec, Option<i64>> {
            t.timestamp(|r: &Rec| r.ts);
            let value = t.prop(|r: &Rec| r.value);
            let trace = Arc::clone(&trace_for_builder);
            value.custom_node1_dyn(move || {
                let trace = Arc::clone(&trace);
                let op: BoxedOperator = Box::new(DualRegisterOp {
                    registered: false,
                    trace,
                });
                op
            })
        },
    );
    let _items: Vec<_> = iter.collect::<Result<Vec<_>, _>>().expect("no errors");

    assert_eq!(
        *trace.lock().unwrap(),
        vec![100_i64, 200_i64],
        "timers fire in ascending fire_ts order"
    );
}
