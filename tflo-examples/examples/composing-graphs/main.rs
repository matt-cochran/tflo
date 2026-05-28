use std::sync::Arc;
use tflo_core::builder::Compile;
use tflo_core::compile::CompiledGraph;
use tflo_core::prelude::*;
use tflo_ops::prelude::*;

// A per-interval API health stat: fraction of requests that returned an error.
#[derive(Clone, Debug)]
struct RequestStat {
    ts: i64,
    error_rate: f64,
}

impl RequestStat {
    const fn new(ts: i64, error_rate: f64) -> Self {
        Self { ts, error_rate }
    }
}

// Error-rate samples (percent) scraped once per interval from an API service.
fn sample_request_stats() -> Vec<RequestStat> {
    vec![
        RequestStat::new(1000, 100.0),
        RequestStat::new(2000, 101.0),
        RequestStat::new(3000, 99.0),
        RequestStat::new(4000, 102.0),
        RequestStat::new(5000, 103.0),
        RequestStat::new(6000, 105.0),
        RequestStat::new(7000, 104.0),
        RequestStat::new(8000, 106.0),
        RequestStat::new(9000, 108.0),
        RequestStat::new(10000, 107.0),
    ]
}

fn main() {
    let stats = sample_request_stats();

    // ---- Simple tuple output (equivalent to zip) ----
    println!("=== Tuple output (same builder) ===");
    let multi: Vec<(f64, f64)> = stats
        .clone()
        .into_iter()
        .tflo(|t| {
            t.timestamp(|x: &RequestStat| x.ts);
            let error_rate = t.prop(|x: &RequestStat| x.error_rate);
            let sma = error_rate.sma(3usize);
            let rsi = error_rate.rsi(14usize);
            (sma, rsi)
        })
        .collect();
    for (ts, (sma, rsi)) in stats.iter().map(|s| s.ts).zip(&multi) {
        println!("ts={ts:>6} sma={sma:.4} rsi={rsi:.4}");
    }

    // ---- Manual graph building + zip ----
    println!("\n=== Manual zip of separate graphs ===");
    fn build_sma_graph() -> CompiledGraph<RequestStat, f64> {
        let mut builder = TFlowBuilder::new();
        builder.timestamp(|x: &RequestStat| x.ts);
        let error_rate = builder.prop(|x: &RequestStat| x.error_rate);
        let sma = error_rate.sma(3usize);
        let nodes = builder.into_nodes();
        CompiledGraph::compile(Arc::new(|x: &RequestStat| x.ts), nodes, sma.output_ids())
    }
    fn build_rsi_graph() -> CompiledGraph<RequestStat, f64> {
        let mut builder = TFlowBuilder::new();
        builder.timestamp(|x: &RequestStat| x.ts);
        let error_rate = builder.prop(|x: &RequestStat| x.error_rate);
        let rsi = error_rate.rsi(14usize);
        let nodes = builder.into_nodes();
        CompiledGraph::compile(Arc::new(|x: &RequestStat| x.ts), nodes, rsi.output_ids())
    }

    let sma_graph = build_sma_graph();
    let rsi_graph = build_rsi_graph();
    let mut combined = sma_graph.zip(rsi_graph);

    println!("Zipped graph stepping:");
    for record in &stats {
        if let Some(item) = combined.step(record) {
            println!(
                "  ts={} → sma={:.4}, rsi={:.4}",
                record.ts, item.value.0, item.value.1
            );
        }
    }

    // ---- Map: transform output ----
    println!("\n=== Map: categorize SMA ===");
    let mut mapped = build_sma_graph().map(|value| {
        if value > 103.0 {
            "ABOVE".to_string()
        } else {
            "BELOW".to_string()
        }
    });
    println!("SMA(3) categorized:");
    for record in &stats {
        if let Some(item) = mapped.step(record) {
            println!("  ts={} → {}", record.ts, item.value);
        }
    }

    // ---- Filter: suppress unwanted outputs ----
    println!("\n=== Filter: keep only values above threshold ===");
    let mut filtered = build_sma_graph().filter(|value| *value > 102.0);
    println!("SMA(3) > 102.0:");
    for record in &stats {
        if let Some(item) = filtered.step(record) {
            println!("  ts={} → {:?}", record.ts, item.value);
        }
    }

    // ---- Fold: count consecutive valid signals ----
    println!("\n=== Fold: count consecutive above-threshold SMAs ===");
    let mut folded =
        build_sma_graph()
            .filter(|value| *value > 102.0)
            .fold(
                0u64,
                // SAFETY: consecutive-signal counter; saturating_add keeps the value
                // monotonic at u64::MAX rather than panicking on the (unreachable) overflow.
                |count, signal| {
                    if signal.is_some() { count.saturating_add(1) } else { 0 }
                },
            );
    println!("Consecutive SMA(3) above 102.0:");
    for record in &stats {
        if let Some(item) = folded.step(record) {
            println!("  ts={} → count={}", record.ts, item.value);
        }
    }

    // ---- Reduce: collapse tuple ----
    println!("\n=== Reduce: ratio of two graphs ===");
    fn build_error_rate_graph() -> CompiledGraph<RequestStat, f64> {
        let mut builder = TFlowBuilder::new();
        builder.timestamp(|x: &RequestStat| x.ts);
        let error_rate = builder.prop(|x: &RequestStat| x.error_rate);
        let nodes = builder.into_nodes();
        CompiledGraph::compile(
            Arc::new(|x: &RequestStat| x.ts),
            nodes,
            error_rate.output_ids(),
        )
    }
    let mut ratio = build_error_rate_graph()
        .zip(build_error_rate_graph())
        .reduce(|a, b| a / b);
    for record in &stats {
        if let Some(item) = ratio.step(record) {
            println!(
                "  ts={} → error_rate/error_rate={:.4}",
                record.ts, item.value
            );
        }
    }

    println!("\n--- Combinator reference ---");
    println!("zip:        Merge two graphs -> (O1, O2)");
    println!("map:        Transform output -> O2");
    println!("filter:     Suppress by predicate -> Option<O>");
    println!("filter_map: Map + filter in one -> Option<O2>");
    println!("fold:       Stateful accumulation -> Acc");
    println!("reduce:     Collapse (A,B) -> D");
    println!("pipe:       Chain graph output as next input");
}
