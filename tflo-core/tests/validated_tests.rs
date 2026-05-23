#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
//! Integration tests for `validated()` — proves every `ValidationOptions`
//! field is actually enforced (not silently ignored).

use tflo_core::prelude::*;

#[derive(Clone, Debug)]
struct Rec {
    ts: i64,
    value: f64,
}

const fn rec(ts: i64, value: f64) -> Rec {
    Rec { ts, value }
}

/// Run `validated()` over `data` with `opts`; the graph's single output is the
/// record's own value.
fn validate(data: Vec<Rec>, opts: ValidationOptions) -> Vec<TFloResult<f64>> {
    data.into_iter()
        .validated(opts, |t| {
            t.timestamp(|x: &Rec| x.ts);
            t.prop(|x: &Rec| x.value)
        })
        .collect()
}

#[test]
fn assert_sorted_rejects_out_of_order_timestamp() {
    let data = vec![rec(1000, 1.0), rec(2000, 2.0), rec(1500, 3.0)];
    let out = validate(data, ValidationOptions::new().assert_sorted(true));
    assert_eq!(out[0], Ok(1.0));
    assert_eq!(out[1], Ok(2.0));
    assert!(matches!(
        out[2],
        Err(TFloError::OutOfOrderTimestamp {
            previous: 2000,
            current: 1500
        })
    ));
}

#[test]
fn min_warmup_suppresses_early_records() {
    let data = vec![
        rec(1000, 1.0),
        rec(2000, 2.0),
        rec(3000, 3.0),
        rec(4000, 4.0),
        rec(5000, 5.0),
    ];
    let out = validate(data, ValidationOptions::new().min_warmup(3));
    // The first two records are inside the warmup window and produce no output.
    assert_eq!(out, vec![Ok(3.0), Ok(4.0), Ok(5.0)]);
}

#[test]
fn max_gap_ms_rejects_large_timestamp_gap() {
    let data = vec![rec(1000, 1.0), rec(2000, 2.0), rec(9000, 3.0)];
    let out = validate(data, ValidationOptions::new().max_gap_ms(5000));
    assert_eq!(out[0], Ok(1.0));
    assert_eq!(out[1], Ok(2.0));
    assert!(matches!(
        out[2],
        Err(TFloError::TimestampGapExceeded {
            previous: 2000,
            current: 9000,
            max_gap: 5000
        })
    ));
}

#[test]
fn reject_nan_filters_nan_values() {
    let data = vec![rec(1000, 1.0), rec(2000, f64::NAN), rec(3000, 3.0)];
    let out = validate(data, ValidationOptions::new().reject_nan(true));
    // The NaN record is filtered out of the stream entirely.
    assert_eq!(out, vec![Ok(1.0), Ok(3.0)]);
}

#[test]
fn reject_inf_filters_infinite_values() {
    let data = vec![rec(1000, 1.0), rec(2000, f64::INFINITY), rec(3000, 3.0)];
    let out = validate(data, ValidationOptions::new().reject_inf(true));
    assert_eq!(out, vec![Ok(1.0), Ok(3.0)]);
}

#[test]
fn error_on_nan_raises_an_error() {
    let data = vec![rec(1000, 1.0), rec(2000, f64::NAN)];
    let out = validate(data, ValidationOptions::new().error_on_nan(true));
    assert_eq!(out[0], Ok(1.0));
    assert_eq!(out[1], Err(TFloError::NaN));
}

#[test]
fn error_on_inf_raises_an_error() {
    let data = vec![rec(1000, 1.0), rec(2000, f64::INFINITY)];
    let out = validate(data, ValidationOptions::new().error_on_inf(true));
    assert_eq!(out[0], Ok(1.0));
    assert_eq!(out[1], Err(TFloError::Infinite));
}

#[test]
fn error_on_negative_raises_an_error() {
    let data = vec![rec(1000, 1.0), rec(2000, -5.0)];
    let out = validate(data, ValidationOptions::new().error_on_negative(true));
    assert_eq!(out[0], Ok(1.0));
    assert!(matches!(out[1], Err(TFloError::NegativeValue { .. })));
}

#[test]
fn clean_data_passes_every_check() {
    let data = vec![rec(1000, 1.0), rec(2000, 2.0), rec(3000, 3.0)];
    let out = validate(data, ValidationOptions::strict().max_gap_ms(5000));
    assert_eq!(out, vec![Ok(1.0), Ok(2.0), Ok(3.0)]);
}
