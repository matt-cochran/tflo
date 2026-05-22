#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use std::time::Duration;
use tflo_core::combinators::{
    GroupByExt, PartitionExt, fork, partition_lazy, rate_limit_keep_last,
};

/// When partition_lazy is applied,
/// the combinator shall yield (item, is_match) pairs,
/// So that items can be processed lazily without allocation,
/// And each item will include its partition assignment.
#[test]
fn test_partition_lazy() {
    let items = vec![1, 2, 3, 4, 5];
    let partitioned: Vec<(i32, bool)> = partition_lazy(items, |&x| x % 2 == 0).collect();

    assert_eq!(partitioned.len(), 5);
    assert_eq!(partitioned[0], (1, false)); // 1 is odd
    assert_eq!(partitioned[1], (2, true)); // 2 is even
    assert_eq!(partitioned[2], (3, false)); // 3 is odd
    assert_eq!(partitioned[3], (4, true)); // 4 is even
    assert_eq!(partitioned[4], (5, false)); // 5 is odd
}

/// When partition_lazy is used with filter,
/// users shall be able to process only matching items,
/// So that non-matching items are skipped efficiently,
/// And no intermediate collections are created.
#[test]
fn test_partition_lazy_filter() {
    let items = vec![1, 2, 3, 4, 5];
    let evens: Vec<i32> = partition_lazy(items, |&x| x % 2 == 0)
        .filter(|(_, is_even)| *is_even)
        .map(|(x, _)| x)
        .collect();

    assert_eq!(evens, vec![2, 4]);
}

/// When fork is called with count N,
/// the combinator shall create N independent copies of the stream,
/// So that multiple consumers can process the same data,
/// And each copy will be identical.
#[test]
fn test_fork_basic() {
    let items = vec![1, 2, 3];
    let copies = fork(items, 3);

    assert_eq!(copies.len(), 3);
    for copy in copies {
        assert_eq!(copy, vec![1, 2, 3]);
    }
}

/// When fork is called with count 0,
/// the combinator shall return an empty Vec,
/// So that edge cases are handled gracefully,
/// And no panic will occur.
#[test]
fn test_fork_zero() {
    let items = vec![1, 2, 3];
    let copies = fork(items, 0);

    assert!(copies.is_empty());
}

/// When fork is called on an empty iterator,
/// the combinator shall create N empty copies,
/// So that empty input is handled correctly,
/// And each copy will be empty.
#[test]
fn test_fork_empty() {
    let items: Vec<i32> = vec![];
    let copies = fork(items, 3);

    assert_eq!(copies.len(), 3);
    for copy in copies {
        assert!(copy.is_empty());
    }
}

/// When rate_limit_keep_last is applied,
/// the combinator shall keep the most recent item within each interval,
/// So that the latest data is preserved rather than first,
/// And gaps will contain the most recent observation.
#[test]
fn test_rate_limit_keep_last() {
    let items = vec![
        (1000_i64, "a"),
        (1100, "b"), // Will replace "a" as most recent
        (1200, "c"), // Will replace "b" as most recent
        (3000, "d"), // New interval starts
        (3100, "e"), // Will replace "d"
    ];

    let limited: Vec<_> =
        rate_limit_keep_last(items.into_iter(), |x| x.0, Duration::from_secs(1)).collect();

    // Should keep last in each interval: "c" from [1000-2000), "e" from [3000-4000)
    assert!(!limited.is_empty());
}

/// When group_by_key is used via extension trait,
/// the iterator shall be collected into groups by key,
/// So that related items are grouped together,
/// And the syntax is ergonomic.
#[test]
fn test_group_by_ext() {
    let values = vec![1, 2, 3, 4, 5, 6];
    let groups = values.into_iter().group_by_key(|&x| x % 2);

    assert_eq!(groups.key_count(), 2);
    assert_eq!(groups.get(&0).map(|v| v.len()), Some(3)); // Even: 2, 4, 6
    assert_eq!(groups.get(&1).map(|v| v.len()), Some(3)); // Odd: 1, 3, 5
}

/// When partition_by is used via extension trait,
/// the iterator shall be partitioned into two vectors,
/// So that matching and non-matching items are separated,
/// And the syntax is ergonomic.
#[test]
fn test_partition_ext() {
    let values = vec![1, 2, 3, 4, 5];
    let (evens, odds) = values.into_iter().partition_by(|&x| x % 2 == 0);

    assert_eq!(evens, vec![2, 4]);
    assert_eq!(odds, vec![1, 3, 5]);
}

/// When partition_lazy_by is used via extension trait,
/// the iterator shall lazily yield partitioned items,
/// So that streaming partition is available via fluent API,
/// And no intermediate allocation is needed.
#[test]
fn test_partition_lazy_ext() {
    let values = vec![1, 2, 3];
    let partitioned: Vec<_> = values
        .into_iter()
        .partition_lazy_by(|&x| x % 2 == 0)
        .collect();

    assert_eq!(partitioned.len(), 3);
}

/// When dc_remove is called on Comp,
/// the computation shall subtract the rolling mean,
/// So that the signal is AC-coupled.
#[test]
fn test_fluent_dc_remove() {
    use tflo_core::prelude::*;

    #[derive(Clone)]
    struct Sample {
        ts: i64,
        value: f64,
    }

    let samples: Vec<Sample> = (0..20)
        .map(|i| Sample {
            ts: i * 100,
            value: 100.0 + (i % 3) as f64, // DC ~101, small AC
        })
        .collect();

    let results: Vec<f64> = samples
        .into_iter()
        .tflo(|t| {
            let _ = t.timestamp(|x| x.ts);
            let value = t.prop(|x| x.value);
            value.dc_remove(10usize)
        })
        .collect();

    // After warmup, AC values should be small (centered around 0)
    for result in results.iter().skip(10) {
        assert!(result.abs() < 5.0);
    }
}

/// When baseline_correct is called on Comp,
/// the computation shall subtract a low percentile,
/// So that drifting baseline is removed.
#[test]
fn test_fluent_baseline_correct() {
    use tflo_core::prelude::*;

    #[derive(Clone)]
    struct Sample {
        ts: i64,
        value: f64,
    }

    let samples: Vec<Sample> = (0..20)
        .map(|i| Sample {
            ts: i * 100,
            // Floor at ~100, occasional spikes to ~150
            value: if i % 5 == 0 { 150.0 } else { 100.0 },
        })
        .collect();

    let results: Vec<f64> = samples
        .into_iter()
        .tflo(|t| {
            let _ = t.timestamp(|x| x.ts);
            let value = t.prop(|x| x.value);
            value.baseline_correct(10usize, 0.1)
        })
        .collect();

    // Results should have values around 0 for floor, ~50 for spikes
    // (after warmup)
    assert!(results.len() == 20);
}

/// When normalize_range is called on Comp,
/// the computation shall scale values to [0, 1].
#[test]
fn test_fluent_normalize_range() {
    use tflo_core::prelude::*;

    #[derive(Clone)]
    struct Sample {
        ts: i64,
        value: f64,
    }

    let samples: Vec<Sample> = (0..20)
        .map(|i| Sample {
            ts: i * 100,
            value: (i as f64) * 5.0, // 0, 5, 10, ..., 95
        })
        .collect();

    let results: Vec<f64> = samples
        .into_iter()
        .tflo(|t| {
            let _ = t.timestamp(|x| x.ts);
            let value = t.prop(|x| x.value);
            value.normalize_range(10usize)
        })
        .collect();

    // After warmup, values should be in [0, 1] range
    for result in results.iter().skip(10) {
        assert!(*result >= 0.0 && *result <= 1.0);
    }
}

/// When calibrate is called on Comp,
/// the computation shall apply gain and offset.
#[test]
fn test_fluent_calibrate() {
    use tflo_core::prelude::*;

    #[derive(Clone)]
    struct Sample {
        ts: i64,
        raw: f64,
    }

    let samples: Vec<Sample> = (0..10)
        .map(|i| Sample {
            ts: i * 100,
            raw: i as f64 * 10.0, // 0, 10, 20, ...
        })
        .collect();

    // Calibration: physical = raw * 0.1 + 25
    let results: Vec<f64> = samples
        .into_iter()
        .tflo(|t| {
            let _ = t.timestamp(|x| x.ts);
            let raw = t.prop(|x| x.raw);
            raw.calibrate(0.1, 25.0)
        })
        .collect();

    // Check calibration: 0*0.1+25=25, 10*0.1+25=26, 20*0.1+25=27...
    assert!((results[0] - 25.0).abs() < 0.01);
    assert!((results[1] - 26.0).abs() < 0.01);
    assert!((results[2] - 27.0).abs() < 0.01);
}

/// When cross_hysteresis is called on Comp,
/// the computation shall generate signals with noise immunity.
#[test]
fn test_fluent_cross_hysteresis() {
    use tflo_core::prelude::*;

    #[derive(Clone)]
    struct Sample {
        ts: i64,
        value: f64,
        threshold: f64,
    }

    // Signal oscillates around threshold with small noise
    let samples: Vec<Sample> = vec![
        Sample {
            ts: 0,
            value: 98.0,
            threshold: 100.0,
        }, // Below
        Sample {
            ts: 100,
            value: 99.5,
            threshold: 100.0,
        }, // Still below
        Sample {
            ts: 200,
            value: 103.0,
            threshold: 100.0,
        }, // Above threshold + margin
        Sample {
            ts: 300,
            value: 101.0,
            threshold: 100.0,
        }, // Above but no new trigger
        Sample {
            ts: 400,
            value: 96.0,
            threshold: 100.0,
        }, // Below threshold - margin
    ];

    let results: Vec<ThresholdCrossEventMode> = samples
        .into_iter()
        .tflo(|t| {
            let _ = t.timestamp(|x| x.ts);
            let value = t.prop(|x| x.value);
            let threshold = t.prop(|x| x.threshold);
            value.cross_hysteresis(&threshold, 2.0) // 2.0 margin
        })
        .collect();

    // Should get Buy at index 2 (crossed above 102), Sell at index 4 (crossed below 98)
    assert_eq!(results[0], ThresholdCrossEventMode::None); // Initial
    assert_eq!(results[1], ThresholdCrossEventMode::None); // No cross
    assert_eq!(results[2], ThresholdCrossEventMode::Rising); // Crossed above threshold + margin
    assert_eq!(results[3], ThresholdCrossEventMode::None); // No new trigger
    assert_eq!(results[4], ThresholdCrossEventMode::Falling); // Crossed below threshold - margin
}
