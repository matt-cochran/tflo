#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
//! Integration tests for `CompiledGraph::snapshot` / `restore`.
//!
//! Covers: checkpointable graphs round-trip exactly; non-checkpointable
//! graphs (`scan`, `fold`, a custom node without `save`) are rejected; a
//! custom node that overrides `save`/`load` round-trips; and a topology
//! mismatch on restore is rejected.

use std::sync::Arc;
use tflo_core::builder::Compile;
use tflo_core::custom_node::{CustomNode, CustomNodeLoadError, require};
use tflo_core::prelude::*;

#[derive(Clone, Debug)]
struct Tick {
    ts: i64,
    value: f64,
}

fn ticks() -> Vec<Tick> {
    (0..12)
        .map(|i| Tick {
            ts: 1000 + i * 1000,
            value: 100.0 + (i as f64) * 1.5 - (i as f64 % 3.0),
        })
        .collect()
}

/// SMA(3) over a count window — a plain checkpointable graph.
fn sma_graph() -> CompiledGraph<Tick, f64> {
    let mut b = TFlowBuilder::new();
    b.timestamp(|t: &Tick| t.ts);
    let v = b.prop(|t: &Tick| t.value);
    let sma = v.sma(3usize);
    let output_ids = sma.output_ids();
    CompiledGraph::compile(Arc::new(|t: &Tick| t.ts), b.into_nodes(), output_ids)
}

/// A wider graph mixing several stateful node kinds.
fn mixed_graph() -> CompiledGraph<Tick, f64> {
    let mut b = TFlowBuilder::new();
    b.timestamp(|t: &Tick| t.ts);
    let v = b.prop(|t: &Tick| t.value);
    let sma = v.sma(4usize);
    let ema = v.ema(3usize);
    let combined = &sma + &ema;
    let output_ids = combined.output_ids();
    CompiledGraph::compile(Arc::new(|t: &Tick| t.ts), b.into_nodes(), output_ids)
}

fn run(graph: &mut CompiledGraph<Tick, f64>, data: &[Tick]) -> Vec<Option<f64>> {
    data.iter()
        .map(|t| graph.step(t).map(|i| i.value))
        .collect()
}

#[test]
fn snapshot_restore_roundtrip_sma() {
    let data = ticks();

    // Uninterrupted reference run.
    let mut reference = sma_graph();
    let reference_out = run(&mut reference, &data);

    // Checkpoint after the first 6 records.
    let mut graph = sma_graph();
    let _ = run(&mut graph, &data[..6]);
    let snap = graph.snapshot().expect("sma graph is checkpointable");

    // Restore into a fresh, identically-built graph and continue.
    let mut restored = sma_graph();
    restored.restore(&snap).expect("restore succeeds");
    let restored_out = run(&mut restored, &data[6..]);

    assert_eq!(
        restored_out,
        reference_out[6..],
        "restored continuation must match the uninterrupted run exactly"
    );
}

#[test]
fn snapshot_restore_roundtrip_mixed_state() {
    let data = ticks();

    let mut reference = mixed_graph();
    let reference_out = run(&mut reference, &data);

    let mut graph = mixed_graph();
    let _ = run(&mut graph, &data[..7]);
    let snap = graph.snapshot().expect("mixed graph is checkpointable");

    let mut restored = mixed_graph();
    restored.restore(&snap).expect("restore succeeds");
    let restored_out = run(&mut restored, &data[7..]);

    assert_eq!(restored_out, reference_out[7..]);
}

#[test]
fn snapshot_rejects_scan_graph() {
    let mut b = TFlowBuilder::new();
    b.timestamp(|t: &Tick| t.ts);
    let v = b.prop(|t: &Tick| t.value);
    let scanned = v.scan_f64(
        || 0.0_f64,
        |s, x| {
            *s += x;
            *s
        },
    );
    let output_ids = scanned.output_ids();
    let mut graph: CompiledGraph<Tick, f64> =
        CompiledGraph::compile(Arc::new(|t: &Tick| t.ts), b.into_nodes(), output_ids);
    let _ = run(&mut graph, &ticks());

    assert!(
        graph.snapshot().is_err(),
        "a scan node holds opaque closure state and must not be checkpointable"
    );
}

#[test]
fn snapshot_rejects_fold_graph() {
    let folded = sma_graph().fold(0.0_f64, |acc, x| acc + x);
    let mut folded = folded;
    let _ = folded.step(&ticks()[0]);

    assert!(
        folded.snapshot().is_err(),
        "a fold composition node holds opaque accumulator state"
    );
}

/// A custom node with the default (non-checkpointable) `save`/`load`.
#[derive(Default)]
struct PlainSum {
    total: f64,
}

impl CustomNode for PlainSum {
    fn eval(&mut self, inputs: &[Computed]) -> Computed {
        self.total += require(inputs, 0)?;
        Ok(self.total)
    }
    fn name(&self) -> &str {
        "plain_sum"
    }
}

#[test]
fn snapshot_rejects_default_custom_node() {
    let mut b = TFlowBuilder::new();
    b.timestamp(|t: &Tick| t.ts);
    let v = b.prop(|t: &Tick| t.value);
    let node = v.custom_node1(PlainSum::default);
    let output_ids = node.output_ids();
    let mut graph: CompiledGraph<Tick, f64> =
        CompiledGraph::compile(Arc::new(|t: &Tick| t.ts), b.into_nodes(), output_ids);
    let _ = run(&mut graph, &ticks());

    assert!(
        graph.snapshot().is_err(),
        "a custom node that does not override save() is not checkpointable"
    );
}

/// A custom node that overrides `save`/`load` — it serializes its accumulator.
#[derive(Default)]
struct CheckpointableSum {
    total: f64,
}

impl CustomNode for CheckpointableSum {
    fn eval(&mut self, inputs: &[Computed]) -> Computed {
        self.total += require(inputs, 0)?;
        Ok(self.total)
    }
    fn name(&self) -> &str {
        "checkpointable_sum"
    }
    fn save(&self) -> Option<Vec<u8>> {
        Some(self.total.to_le_bytes().to_vec())
    }
    fn load(&mut self, bytes: &[u8]) -> Result<(), CustomNodeLoadError> {
        let arr: [u8; 8] = bytes
            .try_into()
            .map_err(|_| CustomNodeLoadError::new("expected 8 bytes"))?;
        self.total = f64::from_le_bytes(arr);
        Ok(())
    }
}

fn checkpointable_custom_graph() -> CompiledGraph<Tick, f64> {
    let mut b = TFlowBuilder::new();
    b.timestamp(|t: &Tick| t.ts);
    let v = b.prop(|t: &Tick| t.value);
    let node = v.custom_node1(CheckpointableSum::default);
    let output_ids = node.output_ids();
    CompiledGraph::compile(Arc::new(|t: &Tick| t.ts), b.into_nodes(), output_ids)
}

#[test]
fn snapshot_restore_roundtrip_custom_node() {
    let data = ticks();

    let mut reference = checkpointable_custom_graph();
    let reference_out = run(&mut reference, &data);

    let mut graph = checkpointable_custom_graph();
    let _ = run(&mut graph, &data[..5]);
    let snap = graph
        .snapshot()
        .expect("a custom node overriding save() is checkpointable");

    let mut restored = checkpointable_custom_graph();
    restored.restore(&snap).expect("restore succeeds");
    let restored_out = run(&mut restored, &data[5..]);

    assert_eq!(restored_out, reference_out[5..]);
}

#[test]
fn restore_rejects_topology_mismatch() {
    let mut graph = sma_graph();
    let _ = run(&mut graph, &ticks());
    let snap = graph.snapshot().expect("checkpointable");

    // A graph with a different node count must reject the snapshot.
    let mut other = mixed_graph();
    assert!(
        other.restore(&snap).is_err(),
        "restoring into a structurally different graph must fail"
    );
}
