use std::sync::Arc;
use tflo_core::builder::Compile;
use tflo_core::compile::CompiledGraph;
use tflo_core::prelude::*;
use tflo_examples::*;
use tflo_ops::prelude::*;

/// An RF spectrum detection event: a timestamp and the measured
/// signal-to-noise ratio (dB) at a given centre frequency (MHz).
#[derive(Clone, Debug)]
struct Detection {
    ts: i64,
    snr: f64,
    freq_mhz: f64,
}

impl Detection {
    const fn new(ts: i64, snr: f64, freq_mhz: f64) -> Self {
        Self { ts, snr, freq_mhz }
    }
}

/// Sample SNR measurements streamed from a spectrum-monitoring receiver.
fn sample_detections() -> Vec<Detection> {
    vec![
        Detection::new(1000, 12.0, 433.9),
        Detection::new(2000, 13.0, 433.9),
        Detection::new(3000, 11.0, 433.9),
        Detection::new(4000, 14.0, 433.9),
        Detection::new(5000, 15.0, 433.9),
        Detection::new(6000, 17.0, 433.9),
        Detection::new(7000, 16.0, 433.9),
        Detection::new(8000, 18.0, 433.9),
        Detection::new(9000, 20.0, 433.9),
        Detection::new(10000, 19.0, 433.9),
    ]
}

#[tokio::main]
async fn main() {
    let detections = sample_detections();
    if let Some(first) = detections.first() {
        println!(
            "Monitoring {} detections at {} MHz",
            detections.len(),
            first.freq_mhz
        );
    }

    // ---- Manual graph: build, compile, step ----
    let mut builder = TFlowBuilder::new();
    builder.timestamp(|x: &Detection| x.ts);
    let snr = builder.prop(|x| x.snr);
    let sma = snr.sma(3usize);
    let output_ids = sma.output_ids();
    let nodes = builder.into_nodes();
    let mut graph: CompiledGraph<Detection, f64> =
        CompiledGraph::compile(Arc::new(|x: &Detection| x.ts), nodes, output_ids);

    let mut outputs = Vec::new();
    for record in &detections {
        if let Some(item) = graph.step(record) {
            outputs.push(item.value);
        }
    }
    print_summary("Manual graph SNR SMA(3)", &outputs);

    // ---- State summary ----
    let summary = graph.state_summary();
    assert!(summary.records_seen > 0);
    assert!(summary.node_count > 0);

    // ---- Graph plan ----
    let plan = graph.graph_plan();
    assert!(plan.node_count > 0);

    // ---- Snapshot ----
    let Ok(snapshot) = graph.snapshot() else {
        return;
    };
    assert!(!snapshot.data.is_empty());

    // ---- Restore ----
    let mut builder2 = TFlowBuilder::new();
    builder2.timestamp(|x: &Detection| x.ts);
    let snr2 = builder2.prop(|x| x.snr);
    let sma2 = snr2.sma(3usize);
    let output_ids2 = sma2.output_ids();
    let nodes2 = builder2.into_nodes();
    let mut graph2: CompiledGraph<Detection, f64> =
        CompiledGraph::compile(Arc::new(|x: &Detection| x.ts), nodes2, output_ids2);
    if let Err(e) = graph2.restore(&snapshot) {
        #[allow(clippy::print_stderr)] // example: stderr is fine for demo output
        {
            eprintln!("restore failed: {e}");
        }
    }

    // ---- Keyed execution (per receiver channel) ----
    let keyed_detections = vec![
        KeyedDetection {
            channel: "VHF".into(),
            ts: 1000,
            snr: 10.0,
        },
        KeyedDetection {
            channel: "UHF".into(),
            ts: 1000,
            snr: 20.0,
        },
        KeyedDetection {
            channel: "VHF".into(),
            ts: 2000,
            snr: 11.0,
        },
        KeyedDetection {
            channel: "UHF".into(),
            ts: 2000,
            snr: 22.0,
        },
        KeyedDetection {
            channel: "VHF".into(),
            ts: 3000,
            snr: 9.0,
        },
        KeyedDetection {
            channel: "UHF".into(),
            ts: 3000,
            snr: 18.0,
        },
    ];

    let results: Vec<_> = keyed_detections
        .into_iter()
        .tflo_keyed(
            |d| d.channel.clone(),
            OutOfOrderPolicy::Error,
            |t| {
                t.timestamp(|x: &KeyedDetection| x.ts);
                let snr = t.prop(|x: &KeyedDetection| x.snr);
                snr.sma(2_u64.secs())
            },
        )
        .collect();
    assert!(results.len() == 6);

    // ---- Async stream ----
    use tokio_stream::StreamExt as _;

    let async_results: Vec<f64> = tokio_stream::iter(detections.clone())
        .tflo(|t| {
            t.timestamp(|x| x.ts);
            let snr = t.prop(|x| x.snr);
            snr.sma(3usize)
        })
        .collect()
        .await;

    print_summary("Async stream SNR SMA(3)", &async_results);

    // ---- Async enrichment stream (.tflo_with()) ----
    let async_enriched: Vec<(Detection, f64)> = tokio_stream::iter(detections)
        .tflo_with(|t| {
            t.timestamp(|x| x.ts);
            let snr = t.prop(|x| x.snr);
            snr.sma(3usize)
        })
        .collect()
        .await;

    assert_eq!(async_enriched.len(), 10);

    println!("records_seen={}", summary.records_seen);
    println!("node_count={}", plan.node_count);
    println!("snapshot_size={} bytes", snapshot.data.len());
    println!("keyed_results={}", results.len());
    println!("async_results={}", async_results.len());
    println!("async_enriched={}", async_enriched.len());
}

#[derive(Clone)]
struct KeyedDetection {
    channel: String,
    ts: i64,
    snr: f64,
}
