use std::sync::Arc;
use tflo_core::builder::Compile;
use tflo_core::compile::CompiledGraph;
use tflo_core::prelude::*;
use tflo_examples::*;
use tflo_ops::events::ThresholdCrossEventMode;
use tflo_ops::prelude::*;

// A datacenter host CPU-utilization sample, in percent.
#[derive(Clone, Debug)]
struct CpuSample {
    ts: i64,
    cpu_pct: f64,
}

impl CpuSample {
    fn new(ts: i64, cpu_pct: f64) -> Self {
        Self { ts, cpu_pct }
    }
}

// CPU utilization (%) sampled once per second from a datacenter host.
// RSI works fine here as a generic 0-100 momentum gauge on any series.
fn sample_cpu_load() -> Vec<f64> {
    vec![
        44.0, 44.25, 44.5, 43.75, 44.5, 44.25, 44.0, 43.5, 43.25, 43.0, 43.25, 43.5, 43.75, 44.0,
        44.25,
    ]
}

fn main() {
    // Create CPU-typed samples from the raw utilization series
    let samples: Vec<CpuSample> = sample_cpu_load()
        .into_iter()
        .enumerate()
        .map(|(i, cpu_pct)| CpuSample::new((i as i64 + 1) * 1000, cpu_pct))
        .collect();

    // ---- Count-based RSI(14) ----
    let rsi_count: Vec<f64> = samples
        .clone()
        .into_iter()
        .tflo(|t| {
            t.timestamp(|x| x.ts);
            let cpu_pct = t.prop(|x| x.cpu_pct);
            cpu_pct.rsi(14usize)
        })
        .collect();
    print_summary("RSI(14) count-based", &rsi_count);

    // ---- Wilder's smoothed RSI(14) ----
    let rsi_wilder: Vec<f64> = samples
        .clone()
        .into_iter()
        .tflo(|t| {
            t.timestamp(|x| x.ts);
            let cpu_pct = t.prop(|x| x.cpu_pct);
            cpu_pct.rsi_wilder_n(14)
        })
        .collect();
    print_summary("RSI Wilder(14)", &rsi_wilder);

    // ---- Cross above/below thresholds ----
    let signals: Vec<(f64, ThresholdCrossEventMode, ThresholdCrossEventMode)> = samples
        .clone()
        .into_iter()
        .tflo(|t| {
            t.timestamp(|x| x.ts);
            let cpu_pct = t.prop(|x| x.cpu_pct);
            let rsi = cpu_pct.rsi(14usize);
            let above = rsi.cross_above(&t.constant(70.0));
            let below = rsi.cross_under(&t.constant(30.0));
            (rsi, above, below)
        })
        .collect();

    println!("\n--- CPU-load momentum cross signals ---");
    for (ts, (rsi, above, below)) in samples.iter().map(|s| s.ts).zip(&signals) {
        if *above != ThresholdCrossEventMode::None {
            println!("ts={ts:>6} momentum={rsi:.2} → RAMPING UP (crossed above 70)");
        }
        if *below != ThresholdCrossEventMode::None {
            println!("ts={ts:>6} momentum={rsi:.2} → COOLING OFF (crossed below 30)");
        }
    }

    // ---- Manual graph with RSI inspection ----
    println!("\n--- Manual RSI stepping ---");
    let mut builder = TFlowBuilder::new();
    builder.timestamp(|x: &CpuSample| x.ts);
    let cpu_pct = builder.prop(|x| x.cpu_pct);
    let rsi = cpu_pct.rsi(14usize);
    let nodes = builder.into_nodes();
    let mut graph: CompiledGraph<CpuSample, f64> =
        CompiledGraph::compile(Arc::new(|x: &CpuSample| x.ts), nodes, rsi.output_ids());

    for sample in &samples {
        let plan = graph.graph_plan();
        match graph.step_with_status(sample) {
            tflo_core::compile::StepResult::Ready(item) => {
                println!(
                    "ts={} rsi={:.4} (seen={})",
                    sample.ts, item.value, plan.records_seen
                );
            }
            tflo_core::compile::StepResult::WarmingUp { remaining, .. } => {
                println!("ts={} WARMING_UP (need {remaining} more)", sample.ts);
            }
            tflo_core::compile::StepResult::Error(e) => {
                println!("ts={} ERROR: {e}", sample.ts);
            }
        }
    }
}
