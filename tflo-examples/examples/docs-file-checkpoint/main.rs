//! File-based checkpoint/restore round-trip.
//!
//! Demonstrates: `snapshot()`, `restore()`, `FileStateStore` save/load.
//!
//! Run: cargo run --example docs-file-checkpoint

use std::sync::Arc;
use tflo_core::builder::Compile;
use tflo_core::compile::CompiledGraph;
use tflo_core::keyed::{SnapshotMetadata, StateSnapshot};
use tflo_core::prelude::*;
use tflo_examples::*;
use tflo_ops::prelude::*;
use tflo_state_files::FileStateStore;

/// A telemetry sample from a CNC machine on the factory floor:
/// a timestamp and the measured spindle speed in RPM.
#[derive(Clone, Debug)]
struct MachineSample {
    ts: i64,
    spindle_rpm: f64,
}

impl MachineSample {
    const fn new(ts: i64, spindle_rpm: f64) -> Self {
        Self { ts, spindle_rpm }
    }
}

/// Sample spindle-speed telemetry from a machining-center PLC.
fn sample_machine_telemetry() -> Vec<MachineSample> {
    vec![
        MachineSample::new(1000, 11_800.0),
        MachineSample::new(2000, 12_050.0),
        MachineSample::new(3000, 11_950.0),
        MachineSample::new(4000, 12_200.0),
        MachineSample::new(5000, 12_400.0),
        MachineSample::new(6000, 12_100.0),
        MachineSample::new(7000, 11_900.0),
        MachineSample::new(8000, 12_300.0),
    ]
}

fn main() -> Result<(), String> {
    // ---- Build and compile the graph ----
    let mut builder = TFlowBuilder::new();
    builder.timestamp(|x: &MachineSample| x.ts);
    let spindle_rpm = builder.prop(|x: &MachineSample| x.spindle_rpm);
    let sma = spindle_rpm.sma(3usize);
    let output_ids = sma.output_ids();
    let nodes = builder.into_nodes();
    let mut graph: CompiledGraph<MachineSample, f64> =
        CompiledGraph::compile(Arc::new(|x: &MachineSample| x.ts), nodes, output_ids);

    // ---- Process some records ----
    let telemetry = sample_machine_telemetry();
    let mut rpm_sma = Vec::new();
    for record in telemetry.iter().take(5) {
        if let Some(item) = graph.step(record) {
            rpm_sma.push(item.value);
        }
    }
    let summary = graph.state_summary();
    println!("After 5 records: records_seen={}", summary.records_seen);
    print_summary("Spindle RPM SMA(3)", &rpm_sma);

    // ---- Take a snapshot ----
    let snapshot = graph.snapshot().map_err(|e| e.to_string())?;
    println!(
        "Snapshot: {} bytes, version={}",
        snapshot.data.len(),
        snapshot.metadata.version
    );

    // ---- Persist to disk via FileStateStore ----
    //
    // Phase 1 added an `AsyncStateStore` impl alongside the legacy sync
    // `StateStore`; the example still uses the sync path to keep the
    // demo single-threaded. Disambiguate explicitly because both traits
    // are now in scope.
    let tmp_dir = std::env::temp_dir().join("tflo-checkpoint-demo");
    let store = FileStateStore::new(&tmp_dir)?;
    <FileStateStore as tflo_core::keyed::StateStore>::save(&store, b"spindle-graph", &snapshot)?;
    println!("Saved snapshot to {}", tmp_dir.display());

    // ---- Load it back ----
    let loaded =
        <FileStateStore as tflo_core::keyed::StateStore>::load(&store, b"spindle-graph")?
            .ok_or("snapshot not found after save")?;
    println!(
        "Loaded snapshot: {} bytes, version={}",
        loaded.data.len(),
        loaded.metadata.version
    );

    // ---- Restore into a fresh graph ----
    let mut builder2 = TFlowBuilder::new();
    builder2.timestamp(|x: &MachineSample| x.ts);
    let spindle_rpm2 = builder2.prop(|x: &MachineSample| x.spindle_rpm);
    let sma2 = spindle_rpm2.sma(3usize);
    let output_ids2 = sma2.output_ids();
    let nodes2 = builder2.into_nodes();
    let mut graph2: CompiledGraph<MachineSample, f64> =
        CompiledGraph::compile(Arc::new(|x: &MachineSample| x.ts), nodes2, output_ids2);
    graph2.restore(&loaded).map_err(|e| e.to_string())?;

    let summary2 = graph2.state_summary();
    println!("Restored graph: records_seen={}", summary2.records_seen);
    assert_eq!(summary.records_seen, summary2.records_seen);

    // ---- Direct StateSnapshot construction (to show the struct API) ----
    let _manual = StateSnapshot {
        data: vec![],
        metadata: SnapshotMetadata {
            key: Some(b"spindle-graph".to_vec()),
            timestamp_ms: 0,
            version: 1,
            topology_fingerprint: None,
        },
    };

    // ---- List persisted keys ----
    let keys = <FileStateStore as tflo_core::keyed::StateStore>::list_keys(&store)?;
    println!("Keys in store: {}", keys.len());
    assert!(!keys.is_empty());

    println!("File checkpoint round-trip: OK");
    Ok(())
}
