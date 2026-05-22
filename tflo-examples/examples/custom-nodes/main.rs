use std::collections::VecDeque;
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
            buffer: VecDeque::with_capacity(period + 1),
        }
    }
}

impl CustomNode for RateOfChange {
    fn eval(&mut self, inputs: &[f64]) -> f64 {
        let current = inputs.first().copied().unwrap_or(f64::NAN);
        self.buffer.push_back(current);

        if self.buffer.len() > self.period + 1 {
            self.buffer.pop_front();
        }
        if self.buffer.len() < self.period + 1 {
            return f64::NAN; // still warming up
        }

        let n_periods_ago = self.buffer.front().copied().unwrap_or(current);
        if n_periods_ago == 0.0 {
            return f64::NAN;
        }
        (current - n_periods_ago) / n_periods_ago
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
    fn new(threshold_db: f64) -> Self {
        Self { threshold_db }
    }
}

impl CustomNode for SnrGate {
    fn eval(&mut self, inputs: &[f64]) -> f64 {
        let v = inputs.first().copied().unwrap_or(f64::NAN);
        if v > self.threshold_db { v } else { f64::NAN }
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
    fn new(ts: i64, snr_db: f64) -> Self {
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
    print_summary("ROC(5) via custom CustomNode", &roc);

    // SNR gate as a custom plugin node.
    let gated: Vec<f64> = detections
        .clone()
        .into_iter()
        .tflo(|t| {
            t.timestamp(|x| x.ts);
            t.prop(|x| x.snr_db).custom_node1(|| SnrGate::new(12.0))
        })
        .collect();
    print_summary("SNR gate (threshold 12.0 dB) via custom CustomNode", &gated);

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
    println!("`CustomNode` plugs a stateful Rust node directly into the graph;");
    println!("`map_f64` / `scan_f64` cover simpler stateless and closure cases.");
}
