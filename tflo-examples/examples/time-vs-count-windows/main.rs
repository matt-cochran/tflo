use tflo_core::prelude::*;
use tflo_examples::*;
use tflo_ops::prelude::*;

/// A throughput measurement for a monitored network link.
#[derive(Clone, Debug)]
struct Throughput {
    /// Timestamp in milliseconds.
    ts: i64,
    /// Observed link throughput in megabits per second.
    mbps: f64,
}

impl Throughput {
    const fn new(ts: i64, mbps: f64) -> Self {
        Self { ts, mbps }
    }
}

/// Per-second throughput samples from a healthy network link.
fn sample_throughput() -> Vec<Throughput> {
    vec![
        Throughput::new(1000, 100.0),
        Throughput::new(2000, 101.0),
        Throughput::new(3000, 99.0),
        Throughput::new(4000, 102.0),
        Throughput::new(5000, 103.0),
        Throughput::new(6000, 105.0),
        Throughput::new(7000, 104.0),
        Throughput::new(8000, 106.0),
        Throughput::new(9000, 108.0),
        Throughput::new(10000, 107.0),
    ]
}

fn main() {
    // Normal regular data
    let samples = sample_throughput();

    // Data with a gap (the monitoring agent missed some samples)
    let gapped_data = vec![
        Throughput::new(1000, 100.0),
        Throughput::new(2000, 101.0),
        Throughput::new(3000, 102.0),
        // Gap: no data for 5 seconds
        Throughput::new(8000, 110.0),
        Throughput::new(9000, 111.0),
        Throughput::new(10000, 112.0),
    ];

    println!("=== Time vs Count Windows ===");
    println!();

    // ---- Regular data ----
    println!("--- Regular data: similar behavior ---");
    let regular_time: Vec<f64> = samples
        .clone()
        .into_iter()
        .tflo(|t| {
            t.timestamp(|x| x.ts);
            let throughput = t.prop(|x| x.mbps);
            throughput.sma(5_u64.secs())
        })
        .collect();
    print_summary("SMA(5s) time-based", &regular_time);

    let regular_count: Vec<f64> = samples
        .clone()
        .into_iter()
        .tflo(|t| {
            t.timestamp(|x| x.ts);
            let throughput = t.prop(|x| x.mbps);
            throughput.sma(5usize)
        })
        .collect();
    print_summary("SMA(5) count-based", &regular_count);

    // ---- Gapped data: behavior diverges ----
    println!("\n--- Gapped data: behavior diverges ---");

    // Time-based: during the gap, old values are evicted
    let gapped_time: Vec<f64> = gapped_data
        .clone()
        .into_iter()
        .tflo(|t| {
            t.timestamp(|x| x.ts);
            let throughput = t.prop(|x| x.mbps);
            throughput.sma(5_u64.secs())
        })
        .collect();

    // Count-based: even during a gap, old values stay until pushed out
    let gapped_count: Vec<f64> = gapped_data
        .clone()
        .into_iter()
        .tflo(|t| {
            t.timestamp(|x| x.ts);
            let throughput = t.prop(|x| x.mbps);
            throughput.sma(5usize)
        })
        .collect();

    println!("Time-based SMA(5s) with gap:");
    for (sample, val) in gapped_data.iter().zip(&gapped_time) {
        println!(
            "  ts={:>6} mbps={:>.1} sma={:>.4}",
            sample.ts, sample.mbps, val
        );
    }
    println!("\nCount-based SMA(5) with gap:");
    for (sample, val) in gapped_data.iter().zip(&gapped_count) {
        println!(
            "  ts={:>6} mbps={:>.1} sma={:>.4}",
            sample.ts, sample.mbps, val
        );
    }

    // ---- Explicit Window enum construction ----
    println!("\n--- Explicit Window enum ---");
    let w_time = Window::Time(5_u64.secs());
    let w_count = Window::Count(5);
    println!("Time window:  {w_time:?}");
    println!("Count window: {w_count:?}");
    println!("From Duration: {:?}", Window::from(5_u64.secs()));
    println!("From usize:    {:?}", Window::from(5usize));

    // ---- Demonstrate with different data rates ----
    println!("\n--- Varying data rates ---");
    let varying = vec![
        Throughput::new(1000, 100.0),
        Throughput::new(1050, 101.0),  // 50ms apart
        Throughput::new(1100, 102.0),  // 50ms apart
        Throughput::new(2000, 103.0),  // 900ms gap
        Throughput::new(2050, 104.0),  // 50ms apart
        Throughput::new(10000, 110.0), // 8 second gap
    ];

    let time_varying: Vec<f64> = varying
        .clone()
        .into_iter()
        .tflo(|t| {
            t.timestamp(|x| x.ts);
            let throughput = t.prop(|x| x.mbps);
            throughput.sma(2_u64.secs())
        })
        .collect();
    let count_varying: Vec<f64> = varying
        .into_iter()
        .tflo(|t| {
            t.timestamp(|x| x.ts);
            let throughput = t.prop(|x| x.mbps);
            throughput.sma(3usize)
        })
        .collect();

    println!("Time-based SMA(2s): always covers 2 seconds of data");
    for (sample, val) in sample_throughput()[..6].iter().zip(&time_varying) {
        println!(
            "  ts={:>6} mbps={:>.1} sma={:>.4}",
            sample.ts, sample.mbps, val
        );
    }
    println!("\nCount-based SMA(3): always covers last 3 samples");
    for (sample, val) in sample_throughput()[..6].iter().zip(&count_varying) {
        println!(
            "  ts={:>6} mbps={:>.1} sma={:>.4}",
            sample.ts, sample.mbps, val
        );
    }
}
