use tflo_core::prelude::*;
use tflo_examples::*;
use tflo_ops::prelude::*;

/// A per-aircraft telemetry sample: a timestamp, an altitude in feet,
/// and the tail number of the aircraft it belongs to.
#[derive(Clone, Debug)]
struct Telemetry {
    ts: i64,
    altitude_ft: f64,
    aircraft_id: &'static str,
}

impl Telemetry {
    fn new(ts: i64, altitude_ft: f64, aircraft_id: &'static str) -> Self {
        Self {
            ts,
            altitude_ft,
            aircraft_id,
        }
    }
}

/// Sample altitude telemetry from an aircraft climb profile.
fn sample_telemetry() -> Vec<Telemetry> {
    vec![
        Telemetry::new(1000, 10000.0, "N1234"),
        Telemetry::new(2000, 10100.0, "N1234"),
        Telemetry::new(3000, 9900.0, "N1234"),
        Telemetry::new(4000, 10200.0, "N1234"),
        Telemetry::new(5000, 10300.0, "N1234"),
        Telemetry::new(6000, 10500.0, "N1234"),
        Telemetry::new(7000, 10400.0, "N1234"),
        Telemetry::new(8000, 10600.0, "N1234"),
        Telemetry::new(9000, 10800.0, "N1234"),
        Telemetry::new(10000, 10700.0, "N1234"),
    ]
}

fn main() {
    let telemetry = sample_telemetry();

    // ---- Mode 1: Sync iterator (default) ----
    println!("=== Mode 1: Sync Iterator ===");
    let smas: Vec<f64> = telemetry
        .clone()
        .into_iter()
        .tflo(|t| {
            t.timestamp(|x: &Telemetry| x.ts);
            let altitude = t.prop(|x: &Telemetry| x.altitude_ft);
            altitude.sma(3usize)
        })
        .collect();
    print_summary("Sync SMA(3)", &smas);

    // ---- Mode 2: Sync with original records (.with()) ----
    println!("\n=== Sync with original records (.with()) ===");
    let enriched: Vec<(Telemetry, f64)> = telemetry
        .clone()
        .into_iter()
        .with(|t| {
            t.timestamp(|x: &Telemetry| x.ts);
            let altitude = t.prop(|x: &Telemetry| x.altitude_ft);
            altitude.sma(3usize)
        })
        .collect();
    for (sample, sma) in &enriched {
        println!(
            "  aircraft={} ts={}, altitude_ft={}, sma={:.4}",
            sample.aircraft_id, sample.ts, sample.altitude_ft, sma
        );
    }

    // ---- Mode 3: Multiple outputs via tuple ----
    println!("\n=== Multiple outputs ===");
    let multi: Vec<(f64, f64)> = telemetry
        .clone()
        .into_iter()
        .tflo(|t| {
            t.timestamp(|x: &Telemetry| x.ts);
            let altitude = t.prop(|x: &Telemetry| x.altitude_ft);
            let sma = altitude.sma(3usize);
            let ema = altitude.ema(3usize);
            (sma, ema)
        })
        .collect();
    for (ts, (sma, ema)) in telemetry.iter().map(|x| x.ts).zip(&multi) {
        println!("  ts={:>6} sma={:.4} ema={:.4}", ts, sma, ema);
    }

    // ---- Explicit error handling with tflo_try() ----
    println!("\n=== Explicit errors with tflo_try() ===");
    let try_results: Vec<_> = telemetry
        .clone()
        .into_iter()
        .tflo_try(|t| {
            t.timestamp(|x: &Telemetry| x.ts);
            let altitude = t.prop(|x: &Telemetry| x.altitude_ft);
            altitude.sma(3usize)
        })
        .collect();
    for (ts, result) in telemetry.iter().map(|x| x.ts).zip(&try_results) {
        match result {
            Ok(item) => println!("  ts={} → Ready: {:.4}", ts, item.value),
            Err(e) => println!("  ts={} → Error: {e}", ts),
        }
    }

    // ---- Strict validation ----
    println!("\n=== Strict validation ===");
    use tflo_core::validation::ValidationOptions;
    let validated: Vec<_> = telemetry
        .clone()
        .into_iter()
        .validated(ValidationOptions::strict().assert_sorted(false), |t| {
            t.timestamp(|x: &Telemetry| x.ts);
            let altitude = t.prop(|x: &Telemetry| x.altitude_ft);
            altitude.sma(3usize)
        })
        .collect();
    for (ts, result) in telemetry.iter().map(|x| x.ts).zip(&validated) {
        match result {
            Ok(val) => println!("  ts={} → Ok({:.4})", ts, val),
            Err(e) => println!("  ts={} → Err({e})", ts),
        }
    }

    // ---- Conceptual: Async mode ----
    println!("\n--- Async mode (conceptual) ---");
    println!("The `async` feature enables:");
    println!("  - TFloStream: async stream adapter");
    println!("  - TFloKeyedStream: keyed async stream adapter (key by aircraft_id)");
    println!("  - Same graph, different execution mode");
    println!("  - Compatible with tokio-stream and futures");
    println!("Enable with: tflo-core = {{ features = [\"async\"] }}");
}
