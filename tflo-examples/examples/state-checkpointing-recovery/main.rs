use std::sync::Arc;
use tflo_core::builder::Compile;
use tflo_core::compile::{CompiledGraph, StepResult};
use tflo_core::prelude::*;
use tflo_examples::*;

/// A per-host network traffic sample: a timestamp, the host that produced
/// it, and the observed packet rate (packets per second).
#[derive(Clone, Debug)]
struct HostSample {
    ts: i64,
    host_id: String,
    packets_per_sec: f64,
}

impl HostSample {
    fn new(ts: i64, host_id: &str, packets_per_sec: f64) -> Self {
        Self {
            ts,
            host_id: host_id.to_string(),
            packets_per_sec,
        }
    }
}

/// Sample traffic measurements from one monitored host on the network.
fn sample_host_traffic() -> Vec<HostSample> {
    vec![
        HostSample::new(1000, "host-01", 1200.0),
        HostSample::new(2000, "host-01", 1280.0),
        HostSample::new(3000, "host-01", 1150.0),
        HostSample::new(4000, "host-01", 1340.0),
        HostSample::new(5000, "host-01", 1410.0),
        HostSample::new(6000, "host-01", 1500.0),
        HostSample::new(7000, "host-01", 1460.0),
        HostSample::new(8000, "host-01", 1520.0),
        HostSample::new(9000, "host-01", 1610.0),
        HostSample::new(10000, "host-01", 1580.0),
    ]
}

fn main() {
    let traffic = sample_host_traffic();

    // ---- Build a graph ----
    let mut builder = TFlowBuilder::new();
    builder.timestamp(|x: &HostSample| x.ts);
    let packets = builder.prop(|x| x.packets_per_sec);
    let sma = packets.sma(3usize);
    let nodes = builder.into_nodes();
    let mut graph: CompiledGraph<HostSample, f64> =
        CompiledGraph::compile(Arc::new(|x: &HostSample| x.ts), nodes, sma.output_ids());

    // ---- Process some data ----
    let mut sma_outputs = Vec::new();
    println!("=== Processing first 5 records ===");
    for record in traffic.iter().take(5) {
        let summary = graph.state_summary();
        let output = graph.step_with_status(record);
        if let StepResult::Ready(item) = &output {
            sma_outputs.push(item.value);
        }
        println!(
            "  host={} ts={} → records_seen={} warmed_up={} output={:?}",
            record.host_id, record.ts, summary.records_seen, summary.is_warmed_up, output
        );
    }

    print_summary("Pre-checkpoint SMA(3) packets/sec", &sma_outputs);

    // ---- Snapshot ----
    println!("\n=== Taking snapshot ===");
    let snapshot = match graph.snapshot() {
        Ok(s) => s,
        Err(e) => {
            println!("  snapshot failed: {e}");
            return;
        }
    };
    println!(
        "  snapshot metadata: version={}, timestamp_ms={}",
        snapshot.metadata.version, snapshot.metadata.timestamp_ms
    );
    println!("  snapshot data size: {} bytes", snapshot.data.len());

    // ---- Restore (simulate restart) ----
    println!("\n=== Restoring from snapshot ===");
    let result = graph.restore(&snapshot);
    match result {
        Ok(()) => {
            let summary = graph.state_summary();
            println!("  Restore successful!");
            println!(
                "  records_seen={} is_warmed_up={}",
                summary.records_seen, summary.is_warmed_up
            );
        }
        Err(e) => println!("  Restore result: {e}"),
    }

    // ---- Process remaining data ----
    println!("\n=== Processing remaining records after restore ===");
    for record in traffic.iter().skip(5) {
        let summary = graph.state_summary();
        let output = graph.step_with_status(record);
        println!(
            "  ts={} → records_seen={} output={:?}",
            record.ts, summary.records_seen, output
        );
    }

    // ---- Conceptual: StateStore and SnapshotCodec ----
    println!("\n--- StateStore and SnapshotCodec (conceptual) ---");
    println!("StateStore trait: save/load/list_keys for snapshot persistence");
    println!("  - tflo-state-files: local file-based storage");
    println!("  - tflo-state-s3: S3-compatible object storage");
    println!("SnapshotCodec trait: encode/decode for snapshot serialization");
    println!("  - Implement for JSON, protobuf, bincode, etc.");
    println!("Checkpoint: pairs StateSnapshot with Kafka offset for partition-aware recovery");

    // ---- Keyed execution (conceptual) ----
    println!("\n--- Keyed execution (conceptual) ---");
    println!("tflo_keyed() partitions records by host_id, runs per-host graphs:");
    println!("  - Each monitored host gets its own graph state");
    println!("  - OutOfOrderPolicy handles late arrivals");
    println!("  - Kafka partitions map naturally to tflo_keyed keys");
}
