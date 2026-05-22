use std::sync::Arc;
use tflo_core::builder::Compile;
use tflo_core::compile::{CompiledGraph, StepResult};
use tflo_core::prelude::*;
use tflo_examples::*;
use tflo_ops::prelude::*;

/// A single outdoor temperature reading from an IoT sensor.
#[derive(Clone, Debug)]
struct Reading {
    /// Timestamp in milliseconds.
    ts: i64,
    /// Measured temperature in degrees Celsius.
    celsius: f64,
}

impl Reading {
    fn new(ts: i64, celsius: f64) -> Self {
        Self { ts, celsius }
    }
}

/// A morning warming trend captured by an outdoor temperature sensor,
/// one reading per second.
fn sample_readings() -> Vec<Reading> {
    vec![
        Reading::new(1000, 12.0),
        Reading::new(2000, 12.4),
        Reading::new(3000, 11.8),
        Reading::new(4000, 13.1),
        Reading::new(5000, 13.6),
        Reading::new(6000, 14.2),
        Reading::new(7000, 14.0),
        Reading::new(8000, 14.9),
        Reading::new(9000, 15.5),
        Reading::new(10000, 15.2),
    ]
}

// Example showing manual graph construction and stepping
fn main() {
    let readings = sample_readings();

    // ---- Approach 1: Iterator API ----
    let smas: Vec<f64> = readings
        .clone()
        .into_iter()
        .tflo(|t| {
            t.timestamp(|x| x.ts);
            let temperature = t.prop(|x| x.celsius);
            temperature.sma(3usize)
        })
        .collect();
    print_summary("SMA(3) count-based", &smas);

    // ---- Approach 2: Time-based ----
    let time_smas: Vec<f64> = readings
        .clone()
        .into_iter()
        .tflo(|t| {
            t.timestamp(|x| x.ts);
            let temperature = t.prop(|x| x.celsius);
            temperature.sma(5_u64.secs())
        })
        .collect();
    print_summary("SMA(5s) time-based", &time_smas);

    // ---- Approach 3: WindowSpec fluent ----
    let ws_smas: Vec<f64> = readings
        .clone()
        .into_iter()
        .tflo(|t| {
            t.timestamp(|x| x.ts);
            let temperature = t.prop(|x| x.celsius);
            temperature.sma(20_u64.secs())
        })
        .collect();
    print_summary("SMA(20s) WindowSpec", &ws_smas);

    // ---- Approach 4: Manual stepping with status ----
    println!("\n--- Manual stepping with StepResult ---");
    let mut builder = TFlowBuilder::new();
    builder.timestamp(|x: &Reading| x.ts);
    let temperature = builder.prop(|x| x.celsius);
    let sma = temperature.sma(3usize);
    let nodes = builder.into_nodes();
    let mut graph: CompiledGraph<Reading, f64> =
        CompiledGraph::compile(Arc::new(|x: &Reading| x.ts), nodes, sma.output_ids());

    for record in &readings {
        let plan = graph.graph_plan();
        match graph.step_with_status(record) {
            StepResult::Ready(item) => {
                println!(
                    "ts={} → value={:.4} (seen={}, warmup_remaining={})",
                    record.ts, item.value, plan.records_seen, plan.warmup_remaining
                );
            }
            StepResult::WarmingUp { remaining, .. } => {
                println!("ts={} → WARMING_UP (remaining={})", record.ts, remaining);
            }
            StepResult::Error(e) => {
                println!("ts={} → ERROR: {e}", record.ts);
            }
        }
    }
}
