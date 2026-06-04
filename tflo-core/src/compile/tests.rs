use super::*;
use crate::builder::TFlowBuilder;
use crate::pipeline::Sequenced;

#[derive(Clone, Debug)]
struct TestRecord {
    ts: i64,
    value: f64,
}

/// Simple graph that just returns the raw value (no windowing)
fn make_graph_passthrough() -> CompiledGraph<TestRecord, f64, Timestamped> {
    let mut builder = TFlowBuilder::<TestRecord>::new();
    let _ = builder.timestamp(|r| r.ts);
    let v = builder.prop(|r| r.value);
    let output_ids = vec![v.id];
    let nodes = builder.into_nodes();
    CompiledGraph::compile(Arc::new(|r: &TestRecord| r.ts), nodes, output_ids)
}

/// Graph with a map for testing — doubles the value.
#[allow(dead_code)]
fn make_graph_double() -> CompiledGraph<TestRecord, f64, Timestamped> {
    let mut builder = TFlowBuilder::<TestRecord>::new();
    let _ = builder.timestamp(|r| r.ts);
    let v = builder.prop(|r| r.value);
    let doubled = v.map_f64(|x| x * 2.0);
    let output_ids = vec![doubled.id];
    let nodes = builder.into_nodes();
    CompiledGraph::compile(Arc::new(|r: &TestRecord| r.ts), nodes, output_ids)
}

// ========================================================================
// NODE-ID OFFSET / output_ids TRACKING TESTS
// ========================================================================

/// When two graphs are zipped,
/// `CompiledGraph` shall offset the second graph's node IDs,
/// So that there are no ID collisions,
/// And the combined `output_ids` correctly reference both outputs.
#[test]
fn test_fm6_zip_id_offset() {
    let g1 = make_graph_passthrough();
    let g2 = make_graph_passthrough();

    let initial_max_id = g1.max_node_id();

    let combined = g1.zip(g2);

    // Combined should have output_ids from both graphs
    assert_eq!(combined.output_ids.len(), 2);

    // Second output ID should be offset
    assert!(combined.output_ids[1].0 > initial_max_id);
}

/// When a graph is mapped,
/// `CompiledGraph` shall create a new composition node with a new ID,
/// So that the `output_ids` correctly reference the mapped output,
/// And the original nodes remain unchanged.
#[test]
fn test_fm6_map_id_tracking() {
    let g = make_graph_passthrough();
    let initial_output_ids = g.output_ids.clone();

    let mapped = g.map(|x| x * 2.0);

    // Mapped should have a single output ID
    assert_eq!(mapped.output_ids.len(), 1);

    // The new ID should be different from the original
    assert_ne!(mapped.output_ids[0], initial_output_ids[0]);

    // New ID should be greater (offset from max)
    assert!(mapped.output_ids[0].0 > initial_output_ids[0].0);
}

/// When a zipped graph is reduced,
/// `CompiledGraph` shall correctly track the new single output ID,
/// So that extraction works correctly after reduction.
#[test]
fn test_fm6_reduce_id_tracking() {
    let g1 = make_graph_passthrough();
    let g2 = make_graph_passthrough();

    let combined = g1.zip(g2);
    let combined_ids = combined.output_ids.clone();

    let reduced = combined.reduce(|a, b| a + b);

    // Reduced should have a single output ID
    assert_eq!(reduced.output_ids.len(), 1);

    // The new ID should be different from both original IDs
    assert_ne!(reduced.output_ids[0], combined_ids[0]);
    assert_ne!(reduced.output_ids[0], combined_ids[1]);
}

// ========================================================================
// PIPELINE CONTEXT AND ITEM TESTS
// ========================================================================

/// When `step()` is called,
/// `CompiledGraph` shall return a `PipelineItem` with correct context,
/// So that timestamp information flows through the pipeline.
#[test]
fn test_step_returns_pipeline_item() {
    let mut g = make_graph_passthrough();

    let record = TestRecord {
        ts: 1000,
        value: 50.0,
    };

    let result = g.step(&record);
    assert!(result.is_some());

    let item = result.unwrap();
    assert_eq!(item.ctx, 1000); // Timestamp preserved
    assert_eq!(item.value, 50.0); // Raw value passed through
}

/// When `step_value()` is called,
/// `CompiledGraph` shall return just the value without context,
/// So that users who don't need context can get values directly.
#[test]
fn test_step_value_returns_raw_value() {
    let mut g = make_graph_passthrough();

    let record = TestRecord {
        ts: 1000,
        value: 50.0,
    };

    let result = g.step_value(&record);
    assert_eq!(result, Some(50.0));
}

/// When `with_context()` is called,
/// `CompiledGraph` shall change the context type,
/// So that pipelines can switch between time-based and sequence-based.
#[test]
fn test_with_context_changes_type() {
    let g: CompiledGraph<TestRecord, f64, Timestamped> = make_graph_passthrough();
    let g_seq: CompiledGraph<TestRecord, f64, Sequenced> = g.with_context();

    // The graph should compile and work the same
    let mut g_seq = g_seq;
    let record = TestRecord {
        ts: 1000,
        value: 50.0,
    };

    let result = g_seq.step(&record);
    assert!(result.is_some());
    // Context is now Sequenced
    let _seq_num = result.unwrap().ctx;
}

// ========================================================================
// COMPOSITION OPERATOR TESTS
// ========================================================================

/// When map is applied,
/// `CompiledGraph` shall transform outputs while preserving context,
/// So that timestamps flow through transformations.
#[test]
fn test_map_preserves_context() {
    let g = make_graph_passthrough();
    let mut mapped = g.map(|x| x * 2.0);

    let record = TestRecord {
        ts: 2000,
        value: 25.0,
    };

    let result = mapped.step(&record);
    assert!(result.is_some());

    let item = result.unwrap();
    assert_eq!(item.ctx, 2000); // Context preserved
    assert_eq!(item.value, 50.0); // 25 * 2 = 50
}

/// When filter is applied with passing predicate,
/// `CompiledGraph` shall return Some(value),
/// So that matching values are preserved.
#[test]
fn test_filter_passes() {
    let g = make_graph_passthrough();
    let mut filtered = g.filter(|&x| x > 10.0);

    let record = TestRecord {
        ts: 1000,
        value: 50.0,
    };

    let result = filtered.step(&record);
    assert!(result.is_some());
    assert_eq!(result.unwrap().value, Some(50.0));
}

/// When filter is applied with failing predicate,
/// `CompiledGraph` shall return Some(None),
/// So that non-matching values are filtered but context is preserved.
#[test]
fn test_filter_rejects() {
    let g = make_graph_passthrough();
    let mut filtered = g.filter(|&x| x > 100.0);

    let record = TestRecord {
        ts: 1000,
        value: 50.0,
    };

    let result = filtered.step(&record);
    assert!(result.is_some());
    assert_eq!(result.unwrap().value, None);
}

/// When fold is applied,
/// `CompiledGraph` shall accumulate values with preserved context,
/// So that running totals can be computed.
#[test]
fn test_fold_accumulates() {
    let g = make_graph_passthrough();
    let mut folded = g.fold(0.0, |acc, x| acc + x);

    let records = vec![
        TestRecord {
            ts: 1000,
            value: 10.0,
        },
        TestRecord {
            ts: 2000,
            value: 20.0,
        },
        TestRecord {
            ts: 3000,
            value: 30.0,
        },
    ];

    let mut results = Vec::new();
    for r in &records {
        if let Some(item) = folded.step(r) {
            results.push(item.value);
        }
    }

    // Cumulative sums: 10, 30, 60
    assert_eq!(results, vec![10.0, 30.0, 60.0]);
}

/// When zip is applied to two graphs,
/// `CompiledGraph` shall combine their outputs into a tuple,
/// So that multiple computations can be performed in parallel.
#[test]
fn test_zip_combines_outputs() {
    let g1 = make_graph_passthrough();
    let g2 = make_graph_passthrough();
    let mut combined = g1.zip(g2);

    let record = TestRecord {
        ts: 1000,
        value: 50.0,
    };

    let result = combined.step(&record);
    assert!(result.is_some());

    let item = result.unwrap();
    assert_eq!(item.value, (50.0, 50.0)); // Both values
    assert_eq!(item.ctx, 1000); // Context preserved
}

/// When reduce is applied to a zipped graph,
/// `CompiledGraph` shall collapse the tuple to a single value,
/// So that combined computations can produce scalar results.
#[test]
fn test_reduce_collapses_tuple() {
    let g1 = make_graph_passthrough();
    let g2 = make_graph_passthrough();
    let mut reduced = g1.zip(g2).reduce(|a, b| a + b);

    let record = TestRecord {
        ts: 1000,
        value: 25.0,
    };

    let result = reduced.step(&record);
    assert!(result.is_some());

    let item = result.unwrap();
    assert_eq!(item.value, 50.0); // 25 + 25
}

// ========================================================================
// SCAN / SCAN2 SNAPSHOT ROUND-TRIP TESTS
// ========================================================================

/// Build a graph whose single output is a checkpointed cumulative-sum scan
/// over `value`. Used by the scan snapshot round-trip test.
fn make_graph_scan_cumsum() -> CompiledGraph<TestRecord, f64, Timestamped> {
    let mut builder = TFlowBuilder::<TestRecord>::new();
    let _ = builder.timestamp(|r| r.ts);
    let cumsum = builder.prop(|r| r.value).scan_f64_checkpointed(
        || 0.0_f64,
        |s, x| {
            *s += x;
            *s
        },
    );
    let output_ids = vec![cumsum.id];
    let nodes = builder.into_nodes();
    CompiledGraph::compile(Arc::new(|r: &TestRecord| r.ts), nodes, output_ids)
}

/// Build a graph whose single output is a checkpointed cumulative
/// dollar-volume scan2 over `(value, value*2)`. The accumulator is a
/// 2-tuple `(count, total)` to exercise non-scalar scan state.
fn make_graph_scan2_cumvol() -> CompiledGraph<TestRecord, f64, Timestamped> {
    let mut builder = TFlowBuilder::<TestRecord>::new();
    let _ = builder.timestamp(|r| r.ts);
    let price = builder.prop(|r| r.value);
    let volume = builder.prop(|r| r.value * 2.0);
    let cumvol = price.scan2_f64_checkpointed(
        &volume,
        || (0_u64, 0.0_f64),
        |s, a, b| {
            s.0 += 1;
            s.1 += a * b;
            s.1
        },
    );
    let output_ids = vec![cumvol.id];
    let nodes = builder.into_nodes();
    CompiledGraph::compile(Arc::new(|r: &TestRecord| r.ts), nodes, output_ids)
}

fn rec(ts: i64, value: f64) -> TestRecord {
    TestRecord { ts, value }
}

/// When a graph containing a checkpointed `scan` node is stepped, snapshotted,
/// and restored into a freshly-compiled identical graph,
/// `CompiledGraph` shall round-trip the scan accumulator,
/// So that feeding further records to the restored graph yields identical
/// outputs to feeding them to the original — enabling state-as-of-T rollback.
#[test]
fn scan_snapshot_round_trips_accumulator() {
    let mut original = make_graph_scan_cumsum();

    // Warm up with three records: cumulative sums 10, 30, 60.
    let warmup = [rec(1, 10.0), rec(2, 20.0), rec(3, 30.0)];
    let mut pre: Vec<f64> = Vec::new();
    for r in &warmup {
        if let Some(item) = original.step(r) {
            pre.push(item.value);
        }
    }
    assert_eq!(pre, vec![10.0, 30.0, 60.0]);

    // Capture the snapshot mid-stream (accumulator == 60).
    let snap = original.snapshot().expect("scan snapshot should succeed");

    // Restore into a fresh, identical graph.
    let mut restored = make_graph_scan_cumsum();
    restored.restore(&snap).expect("restore should succeed");

    // Feed MORE records to both; outputs must match exactly.
    let more = [rec(4, 40.0), rec(5, 50.0)];
    let mut orig_out: Vec<f64> = Vec::new();
    let mut rest_out: Vec<f64> = Vec::new();
    for r in &more {
        if let Some(item) = original.step(r) {
            orig_out.push(item.value);
        }
        if let Some(item) = restored.step(r) {
            rest_out.push(item.value);
        }
    }

    // Original continues 100, 150 (60+40, 100+50).
    assert_eq!(orig_out, vec![100.0, 150.0]);
    // Restored, having recovered the accumulator==60, matches exactly.
    assert_eq!(rest_out, orig_out);
}

/// Same round-trip guarantee for a checkpointed `scan2` node whose
/// accumulator is a non-scalar `(u64, f64)` tuple.
#[test]
fn scan2_snapshot_round_trips_accumulator() {
    let mut original = make_graph_scan2_cumvol();

    // value*value*2 cumulative: 10*20=200; +20*40=1000; +30*60=2800.
    let warmup = [rec(1, 10.0), rec(2, 20.0), rec(3, 30.0)];
    let mut pre: Vec<f64> = Vec::new();
    for r in &warmup {
        if let Some(item) = original.step(r) {
            pre.push(item.value);
        }
    }
    assert_eq!(pre, vec![200.0, 1000.0, 2800.0]);

    let snap = original.snapshot().expect("scan2 snapshot should succeed");

    let mut restored = make_graph_scan2_cumvol();
    restored.restore(&snap).expect("restore should succeed");

    let more = [rec(4, 40.0), rec(5, 50.0)];
    let mut orig_out: Vec<f64> = Vec::new();
    let mut rest_out: Vec<f64> = Vec::new();
    for r in &more {
        if let Some(item) = original.step(r) {
            orig_out.push(item.value);
        }
        if let Some(item) = restored.step(r) {
            rest_out.push(item.value);
        }
    }

    // +40*80=3200 -> 6000; +50*100=5000 -> 11000.
    assert_eq!(orig_out, vec![6000.0, 11000.0]);
    assert_eq!(rest_out, orig_out);
}
