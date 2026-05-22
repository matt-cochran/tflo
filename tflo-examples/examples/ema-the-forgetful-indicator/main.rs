use tflo_core::prelude::*;
use tflo_examples::*;

/// A single web server request-latency measurement.
#[derive(Clone, Debug)]
struct Sample {
    /// Timestamp in milliseconds.
    ts: i64,
    /// Request latency in milliseconds.
    latency_ms: f64,
}

impl Sample {
    fn new(ts: i64, latency_ms: f64) -> Self {
        Self { ts, latency_ms }
    }
}

/// Per-second latency samples scraped from a web server, showing a
/// gradual slowdown as load builds up.
fn sample_latencies() -> Vec<Sample> {
    vec![
        Sample::new(1000, 100.0),
        Sample::new(2000, 101.0),
        Sample::new(3000, 99.0),
        Sample::new(4000, 102.0),
        Sample::new(5000, 103.0),
        Sample::new(6000, 105.0),
        Sample::new(7000, 104.0),
        Sample::new(8000, 106.0),
        Sample::new(9000, 108.0),
        Sample::new(10000, 107.0),
    ]
}

fn main() {
    let samples = sample_latencies();

    // ---- Count-based EMA ----
    let ema5: Vec<f64> = samples
        .clone()
        .into_iter()
        .tflo(|t| {
            t.timestamp(|x| x.ts);
            let latency = t.prop(|x| x.latency_ms);
            latency.ema(5usize)
        })
        .collect();
    print_summary("EMA(5) count-based", &ema5);

    let ema20: Vec<f64> = samples
        .clone()
        .into_iter()
        .tflo(|t| {
            t.timestamp(|x| x.ts);
            let latency = t.prop(|x| x.latency_ms);
            latency.ema(20usize)
        })
        .collect();
    print_summary("EMA(20) count-based", &ema20);

    // ---- Compare EMA vs SMA on same window ----
    println!("\n--- EMA vs SMA(5) comparison ---");
    let ema_vs_sma: Vec<(f64, f64)> = samples
        .clone()
        .into_iter()
        .tflo(|t| {
            t.timestamp(|x| x.ts);
            let latency = t.prop(|x| x.latency_ms);
            let ema = latency.ema(5usize);
            let sma = latency.sma(5usize);
            (ema, sma)
        })
        .collect();
    for (ts, (ema, sma)) in samples.iter().map(|t| t.ts).zip(&ema_vs_sma) {
        println!(
            "  ts={:>6} ema={:>.4} sma={:>.4} diff={:>.4}",
            ts,
            ema,
            sma,
            (ema - sma).abs()
        );
    }

    // ---- Time-based EMA ----
    let ema_time: Vec<f64> = samples
        .clone()
        .into_iter()
        .tflo(|t| {
            t.timestamp(|x| x.ts);
            let latency = t.prop(|x| x.latency_ms);
            latency.ema(5_u64.secs())
        })
        .collect();
    print_summary("EMA(5s) time-based", &ema_time);

    // ---- WindowSpec fluent ----
    let ws_ema: Vec<f64> = samples
        .clone()
        .into_iter()
        .tflo(|t| {
            t.timestamp(|x| x.ts);
            let latency = t.prop(|x| x.latency_ms);
            latency.over(20_u64.secs()).ema()
        })
        .collect();
    print_summary("EMA(20s) via WindowSpec", &ws_ema);

    // ---- Custom EMA via scan_f64 ----
    println!("\n--- Custom EMA with different alphas via scan_f64 ---");
    let custom_ema_fast: Vec<f64> = samples
        .clone()
        .into_iter()
        .tflo(|t| {
            t.timestamp(|x| x.ts);
            let latency = t.prop(|x| x.latency_ms);
            latency.scan_f64(
                || 0.0_f64,
                |state, x| {
                    *state = 0.3 * x + 0.7 * *state;
                    *state
                },
            )
        })
        .collect();
    print_summary("Custom EMA(α=0.3) via scan_f64", &custom_ema_fast);

    let custom_ema_slow: Vec<f64> = samples
        .into_iter()
        .tflo(|t| {
            t.timestamp(|x| x.ts);
            let latency = t.prop(|x| x.latency_ms);
            latency.scan_f64(
                || 0.0_f64,
                |state, x| {
                    *state = 0.1 * x + 0.9 * *state;
                    *state
                },
            )
        })
        .collect();
    print_summary("Custom EMA(α=0.1) via scan_f64", &custom_ema_slow);
}
