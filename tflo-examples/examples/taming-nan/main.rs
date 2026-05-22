use tflo_core::prelude::*;

/// A single reading from an IoT soil-moisture sensor.
#[derive(Clone, Debug)]
struct SoilReading {
    /// Timestamp in milliseconds.
    ts: i64,
    /// Volumetric soil moisture as a percentage.
    moisture_pct: f64,
}

impl SoilReading {
    fn new(ts: i64, moisture_pct: f64) -> Self {
        Self { ts, moisture_pct }
    }
}

/// A clean run of soil-moisture readings, one per second.
fn sample_soil() -> Vec<SoilReading> {
    vec![
        SoilReading::new(1000, 30.0),
        SoilReading::new(2000, 31.0),
        SoilReading::new(3000, 29.0),
        SoilReading::new(4000, 32.0),
        SoilReading::new(5000, 33.0),
        SoilReading::new(6000, 35.0),
        SoilReading::new(7000, 34.0),
        SoilReading::new(8000, 36.0),
        SoilReading::new(9000, 38.0),
        SoilReading::new(10000, 37.0),
    ]
}

/// Soil-moisture readings with NaN values where the sensor dropped
/// out and reported no valid measurement.
fn sample_soil_with_dropouts() -> Vec<SoilReading> {
    vec![
        SoilReading::new(1000, 30.0),
        SoilReading::new(2000, f64::NAN),
        SoilReading::new(3000, 29.0),
        SoilReading::new(4000, 32.0),
        SoilReading::new(5000, 33.0),
        SoilReading::new(6000, f64::NAN),
        SoilReading::new(7000, 34.0),
        SoilReading::new(8000, 36.0),
    ]
}

fn main() {
    let clean_soil = sample_soil();
    let soil_with_dropouts = sample_soil_with_dropouts();

    // ---- Permissive default: NaN propagates silently ----
    println!("=== Permissive default (NaN propagates) ===");
    let permissive: Vec<f64> = soil_with_dropouts
        .clone()
        .into_iter()
        .tflo(|t| {
            t.timestamp(|x| x.ts);
            let moisture = t.prop(|x| x.moisture_pct);
            let sma = moisture.sma(3usize);

            &sma * 2.0
        })
        .collect();
    for (reading, val) in soil_with_dropouts.iter().zip(&permissive) {
        println!(
            "  ts={:>6} moisture={:>8} output={:>8}",
            reading.ts, reading.moisture_pct, val
        );
    }

    // ---- Strict validation: catches NaN ----
    println!("\n=== Strict validation (catches NaN) ===");
    let strict: Vec<_> = soil_with_dropouts
        .clone()
        .into_iter()
        .validated(ValidationOptions::strict(), |t| {
            t.timestamp(|x| x.ts);
            let moisture = t.prop(|x| x.moisture_pct);
            let sma = moisture.sma(3usize);

            &sma * 2.0
        })
        .collect();
    for (reading, result) in soil_with_dropouts.iter().zip(&strict) {
        match result {
            Ok(val) => println!(
                "  ts={:>6} moisture={:>8} Ok({:.4})",
                reading.ts, reading.moisture_pct, val
            ),
            Err(e) => println!(
                "  ts={:>6} moisture={:>8} Error: {e}",
                reading.ts, reading.moisture_pct
            ),
        }
    }

    // ---- Validation options configuration ----
    println!("\n--- ValidationOptions ---");
    let permissive_opts = ValidationOptions::new();
    println!(
        "Permissive: assert_sorted={}, reject_nan={}, reject_inf={}",
        permissive_opts.assert_sorted, permissive_opts.reject_nan, permissive_opts.reject_inf
    );

    let strict_opts = ValidationOptions::strict();
    println!(
        "Strict: assert_sorted={}, reject_nan={}, reject_inf={}, error_on_nan={}",
        strict_opts.assert_sorted,
        strict_opts.reject_nan,
        strict_opts.reject_inf,
        strict_opts.error_on_nan
    );

    // ---- Fine-grained options ----
    println!("\n--- Custom validation ---");
    let custom_opts = ValidationOptions::new()
        .error_on_nan(true)
        .error_on_inf(true)
        .assert_sorted(true);
    println!(
        "Custom: assert_sorted={}, error_on_nan={}, error_on_inf={}",
        custom_opts.assert_sorted, custom_opts.error_on_nan, custom_opts.error_on_inf
    );

    // ---- Clean data with strict validation works fine ----
    println!("\n--- Clean data with strict validation ---");
    let clean_validated: Vec<_> = clean_soil
        .into_iter()
        .validated(ValidationOptions::strict().assert_sorted(false), |t| {
            t.timestamp(|x| x.ts);
            let moisture = t.prop(|x| x.moisture_pct);
            moisture.sma(3usize)
        })
        .collect();
    for (ts, result) in sample_soil().iter().map(|t| t.ts).zip(&clean_validated) {
        if let Ok(val) = result {
            println!("  ts={:>6} Ok({:.4})", ts, val);
        }
    }
}
