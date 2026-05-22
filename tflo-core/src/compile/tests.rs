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
/// CompiledGraph shall offset the second graph's node IDs,
/// So that there are no ID collisions,
/// And the combined output_ids correctly reference both outputs.
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
/// CompiledGraph shall create a new composition node with a new ID,
/// So that the output_ids correctly reference the mapped output,
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
/// CompiledGraph shall correctly track the new single output ID,
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

/// When step() is called,
/// CompiledGraph shall return a PipelineItem with correct context,
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

/// When step_value() is called,
/// CompiledGraph shall return just the value without context,
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

/// When with_context() is called,
/// CompiledGraph shall change the context type,
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
/// CompiledGraph shall transform outputs while preserving context,
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
/// CompiledGraph shall return Some(value),
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
/// CompiledGraph shall return Some(None),
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
/// CompiledGraph shall accumulate values with preserved context,
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
/// CompiledGraph shall combine their outputs into a tuple,
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
/// CompiledGraph shall collapse the tuple to a single value,
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
// TRIGGER RESULT HELPER TESTS
// ========================================================================

/// When GlitchResult helper methods are used,
/// the helpers shall correctly map results to f64 and ThresholdCrossEventMode,
/// So that trigger results can be easily converted.
#[test]
fn test_glitch_result_helpers() {
    assert!(GlitchResult::ValidPulse.is_valid_pulse());
    assert!(!GlitchResult::ValidPulse.is_rejected());
    assert!(GlitchResult::Rejected.is_rejected());
    assert_eq!(
        GlitchResult::ValidPulse.to_threshold_cross(),
        ThresholdCrossEventMode::Rising
    );
    assert_eq!(
        GlitchResult::Rejected.to_threshold_cross(),
        ThresholdCrossEventMode::Falling
    );
    assert_eq!(
        GlitchResult::NoTransition.to_threshold_cross(),
        ThresholdCrossEventMode::None
    );
}

/// When RuntResult helper methods are used,
/// the helpers shall correctly identify valid vs runt pulses.
#[test]
fn test_runt_result_helpers() {
    let valid = RuntResult::ValidPulse { peak: 100.0 };
    let runt = RuntResult::Runt { peak: 50.0 };

    assert!(valid.is_valid());
    assert!(!valid.is_runt());
    assert!(runt.is_runt());
    assert!(!runt.is_valid());
    assert_eq!(valid.peak(), 100.0);
    assert_eq!(runt.peak(), 50.0);
    assert_eq!(valid.to_threshold_cross(), ThresholdCrossEventMode::Rising);
    assert_eq!(runt.to_threshold_cross(), ThresholdCrossEventMode::Falling);
}

/// When PulseWidthResult helper methods are used,
/// the helpers shall correctly identify valid, too short, and too long pulses.
#[test]
fn test_pulse_width_result_helpers() {
    let valid = PulseWidthResult::Valid { width_ms: 100 };
    let short = PulseWidthResult::TooShort { width_ms: 10 };
    let long = PulseWidthResult::TooLong { width_ms: 1000 };

    assert!(valid.is_valid());
    assert!(short.is_too_short());
    assert!(long.is_too_long());
    assert_eq!(valid.width_ms(), 100);
    assert_eq!(short.width_ms(), 10);
    assert_eq!(valid.to_threshold_cross(), ThresholdCrossEventMode::Rising);
    assert_eq!(short.to_threshold_cross(), ThresholdCrossEventMode::Falling);
}

/// When WindowEvent helper methods are used,
/// the helpers shall correctly identify entry and exit events.
#[test]
fn test_window_event_helpers() {
    let entered = WindowEvent::EnteredWindow;
    let exited_low = WindowEvent::ExitedLow;
    let exited_high = WindowEvent::ExitedHigh;

    assert!(entered.is_entered());
    assert!(!entered.is_exited());
    assert!(exited_low.is_exited_low());
    assert!(exited_low.is_exited());
    assert!(exited_high.is_exited_high());
    assert!(exited_high.is_exited());
    assert_eq!(
        entered.to_threshold_cross(),
        ThresholdCrossEventMode::Rising
    );
    assert_eq!(
        exited_low.to_threshold_cross(),
        ThresholdCrossEventMode::Falling
    );
}
