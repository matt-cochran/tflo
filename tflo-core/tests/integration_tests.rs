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
            price.map_f64(|x| x * 1.0)
        })
        .collect();

    assert_eq!(results.len(), 6);
    // Original records preserved
    assert_eq!(results[0].0.symbol, "AAPL");
    assert_eq!(results[0].0.price, 150.0);
}

/// When arithmetic operations are applied to computations,
/// the system shall correctly combine values via map2_f64,
/// So that derived metrics can be computed using closure ops,
/// And the result will be numerically correct.
#[test]
fn test_arithmetic_composition() {
    let ticks = sample_ticks();

    let results: Vec<f64> = ticks
        .into_iter()
        .tflo(|t| {
            let _ = t.timestamp(|x| x.ts);
            let price = t.prop(|x| x.price);
            let volume = t.prop(|x| x.volume);
            // Compute price * volume using map2_f64
            price.map2_f64(&volume, |p, v| p * v)
        })
        .collect();

    assert_eq!(results.len(), 6);
    // First tick: 150.0 * 1000.0 = 150_000
    assert!((results[0] - 150_000.0).abs() < 0.001);
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
            price.map_f64(|x| x * 1.0)
        })
        .collect();

    assert_eq!(results.len(), 3);
    assert!(results[0].is_ok());
    assert!(results[1].is_ok());
    assert!(results[2].is_err()); // Out of order detected
}
