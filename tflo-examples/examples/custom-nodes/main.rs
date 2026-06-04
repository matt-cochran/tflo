use std::collections::VecDeque;
use tflo_core::operator::require;
use tflo_core::prelude::*;
use tflo_examples::*;

// ---- Rate of Change: a stateful custom plugin node ----
#[derive(Debug)]
struct RateOfChange {
    period: usize,
    buffer: VecDeque<f64>,
}

impl RateOfChange {
    fn new(period: usize) -> Self {
        Self {
            period,
            // SAFETY: period is a small ROC window (e.g. 5–50); +1 cannot overflow usize
            buffer: VecDeque::with_capacity(period.saturating_add(1)),
        }
    }
}

impl Operator for RateOfChange {
    fn eval(&mut self, inputs: &[Computed], _ts: i64) -> NodeOutput {
        let current = match require(inputs, 0) {
            Ok(v) => v,
            Err(e) => return NodeOutput::computed(Err(e)),
        };
        self.buffer.push_back(current);

        // SAFETY: period is a small ROC window; saturating add cannot overflow usize
        if self.buffer.len() > self.period.saturating_add(1) {
            self.buffer.pop_front();
        }
        if self.buffer.len() < self.period.saturating_add(1) {
            return NodeOutput::computed(Err(Absent::WarmingUp)); // still warming up
        }

        let n_periods_ago = self.buffer.front().copied().unwrap_or(current);
        if n_periods_ago == 0.0 {
            return NodeOutput::computed(Err(Absent::DivideByZero));
        }
        NodeOutput::computed(Ok((current - n_periods_ago) / n_periods_ago))
    }

    fn reset(&mut self) {
        self.buffer.clear();
    }

    fn name(&self) -> &str {
        "rate_of_change"
    }
}

// ---- SNR gate: a stateless custom plugin node ----
#[derive(Debug)]
struct SnrGate {
    threshold_db: f64,
}

impl SnrGate {
    const fn new(threshold_db: f64) -> Self {
        Self { threshold_db }
    }
}

impl Operator for SnrGate {
    fn eval(&mut self, inputs: &[Computed], _ts: i64) -> NodeOutput {
        let v = match require(inputs, 0) {
            Ok(v) => v,
            Err(e) => return NodeOutput::computed(Err(e)),
        };
        if v > self.threshold_db {
            NodeOutput::computed(Ok(v))
        } else {
            NodeOutput::computed(Err(Absent::FilteredOut))
        }
    }

    fn name(&self) -> &str {
        "snr_gate"
    }
}

/// A radio-frequency emitter detection: a timestamp and a signal-to-noise ratio in dB.
#[derive(Clone, Debug)]
struct Detection {
    ts: i64,
    snr_db: f64,
}

impl Detection {
    const fn new(ts: i64, snr_db: f64) -> Self {
        Self { ts, snr_db }
    }
}

/// Sample emitter detections from an RF spectrum monitor.
fn sample_detections() -> Vec<Detection> {
    vec![
        Detection::new(1000, 8.0),
        Detection::new(2000, 9.5),
        Detection::new(3000, 7.0),
        Detection::new(4000, 11.0),
        Detection::new(5000, 12.5),
        Detection::new(6000, 14.0),
        Detection::new(7000, 13.0),
        Detection::new(8000, 15.5),
        Detection::new(9000, 17.0),
        Detection::new(10000, 16.0),
    ]
}

fn main() {
    let detections = sample_detections();

    // Rate of Change as a real custom plugin node.
    let roc: Vec<f64> = detections
        .clone()
        .into_iter()
        .tflo(|t| {
            t.timestamp(|x| x.ts);
            t.prop(|x| x.snr_db).custom_node1(|| RateOfChange::new(5))
        })
        .collect();
    print_summary("ROC(5) via custom Operator", &roc);

    // SNR gate as a custom plugin node.
    let gated: Vec<f64> = detections
        .clone()
        .into_iter()
        .tflo(|t| {
            t.timestamp(|x| x.ts);
            t.prop(|x| x.snr_db).custom_node1(|| SnrGate::new(12.0))
        })
        .collect();
    print_summary("SNR gate (threshold 12.0 dB) via custom Operator", &gated);

    // Built-in functional primitives still cover simpler cases.
    println!("\n--- Functional primitives ---");
    let clamped: Vec<f64> = detections
        .clone()
        .into_iter()
        .tflo(|t| {
            t.timestamp(|x| x.ts);
            t.prop(|x| x.snr_db).map_f64(|x| x.max(0.0))
        })
        .collect();
    print_summary("Clamp via map_f64", &clamped);

    let custom_ema: Vec<f64> = detections
        .into_iter()
        .tflo(|t| {
            t.timestamp(|x| x.ts);
            t.prop(|x| x.snr_db).scan_f64(
                || 0.0_f64,
                |state, x| {
                    *state = 0.9 * *state + 0.1 * x;
                    *state
                },
            )
        })
        .collect();
    print_summary("Custom EMA(0.1) via scan_f64", &custom_ema);

    println!();
    println!("`Operator` plugs a stateful Rust node directly into the graph;");
    println!("`map_f64` / `scan_f64` cover simpler stateless and closure cases.");
}
