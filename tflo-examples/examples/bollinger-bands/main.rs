use tflo_core::prelude::*;
use tflo_examples::*;
use tflo_fintech::prelude::*;
use tflo_ops::prelude::*;

/// A greenhouse temperature reading: a timestamp and a Celsius measurement.
#[derive(Clone, Debug)]
struct Reading {
    ts: i64,
    celsius: f64,
}

impl Reading {
    const fn new(ts: i64, celsius: f64) -> Self {
        Self { ts, celsius }
    }
}

/// Sample sensor readings from a greenhouse climate controller.
fn sample_readings() -> Vec<Reading> {
    vec![
        Reading::new(1000, 22.0),
        Reading::new(2000, 22.4),
        Reading::new(3000, 21.6),
        Reading::new(4000, 23.1),
        Reading::new(5000, 23.8),
        Reading::new(6000, 25.0),
        Reading::new(7000, 24.3),
        Reading::new(8000, 25.6),
        Reading::new(9000, 26.9),
        Reading::new(10000, 26.1),
    ]
}

fn main() {
    let readings = sample_readings();

    // ---- Manual composition: SMA + StdDev ----
    let manual: Vec<(f64, f64, f64)> = readings
        .clone()
        .into_iter()
        .tflo(|t| {
            t.timestamp(|x| x.ts);
            let celsius = t.prop(|x| x.celsius);
            let middle = celsius.sma(4usize);
            let std = celsius.std(4usize);
            // SAFETY: graph-node combinator (Comp<R> Mul/Add/Sub overloads); not numeric arithmetic
            #[allow(clippy::arithmetic_side_effects)]
            let band_width = &std * 2.0;
            #[allow(clippy::arithmetic_side_effects)]
            let upper = &middle + &band_width;
            #[allow(clippy::arithmetic_side_effects)]
            let lower = &middle - &band_width;
            (middle, upper, lower)
        })
        .collect();
    print_tuple3_summary("Manual temp band(4, 2.0)", &manual);

    // ---- Built-in convenience ----
    let builtin: Vec<(f64, f64, f64)> = readings
        .clone()
        .into_iter()
        .tflo(|t| {
            t.timestamp(|x| x.ts);
            let celsius = t.prop(|x| x.celsius);
            celsius.bollinger_bands(4usize, 2.0)
        })
        .collect();
    print_tuple3_summary("Built-in temp band(4, 2.0)", &builtin);

    // ---- EMA middle band instead of SMA ----
    let ema_bands: Vec<(f64, f64, f64)> = readings
        .clone()
        .into_iter()
        .tflo(|t| {
            t.timestamp(|x| x.ts);
            let celsius = t.prop(|x| x.celsius);
            let middle = celsius.ema(4usize);
            let std = celsius.std(4usize);
            // SAFETY: graph-node combinator (Comp<R> Mul/Add/Sub overloads); not numeric arithmetic
            #[allow(clippy::arithmetic_side_effects)]
            let band_width = &std * 2.0;
            #[allow(clippy::arithmetic_side_effects)]
            let upper = &middle + &band_width;
            #[allow(clippy::arithmetic_side_effects)]
            let lower = &middle - &band_width;
            (middle, upper, lower)
        })
        .collect();
    print_tuple3_summary("EMA temp band(4, 2.0)", &ema_bands);

    // ---- 3-sigma bands ----
    let bands_3sigma: Vec<(f64, f64, f64)> = readings
        .clone()
        .into_iter()
        .tflo(|t| {
            t.timestamp(|x| x.ts);
            let celsius = t.prop(|x| x.celsius);
            let middle = celsius.sma(4usize);
            let std = celsius.std(4usize);
            // SAFETY: graph-node combinator (Comp<R> Mul/Add/Sub overloads); not numeric arithmetic
            #[allow(clippy::arithmetic_side_effects)]
            let band_width = &std * 3.0;
            #[allow(clippy::arithmetic_side_effects)]
            let upper = &middle + &band_width;
            #[allow(clippy::arithmetic_side_effects)]
            let lower = &middle - &band_width;
            (middle, upper, lower)
        })
        .collect();
    print_tuple3_summary("3-sigma temp band(4, 3.0)", &bands_3sigma);

    // ---- Stable-climate detection (tight band) ----
    let stable: Vec<f64> = readings
        .into_iter()
        .tflo(|t| {
            t.timestamp(|x| x.ts);
            let celsius = t.prop(|x| x.celsius);
            let (_middle, upper, lower) = celsius.bollinger_bands(4usize, 2.0);
            // SAFETY: graph-node combinator (Comp<R> Sub overload); not numeric arithmetic
            #[allow(clippy::arithmetic_side_effects)]
            let band_width = &upper - &lower;
            let thresh = t.constant(10.0);
            band_width.lt(&thresh)
        })
        .collect();
    print_summary("Stable climate (band width < 10)", &stable);
}
