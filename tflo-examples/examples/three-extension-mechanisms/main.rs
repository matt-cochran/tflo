use tflo_core::custom_node::require;
use tflo_core::prelude::*;
use tflo_examples::*;

// ---- Rust native: a custom plugin node via the `CustomNode` trait ----
/// Passes the input through only when it clears a threshold; emits NaN otherwise.
#[derive(Debug)]
struct ScoreFilter {
    threshold: f64,
}

impl ScoreFilter {
    fn new(threshold: f64) -> Self {
        Self { threshold }
    }
}

impl CustomNode for ScoreFilter {
    fn eval(&mut self, inputs: &[Computed]) -> Computed {
        let v = require(inputs, 0)?;
        if v > self.threshold {
            Ok(v)
        } else {
            Err(Absent::FilteredOut)
        }
    }

    fn name(&self) -> &str {
        "score_filter"
    }
}

/// A network intrusion alert: a timestamp and a threat score (0-100).
#[derive(Clone, Debug)]
struct Alert {
    ts: i64,
    score: f64,
}

impl Alert {
    fn new(ts: i64, score: f64) -> Self {
        Self { ts, score }
    }
}

/// Sample intrusion alerts from a network IDS.
fn sample_alerts() -> Vec<Alert> {
    vec![
        Alert::new(1000, 40.0),
        Alert::new(2000, 55.0),
        Alert::new(3000, 38.0),
        Alert::new(4000, 62.0),
        Alert::new(5000, 71.0),
        Alert::new(6000, 84.0),
        Alert::new(7000, 79.0),
        Alert::new(8000, 91.0),
        Alert::new(9000, 95.0),
        Alert::new(10000, 88.0),
    ]
}

fn main() {
    let alerts = sample_alerts();

    println!("=== Three extension mechanisms ===");
    println!();
    println!("tflo provides three scripting/rule extensions plus a native path:");
    println!("  CEL   - Common Expression Language (boolean expressions)");
    println!("  Rego  - OPA policy language (multi-condition policy)");
    println!("  Rhai  - Embedded scripting (transformations)");
    println!("  Rust native - custom graph node via the CustomNode trait");
    println!();

    // ---- CEL (conceptual) ----
    println!("--- CEL: Boolean expressions ---");
    println!("CEL excels at threshold checks and routing rules:");
    println!("  let filtered = alerts.into_iter()");
    println!("      .cel_filter(\"score > 80.0 && src_internal == false\")");
    println!("      .collect::<Vec<_>>();");
    println!("Supports YAML-based rule configuration with hot reload.");
    println!();

    // ---- Rego (conceptual) ----
    println!("--- Rego: Policy decisions ---");
    println!("Rego handles multi-condition policies with external data:");
    println!("  engine.add_policy(\"intrusion\", r#\"");
    println!("      package intrusion");
    println!("      block {{ input.score > 80.0 not trusted_host }}");
    println!("  \"#)?;");
    println!("Supports reference data (allowlists, roles, asset tables).");
    println!();

    // ---- Rhai (conceptual) ----
    println!("--- Rhai: Scripting ---");
    println!("Rhai provides full scripting for transformations:");
    println!("  let weighted: Vec<Dynamic> = alerts.into_iter()");
    println!("      .rhai_map(\"score * asset_value * 0.01\")");
    println!("      .collect();");
    println!("Supports custom registered Rust functions callable from scripts.");
    println!();

    // ---- Rust native: custom graph nodes ----
    println!("--- Rust native: the CustomNode trait ---");
    println!("For stable, performance-sensitive logic:");
    println!("  - Full Rust semantics (type safety, pattern matching)");
    println!("  - A real stateful node compiled into the graph");
    println!("  - Attached with `Comp::custom_node` / `Comp::custom_node1`");
    println!();

    // Demonstrate a real custom plugin node compiled into the graph.
    let high_severity: Vec<f64> = alerts
        .clone()
        .into_iter()
        .tflo(|t| {
            t.timestamp(|x: &Alert| x.ts);
            let score = t.prop(|x: &Alert| x.score);
            score.custom_node1(|| ScoreFilter::new(80.0))
        })
        .collect();
    print_summary("Rust native: custom node (CustomNode)", &high_severity);

    // Closures still cover simpler stateless cases.
    let clamped: Vec<f64> = alerts
        .into_iter()
        .tflo(|t| {
            t.timestamp(|x: &Alert| x.ts);
            let score = t.prop(|x: &Alert| x.score);
            score.map_f64(|x| x.max(0.0))
        })
        .collect();
    print_summary("Rust native: clamp via map_f64", &clamped);

    // ---- Decision guide ----
    println!();
    println!("--- Decision guide ---");
    println!("| Need                              | Use         |");
    println!("|-----------------------------------|-------------|");
    println!("| Boolean check (field > threshold) | CEL         |");
    println!("| Policy with context/rules         | Rego        |");
    println!("| Transform, enrichment             | Rhai        |");
    println!("| Stable, perf-sensitive logic      | CustomNode  |");
}
