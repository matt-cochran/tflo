use std::sync::Arc;
use tflo_core::builder::Compile;
use tflo_core::compile::CompiledGraph;
use tflo_core::prelude::*;

// A power-grid telemetry sample: instantaneous load on a feeder, in megawatts.
#[derive(Clone, Debug)]
struct GridSample {
    ts: i64,
    load_mw: f64,
}

impl GridSample {
    fn new(ts: i64, load_mw: f64) -> Self {
        Self { ts, load_mw }
    }
}

// Grid load samples (MW) collected at a fixed cadence from a SCADA feed.
fn sample_grid_load() -> Vec<GridSample> {
    vec![
        GridSample::new(1000, 100.0),
        GridSample::new(2000, 101.0),
        GridSample::new(3000, 99.0),
        GridSample::new(4000, 102.0),
        GridSample::new(5000, 103.0),
        GridSample::new(6000, 105.0),
        GridSample::new(7000, 104.0),
        GridSample::new(8000, 106.0),
        GridSample::new(9000, 108.0),
        GridSample::new(10000, 107.0),
    ]
}

fn main() {
    println!("=== Computation Graph Architecture ===");
    println!();
    println!("tflo's architecture has these layers:");
    println!();
    println!("  Builder  →  Nodes  →  Compile  →  Execute");
    println!("  (DSL)       (DAG)     (Plan)      (Runtime)");
    println!();

    // ---- Phase 1: Build (DSL) ----
    println!("--- Phase 1: Build (DSL) ---");
    let mut builder = TFlowBuilder::<GridSample>::new();
    builder.timestamp(|x: &GridSample| x.ts);
    let load = builder.prop(|x| x.load_mw);
    let sma = load.sma(3usize);
    let std = load.std(3usize);
    let diff = &load - &sma;
    let zscore = &diff / &std;
    println!("Nodes created:");
    println!("  - prop:  load");
    println!("  - sma:   load -> SMA(3)");
    println!("  - std:   load -> StdDev(3)");
    println!("  - sub:   load - sma");
    println!("  - div:   (load - sma) / std -> zscore");
    println!();

    // ---- Phase 2: Compile ----
    println!("--- Phase 2: Compile ---");
    let output_ids = zscore.output_ids();
    let nodes = builder.into_nodes();
    let mut graph: CompiledGraph<GridSample, f64> =
        CompiledGraph::compile(Arc::new(|x: &GridSample| x.ts), nodes, output_ids);

    let plan = graph.graph_plan();
    println!("GraphPlan:");
    println!("  node_count:         {}", plan.node_count);
    println!("  base_node_count:    {}", plan.base_node_count);
    println!("  output_count:       {}", plan.output_count);
    println!("  min_warmup:         {}", plan.min_warmup);
    println!();

    // ---- Phase 3: Execute ----
    println!("--- Phase 3: Execute ---");
    let samples = sample_grid_load();
    for record in &samples {
        let summary = graph.state_summary();
        match graph.step_with_status(record) {
            tflo_core::compile::StepResult::Ready(item) => {
                println!(
                    "  ts={:>6} load={:.1} MW → zscore={:.4} (records_seen={})",
                    record.ts, record.load_mw, item.value, summary.records_seen
                );
            }
            tflo_core::compile::StepResult::WarmingUp { remaining } => {
                println!(
                    "  ts={:>6} load={:.1} MW → warming_up (need {remaining} more)",
                    record.ts, record.load_mw
                );
            }
            tflo_core::compile::StepResult::Error(e) => {
                println!(
                    "  ts={:>6} load={:.1} MW → error: {e}",
                    record.ts, record.load_mw
                );
            }
        }
    }
    println!();

    // ---- Node types overview ----
    println!("--- Node types ---");
    println!("Sources:       Prop, Const");
    println!("Windowed:      Sma, Ema, Std, Variance, Max, Min, Sum, Count, Wma, Median, Rsi");
    println!("Lookback:      Prev, PrevBy, Lag, Delta");
    println!("Arithmetic:    Add, Sub, Mul, Div, Abs, Sqrt, Ln, Pow, Exp");
    println!("Comparison:    Gt, Gte, Lt, Lte, Eq");
    println!("Cumulative:    CumSum, CumMax, CumMin, CumProd");
    println!("Returns:       PctChange, LogReturn");
    println!("Rate:          Rate, Velocity, Acceleration");
    println!("Cross:         Cross, CrossAbove, CrossUnder, CrossHysteresis");
    println!("Signal:        GlitchFilter, RuntDetect, PulseWidth, WindowDetect");
    println!("Functional:    MapF64, Map2F64, FilterF64, FilterMapF64, ScanF64, Scan2F64");
    println!("Statistical:   Correlation, Covariance, Skewness, Kurtosis, Rank, Quantile");
    println!("Composite:     BollingerBands, MACD, ZScore, Drawdown, Stochastic, ATR, CCI");
}
