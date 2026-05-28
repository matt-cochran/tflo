#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
//! Integration tests for `OutOfOrderPolicy::Buffer` keyed execution.
//!
//! Covers: in-order input is unaffected; out-of-order input within the
//! lateness window is reordered; records beyond the window are dropped; a
//! watermark jump releases several buffered records; end-of-stream flush
//! emits records still in the buffer; and per-key buffers are isolated.

use proptest::prelude::*;
use tflo_core::keyed::OutOfOrderPolicy;
use tflo_core::prelude::*;

#[derive(Clone, Debug)]
struct Ev {
    key: String,
    ts: i64,
}

fn ev(key: &str, ts: i64) -> Ev {
    Ev {
        key: key.to_string(),
        ts,
    }
}

/// Run a keyed pipeline whose single output is the record's own timestamp,
/// returning the released `(key, ts)` pairs in emission order.
fn run(data: Vec<Ev>, policy: OutOfOrderPolicy) -> Vec<(String, i64)> {
    data.into_iter()
        .tflo_keyed(
            |e: &Ev| e.key.clone(),
            policy,
            |t| {
                t.timestamp(|x: &Ev| x.ts);
                t.prop(|x: &Ev| x.ts as f64)
            },
        )
        .map(|r| r.expect("no policy error"))
        .map(|item| (item.ctx.key, item.ctx.ts))
        .collect()
}

fn timestamps(out: &[(String, i64)]) -> Vec<i64> {
    out.iter().map(|(_, t)| *t).collect()
}

#[test]
fn buffer_in_order_matches_error_policy() {
    let data = vec![ev("a", 10), ev("a", 20), ev("a", 30), ev("a", 40)];
    let error_out = run(data.clone(), OutOfOrderPolicy::Error);
    let buffer_out = run(
        data,
        OutOfOrderPolicy::Buffer {
            max_lateness_ms: 100,
        },
    );
    assert_eq!(
        error_out, buffer_out,
        "for already-ordered input, Buffer must emit exactly what Error does"
    );
}

#[test]
fn buffer_reorders_within_lateness_window() {
    // 30 then 20 arrives late, but inside the lateness window.
    let data = vec![ev("a", 10), ev("a", 30), ev("a", 20), ev("a", 40)];
    let out = run(
        data,
        OutOfOrderPolicy::Buffer {
            max_lateness_ms: 25,
        },
    );
    assert_eq!(timestamps(&out), vec![10, 20, 30, 40]);
}

#[test]
fn buffer_drops_records_behind_the_released_frontier() {
    // After 10 and 50 are released, a record at ts=5 is hopelessly late.
    let data = vec![ev("a", 10), ev("a", 100), ev("a", 50), ev("a", 5)];
    let out = run(
        data,
        OutOfOrderPolicy::Buffer {
            max_lateness_ms: 30,
        },
    );
    assert_eq!(
        timestamps(&out),
        vec![10, 50, 100],
        "the ts=5 record arrived behind the frontier and must be dropped"
    );
}

#[test]
fn buffer_releases_several_records_on_a_watermark_jump() {
    // 10/11/12 stay buffered; the jump to 1000 advances the watermark past
    // all three, releasing them together in a single step.
    let data = vec![ev("a", 10), ev("a", 11), ev("a", 12), ev("a", 1000)];
    let out = run(data, OutOfOrderPolicy::Buffer { max_lateness_ms: 5 });
    assert_eq!(timestamps(&out), vec![10, 11, 12, 1000]);
}

#[test]
fn buffer_flushes_remaining_records_at_end_of_stream() {
    // With a huge lateness window nothing is released mid-stream — every
    // record must come out via the end-of-stream flush.
    let data = vec![ev("a", 10), ev("a", 20), ev("a", 30)];
    let out = run(
        data,
        OutOfOrderPolicy::Buffer {
            max_lateness_ms: 1_000_000,
        },
    );
    assert_eq!(
        timestamps(&out),
        vec![10, 20, 30],
        "without end-of-stream flush these records would be silently lost"
    );
}

#[test]
fn buffer_isolates_per_key_state() {
    let data = vec![
        ev("a", 10),
        ev("b", 100),
        ev("a", 30),
        ev("b", 50),
        ev("a", 20),
        ev("b", 90),
    ];
    let out = run(
        data,
        OutOfOrderPolicy::Buffer {
            max_lateness_ms: 1_000_000,
        },
    );

    let key_ts = |k: &str| -> Vec<i64> {
        out.iter()
            .filter(|(key, _)| key == k)
            .map(|(_, t)| *t)
            .collect()
    };
    assert_eq!(key_ts("a"), vec![10, 20, 30]);
    assert_eq!(key_ts("b"), vec![50, 90, 100]);
}

proptest! {
    /// With an effectively-unbounded lateness window, nothing is dropped and
    /// nothing is released until the end-of-stream flush — so a `Buffer`
    /// pipeline fully sorts any permutation of its input timestamps.
    #[test]
    fn buffer_with_unbounded_lateness_fully_sorts(
        raw in prop::collection::vec(0i64..10_000, 1..40)
    ) {
        let data: Vec<Ev> = raw.iter().map(|&t| ev("k", t)).collect();
        let out = run(
            data,
            // SAFETY: compile-time constant division; no runtime panic.
            #[allow(clippy::integer_division)]
            OutOfOrderPolicy::Buffer { max_lateness_ms: i64::MAX / 2 },
        );
        let mut expected = raw.clone();
        expected.sort_unstable();
        prop_assert_eq!(timestamps(&out), expected);
    }
}
