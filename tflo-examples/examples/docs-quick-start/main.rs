use std::sync::Arc;
use tflo_core::builder::Compile;
use tflo_core::compile::CompiledGraph;
use tflo_core::prelude::*;
use tflo_examples::*;
use tflo_ops::prelude::*;

/// A weather-station reading: a timestamp and a sensor measurement
/// (e.g. temperature in degrees Celsius).
#[derive(Clone, Debug)]
struct Reading {
    ts: i64,
    value: f64,
}

impl Reading {
    fn new(ts: i64, value: f64) -> Self {
        Self { ts, value }
    }
}

/// Sample readings streamed from an outdoor IoT weather station.
fn sample_readings() -> Vec<Reading> {
    vec![
        Reading::new(1000, 18.0),
        Reading::new(2000, 18.4),
        Reading::new(3000, 17.6),
        Reading::new(4000, 19.1),
        Reading::new(5000, 19.8),
        Reading::new(6000, 21.0),
        Reading::new(7000, 20.3),
        Reading::new(8000, 21.6),
        Reading::new(9000, 22.9),
        Reading::new(10000, 22.1),
    ]
}

fn main() {
    let readings = sample_readings();

    // ---- Iterator API: .tflo() ----
    let smas: Vec<f64> = readings
        .clone()
        .into_iter()
        .tflo(|t| {
            t.timestamp(|x| x.ts);
            let value = t.prop(|x| x.value);
            value.sma(3_u64.secs()) // 3-second SMA (time-based)
        })
        .collect();
    print_summary("SMA(3s) time-based", &smas);

    // ---- Count-based window ----
    let smas_count: Vec<f64> = readings
        .clone()
        .into_iter()
        .tflo(|t| {
            t.timestamp(|x| x.ts);
            let value = t.prop(|x| x.value);
            value.sma(3usize) // 3-reading SMA (count-based)
        })
        .collect();
    print_summary("SMA(3) count-based", &smas_count);

    // ---- Multiple indicators in one graph ----
    let multi: Vec<(f64, f64)> = readings
        .clone()
        .into_iter()
        .tflo(|t| {
            t.timestamp(|x| x.ts);
            let value = t.prop(|x| x.value);
            let sma = value.sma(3_u64.secs());
            let ema = value.ema(3_u64.secs());
            (sma, ema)
        })
        .collect();
    let sma_vals: Vec<f64> = multi.iter().map(|(s, _)| *s).collect();
    print_summary("SMA+EMA (SMA part)", &sma_vals);

    // ---- Enrichment pattern: .with() ----
    let enriched: Vec<(Reading, f64)> = readings
        .clone()
        .into_iter()
        .with(|t| {
            t.timestamp(|x| x.ts);
            let value = t.prop(|x| x.value);
            value.sma(3_u64.secs())
        })
        .collect();
    assert_eq!(enriched.len(), readings.len());

    // ---- Manual graph with snapshot/restore ----
    let mut builder = TFlowBuilder::new();
    builder.timestamp(|x: &Reading| x.ts);
    let value = builder.prop(|x| x.value);
    let sma = value.sma(3usize);
    let output_ids = sma.output_ids();
    let nodes = builder.into_nodes();
    let mut graph: CompiledGraph<Reading, f64> =
        CompiledGraph::compile(Arc::new(|x: &Reading| x.ts), nodes, output_ids);

    let mut outputs = Vec::new();
    for record in &readings {
        if let Some(item) = graph.step(record) {
            outputs.push(item.value);
        }
    }
    print_summary("Manual graph SMA(3)", &outputs);

    // ---- Snapshot and restore ----
    let Ok(snapshot) = graph.snapshot() else {
        return;
    };
    let mut builder2 = TFlowBuilder::new();
    builder2.timestamp(|x: &Reading| x.ts);
    let value2 = builder2.prop(|x| x.value);
    let sma2 = value2.sma(3usize);
    let output_ids2 = sma2.output_ids();
    let nodes2 = builder2.into_nodes();
    let mut graph2: CompiledGraph<Reading, f64> =
        CompiledGraph::compile(Arc::new(|x: &Reading| x.ts), nodes2, output_ids2);
    let _ = graph2.restore(&snapshot);
}
