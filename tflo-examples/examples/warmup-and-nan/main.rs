use std::sync::Arc;
use tflo_core::builder::Compile;
use tflo_core::compile::{CompiledGraph, StepResult};
use tflo_core::prelude::*;

/// A single PM2.5 measurement from an air-quality monitor.
#[derive(Clone, Debug)]
struct AirSample {
    /// Timestamp in milliseconds.
    ts: i64,
    /// Fine particulate matter concentration in µg/m³.
    pm25: f64,
}

impl AirSample {
    fn new(ts: i64, pm25: f64) -> Self {
        Self { ts, pm25 }
    }
}

/// Per-second PM2.5 readings from an air-quality monitor during a
/// slow rise in particulate levels.
fn sample_air() -> Vec<AirSample> {
    vec![
        AirSample::new(1000, 12.0),
        AirSample::new(2000, 13.0),
        AirSample::new(3000, 11.0),
        AirSample::new(4000, 14.0),
        AirSample::new(5000, 15.0),
        AirSample::new(6000, 17.0),
        AirSample::new(7000, 16.0),
        AirSample::new(8000, 18.0),
        AirSample::new(9000, 20.0),
        AirSample::new(10000, 19.0),
    ]
}

/// Air-quality readings with occasional NaN values, modelling moments
/// when the monitor failed to report a valid PM2.5 sample.
fn sample_air_with_nan() -> Vec<AirSample> {
    vec![
        AirSample::new(1000, 12.0),
        AirSample::new(2000, f64::NAN),
        AirSample::new(3000, 11.0),
        AirSample::new(4000, 14.0),
        AirSample::new(5000, 15.0),
        AirSample::new(6000, f64::NAN),
        AirSample::new(7000, 16.0),
        AirSample::new(8000, 18.0),
    ]
}

fn main() {
    let air = sample_air();
    let air_with_nan = sample_air_with_nan();

    // ---- Warmup demonstration ----
    println!("=== Warmup: SMA(3) with step_with_status ===");
    let mut builder = TFlowBuilder::new();
    builder.timestamp(|x: &AirSample| x.ts);
    let pm25 = builder.prop(|x| x.pm25);
    let sma = pm25.sma(3usize);
    let nodes = builder.into_nodes();
    let mut graph: CompiledGraph<AirSample, f64> =
        CompiledGraph::compile(Arc::new(|x: &AirSample| x.ts), nodes, sma.output_ids());

    for record in &air {
        let summary = graph.state_summary();
        println!(
            "ts={} records_seen={} is_warmed_up={} warmup_remaining={} node_count={} output_count={}",
            record.ts,
            summary.records_seen,
            summary.is_warmed_up,
            summary.warmup_remaining,
            summary.node_count,
            summary.output_count
        );

        match graph.step_with_status(record) {
            StepResult::Ready(item) => println!("  → Ready: {:.4}", item.value),
            StepResult::WarmingUp { remaining } => println!("  → WarmingUp: need {remaining} more"),
            StepResult::Error(e) => println!("  → Error: {e}"),
        }
    }

    // ---- NaN propagation ----
    println!("\n=== NaN Propagation ===");
    let nan_results: Vec<f64> = air_with_nan
        .clone()
        .into_iter()
        .tflo(|t| {
            t.timestamp(|x| x.ts);
            let pm25 = t.prop(|x| x.pm25);
            pm25.sma(3usize)
        })
        .collect();
    for (sample, result) in air_with_nan.iter().zip(&nan_results) {
        println!("ts={} pm25={} sma={}", sample.ts, sample.pm25, result);
    }

    // ---- Strict validation catches NaN ----
    println!("\n=== Strict Validation (catches NaN) ===");
    let validated_results: Vec<_> = air_with_nan
        .clone()
        .into_iter()
        .validated(ValidationOptions::strict(), |t| {
            t.timestamp(|x| x.ts);
            let pm25 = t.prop(|x| x.pm25);
            pm25.sma(3usize)
        })
        .collect();
    for (sample, result) in air_with_nan.iter().zip(&validated_results) {
        match result {
            Ok(val) => println!("ts={} pm25={} → Ok({})", sample.ts, sample.pm25, val),
            Err(e) => println!("ts={} pm25={} → Error: {e}", sample.ts, sample.pm25),
        }
    }

    // ---- Using .with() to keep original records ----
    println!("\n=== .with() preserves original records ===");
    let enriched: Vec<(AirSample, f64)> = air
        .clone()
        .into_iter()
        .with(|t| {
            t.timestamp(|x| x.ts);
            let pm25 = t.prop(|x| x.pm25);
            pm25.sma(3usize)
        })
        .collect();
    for (sample, sma_value) in &enriched {
        println!(
            "air(t={}, pm25={}) → sma={}",
            sample.ts, sample.pm25, sma_value
        );
    }
}
