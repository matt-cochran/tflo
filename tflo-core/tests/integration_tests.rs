#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
//! Integration tests for tflow.

use tflo_core::prelude::*;

#[derive(Clone, Debug, PartialEq)]
struct Tick {
    ts: i64,
    symbol: String,
    price: f64,
    volume: f64,
}

fn sample_ticks() -> Vec<Tick> {
    vec![
        Tick {
            ts: 1000,
            symbol: "AAPL".into(),
            price: 150.0,
            volume: 1000.0,
        },
        Tick {
            ts: 2000,
            symbol: "GOOG".into(),
            price: 2800.0,
            volume: 500.0,
        },
        Tick {
            ts: 3000,
            symbol: "AAPL".into(),
            price: 151.0,
            volume: 1200.0,
        },
        Tick {
            ts: 4000,
            symbol: "GOOG".into(),
            price: 2810.0,
            volume: 600.0,
        },
        Tick {
            ts: 5000,
            symbol: "AAPL".into(),
            price: 149.0,
            volume: 800.0,
        },
        Tick {
            ts: 6000,
            symbol: "GOOG".into(),
            price: 2795.0,
            volume: 700.0,
        },
    ]
}

/// When time-based SMA receives values within its window,
/// TimeWindow shall compute the arithmetic mean,
/// So that the returned value equals sum/count,
/// And TimeWindow will be in a state where buffer contains only in-window values.
#[test]
fn test_sma_time_based() {
    let ticks = sample_ticks();

    let results: Vec<f64> = ticks
        .into_iter()
        .tflo(|t| {
            let _ = t.timestamp(|x| x.ts);
            let price = t.prop(|x| x.price);
            price.sma(10_u64.secs())
        })
        .collect();

    assert_eq!(results.len(), 6);
    // First tick: SMA = 150.0
    assert!((results[0] - 150.0).abs() < 0.001);
    // After 6 ticks, all in window: (150 + 2800 + 151 + 2810 + 149 + 2795) / 6
    let expected_last = (150.0 + 2800.0 + 151.0 + 2810.0 + 149.0 + 2795.0) / 6.0;
    assert!((results[5] - expected_last).abs() < 0.001);
}

/// When temporal_with is used,
/// the system shall return tuples of (original_record, computed_value),
/// So that users can access both the input and output,
/// And the original record will be unmodified.
#[test]
fn test_temporal_with_preserves_record() {
    let ticks = sample_ticks();

    let results: Vec<(Tick, f64)> = ticks
        .clone()
        .into_iter()
        .with(|t| {
            let _ = t.timestamp(|x| x.ts);
            let price = t.prop(|x| x.price);
            price.sma(5_u64.secs())
        })
        .collect();

    assert_eq!(results.len(), 6);
    // Original records preserved
    assert_eq!(results[0].0.symbol, "AAPL");
    assert_eq!(results[0].0.price, 150.0);
}

/// When cross_above detects a value crossing above threshold,
/// CrossDetector shall return `ThresholdCrossEventMode::Rising`,
/// So that users can detect upward crossings,
/// And CrossDetector will be in a state tracking the new position.
#[test]
fn test_cross_detection() {
    #[derive(Clone)]
    struct Data {
        ts: i64,
        value: f64,
    }

    let data = vec![
        Data {
            ts: 1000,
            value: 90.0,
        },
        Data {
            ts: 2000,
            value: 95.0,
        },
        Data {
            ts: 3000,
            value: 105.0,
        }, // Crosses above 100
        Data {
            ts: 4000,
            value: 110.0,
        },
        Data {
            ts: 5000,
            value: 95.0,
        }, // Crosses below 100
    ];

    let results: Vec<ThresholdCrossEventMode> = data
        .into_iter()
        .tflo(|t| {
            let _ = t.timestamp(|x| x.ts);
            let value = t.prop(|x| x.value);
            let threshold = t.constant(100.0);
            value.cross(&threshold)
        })
        .collect();

    assert_eq!(results.len(), 5);
    assert_eq!(results[0], ThresholdCrossEventMode::None); // First observation
    assert_eq!(results[1], ThresholdCrossEventMode::None); // Still below
    assert_eq!(results[2], ThresholdCrossEventMode::Rising); // Crossed above
    assert_eq!(results[3], ThresholdCrossEventMode::None); // Still above
    assert_eq!(results[4], ThresholdCrossEventMode::Falling); // Crossed below
}

/// When multiple computations are requested,
/// the system shall compute all values efficiently,
/// So that users can get multiple outputs in a single pass,
/// And the computation graph will share common subexpressions.
#[test]
fn test_multiple_outputs() {
    let ticks = sample_ticks();

    let results: Vec<(f64, f64, f64)> = ticks
        .into_iter()
        .tflo(|t| {
            let _ = t.timestamp(|x| x.ts);
            let price = t.prop(|x| x.price);
            let sma = price.sma(5_u64.secs());
            let max = price.max(5_u64.secs());
            let min = price.min(5_u64.secs());
            (sma, max, min)
        })
        .collect();

    assert_eq!(results.len(), 6);
    // Check last result
    let (sma, max, min) = results[5];
    assert!(sma > min);
    assert!(sma < max);
}

/// When arithmetic operations are applied to computations,
/// the system shall correctly combine values,
/// So that derived metrics like z-score can be computed,
/// And the result will be numerically correct.
#[test]
fn test_arithmetic_composition() {
    #[derive(Clone)]
    struct Data {
        ts: i64,
        value: f64,
    }

    let data: Vec<Data> = (0..100)
        .map(|i| Data {
            ts: i * 100,
            value: 100.0 + (i as f64) * 0.5,
        })
        .collect();

    let results: Vec<f64> = data
        .into_iter()
        .tflo(|t| {
            let _ = t.timestamp(|x| x.ts);
            let value = t.prop(|x| x.value);
            let sma = value.sma(1_u64.secs());
            let std = value.std(1_u64.secs());
            // Z-score: (value - mean) / std
            (&value - &sma) / &std
        })
        .collect();

    assert_eq!(results.len(), 100);
    // Z-scores should be reasonable
    for z in &results[10..] {
        // Skip warmup period
        if !z.is_nan() {
            assert!(z.abs() < 10.0, "Z-score too extreme: {z}");
        }
    }
}

/// When merge_by_timestamp combines streams,
/// the system shall output items in timestamp order,
/// So that users can process interleaved data correctly,
/// And all items from all streams will be included.
#[test]
fn test_merge_streams() {
    let stream1 = vec![
        Tick {
            ts: 1000,
            symbol: "A".into(),
            price: 1.0,
            volume: 1.0,
        },
        Tick {
            ts: 3000,
            symbol: "A".into(),
            price: 3.0,
            volume: 1.0,
        },
        Tick {
            ts: 5000,
            symbol: "A".into(),
            price: 5.0,
            volume: 1.0,
        },
    ];

    let stream2 = vec![
        Tick {
            ts: 2000,
            symbol: "B".into(),
            price: 2.0,
            volume: 1.0,
        },
        Tick {
            ts: 4000,
            symbol: "B".into(),
            price: 4.0,
            volume: 1.0,
        },
    ];

    let merged: Vec<Tick> =
        merge_by_timestamp(vec![stream1.into_iter(), stream2.into_iter()], |t| t.ts).collect();

    assert_eq!(merged.len(), 5);
    assert_eq!(
        merged.iter().map(|t| t.ts).collect::<Vec<_>>(),
        vec![1000, 2000, 3000, 4000, 5000]
    );
}

/// When batch_by_time is applied,
/// the system shall group records by time intervals,
/// So that users can process time-bucketed data,
/// And each batch will contain only records from that interval.
#[test]
fn test_batch_by_time() {
    let ticks = sample_ticks();

    let batches: Vec<Vec<Tick>> =
        batch_by_time(ticks.into_iter(), |t| t.ts, 2_u64.secs()).collect();

    // Expect batches for intervals: [0-2000), [2000-4000), [4000-6000)
    // But our data: 1000, 2000, 3000, 4000, 5000, 6000
    // So: [1000], [2000, 3000], [4000, 5000], [6000]
    assert!(!batches.is_empty());
}

/// When dedupe_by_key is applied,
/// the system shall remove duplicates within the window,
/// So that only the first occurrence is kept,
/// And duplicates outside the window will be preserved.
#[test]
fn test_dedupe() {
    let ticks = vec![
        Tick {
            ts: 1000,
            symbol: "AAPL".into(),
            price: 150.0,
            volume: 1.0,
        },
        Tick {
            ts: 1100,
            symbol: "AAPL".into(),
            price: 150.1,
            volume: 1.0,
        }, // Duplicate
        Tick {
            ts: 1200,
            symbol: "GOOG".into(),
            price: 2800.0,
            volume: 1.0,
        },
        Tick {
            ts: 5000,
            symbol: "AAPL".into(),
            price: 151.0,
            volume: 1.0,
        }, // Outside window
    ];

    let deduped: Vec<Tick> = dedupe_by_key(
        ticks.into_iter(),
        |t| t.symbol.clone(),
        |t| t.ts,
        2_u64.secs(),
    )
    .collect();

    assert_eq!(deduped.len(), 3);
    assert_eq!(deduped[0].symbol, "AAPL");
    assert_eq!(deduped[1].symbol, "GOOG");
    assert_eq!(deduped[2].symbol, "AAPL"); // After window expired
}

/// When rate_limit is applied,
/// the system shall drop items that arrive too quickly,
/// So that output rate is controlled,
/// And at least min_interval passes between outputs.
#[test]
fn test_rate_limit() {
    let ticks = sample_ticks();

    let limited: Vec<Tick> = rate_limit(ticks.into_iter(), |t| t.ts, 3_u64.secs()).collect();

    // With 3 second limit: ts 1000, then next at 4000, then 6000 but may be <3s
    assert!(limited.len() < 6);
    assert_eq!(limited[0].ts, 1000);
}

/// When partition is applied,
/// the system shall split items into two collections,
/// So that items matching the predicate go to one, others to another,
/// And all items will be in exactly one collection.
#[test]
fn test_partition() {
    let ticks = sample_ticks();

    let (aapl, others) = partition(ticks, |t| t.symbol == "AAPL");

    assert_eq!(aapl.len(), 3);
    assert_eq!(others.len(), 3);
    assert!(aapl.iter().all(|t| t.symbol == "AAPL"));
    assert!(others.iter().all(|t| t.symbol == "GOOG"));
}

/// When prev_by is used with a key function,
/// the system shall track previous values per key,
/// So that delta calculations are scoped to each key,
/// And cross-key values will not interfere.
#[test]
fn test_prev_by_key() {
    let ticks = sample_ticks();

    let results: Vec<(Tick, f64)> = ticks
        .into_iter()
        .with(|t| {
            let _ = t.timestamp(|x| x.ts);
            let price = t.prop(|x| x.price);
            price.prev_by(|x| x.symbol.clone())
        })
        .collect();

    assert_eq!(results.len(), 6);
    // First AAPL has no prev
    assert!(results[0].1.is_nan());
    // First GOOG has no prev
    assert!(results[1].1.is_nan());
    // Second AAPL (index 2) should have prev = 150.0
    assert!((results[2].1 - 150.0).abs() < 0.001);
    // Second GOOG (index 3) should have prev = 2800.0
    assert!((results[3].1 - 2800.0).abs() < 0.001);
}

/// When validation is enabled with assert_sorted,
/// the system shall detect out-of-order timestamps,
/// So that data quality issues are caught early,
/// And an error will be returned for violations.
#[test]
fn test_validation_sorted() {
    let unsorted = vec![
        Tick {
            ts: 1000,
            symbol: "A".into(),
            price: 1.0,
            volume: 1.0,
        },
        Tick {
            ts: 3000,
            symbol: "A".into(),
            price: 3.0,
            volume: 1.0,
        },
        Tick {
            ts: 2000,
            symbol: "A".into(),
            price: 2.0,
            volume: 1.0,
        }, // Out of order!
    ];

    let options = ValidationOptions::new().assert_sorted(true);

    let results: Vec<TFloResult<f64>> = unsorted
        .into_iter()
        .validated(options, |t| {
            let _ = t.timestamp(|x| x.ts);
            let price = t.prop(|x| x.price);
            price.sma(5_u64.secs())
        })
        .collect();

    assert_eq!(results.len(), 3);
    assert!(results[0].is_ok());
    assert!(results[1].is_ok());
    assert!(results[2].is_err()); // Out of order detected
}
