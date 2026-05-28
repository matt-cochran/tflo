#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing
)] // SAFETY: test code, indexes into vecs of known size
//! Integration tests for `CompiledGraph::snapshot` / `restore`.
//!
//! Covers: checkpointable graphs round-trip exactly; non-checkpointable
//! graphs (`scan`, `fold`, a plugin node without `save`) are rejected; a
//! plugin operator that overrides `save`/`load` round-trips; and a topology
//! mismatch on restore is rejected.

use std::sync::Arc;
use tflo_core::builder::Compile;
use tflo_core::operator::{Operator, OperatorLoadError, require};
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

/// A simple passthrough graph — not checkpointable via snapshot (no state to save).
fn passthrough_graph() -> CompiledGraph<Tick, f64> {
    let mut b = TFlowBuilder::new();
    b.timestamp(|t: &Tick| t.ts);
    let v = b.prop(|t: &Tick| t.value);
    let doubled = v.map_f64(|x| x * 2.0);
    let output_ids = doubled.output_ids();
    CompiledGraph::compile(Arc::new(|t: &Tick| t.ts), b.into_nodes(), output_ids)
}

fn run(graph: &mut CompiledGraph<Tick, f64>, data: &[Tick]) -> Vec<Option<f64>> {
    data.iter()
        .map(|t| graph.step(t).map(|i| i.value))
        .collect()
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
    let folded = checkpointable_custom_graph().fold(0.0_f64, |acc, x| acc + x);
    let mut folded = folded;
    let _ = folded.step(&ticks()[0]);

    assert!(
        folded.snapshot().is_err(),
        "a fold composition node holds opaque accumulator state"
    );
}

/// A plugin operator with the default (non-checkpointable) `save`/`load`.
#[derive(Default)]
struct PlainSum {
    total: f64,
}

impl Operator for PlainSum {
    fn eval(&mut self, inputs: &[Computed], _ts: i64) -> NodeOutput {
        NodeOutput::computed(match require(inputs, 0) {
            Err(e) => Err(e),
            Ok(v) => {
                self.total += v;
                Ok(self.total)
            }
        })
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
        "a plugin operator that does not override save() is not checkpointable"
    );
}

/// A plugin operator that overrides `save`/`load` — it serializes its accumulator.
#[derive(Default)]
struct CheckpointableSum {
    total: f64,
}

impl Operator for CheckpointableSum {
    fn eval(&mut self, inputs: &[Computed], _ts: i64) -> NodeOutput {
        NodeOutput::computed(match require(inputs, 0) {
            Err(e) => Err(e),
            Ok(v) => {
                self.total += v;
                Ok(self.total)
            }
        })
    }
    fn name(&self) -> &str {
        "checkpointable_sum"
    }
    fn save(&self) -> Option<Vec<u8>> {
        Some(self.total.to_le_bytes().to_vec())
    }
    fn load(&mut self, bytes: &[u8]) -> Result<(), OperatorLoadError> {
        let arr: [u8; 8] = bytes
            .try_into()
            .map_err(|_| OperatorLoadError::new("expected 8 bytes"))?;
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
        .expect("a plugin operator overriding save() is checkpointable");

    let mut restored = checkpointable_custom_graph();
    restored.restore(&snap).expect("restore succeeds");
    let restored_out = run(&mut restored, &data[5..]);

    assert_eq!(restored_out, reference_out[5..]);
}

#[test]
fn restore_rejects_topology_mismatch() {
    let mut graph = checkpointable_custom_graph();
    let _ = run(&mut graph, &ticks());
    let snap = graph.snapshot().expect("checkpointable");

    // A graph with a different node count must reject the snapshot.
    // Use a passthrough (map_f64) graph — different topology.
    let mut other = passthrough_graph();
    assert!(
        other.restore(&snap).is_err(),
        "restoring into a structurally different graph must fail"
    );
}
