use tflo_core::prelude::*;
use tflo_examples::*;
use tflo_fintech::prelude::*;

fn main() {
    println!("=== Golden Tests: Methodology Demonstration ===");
    println!();
    println!("Golden tests capture exact output of a known-good implementation");
    println!("and compare against it on every change.");
    println!();

    // ---- Concept: known-good output as reference ----
    println!("--- Reference: RSI(14) on known sample data ---");
    let ticks: Vec<Tick> = sample_rsi_prices()
        .into_iter()
        .enumerate()
        .map(|(i, price)| Tick::new((i as i64 + 1) * 1000, price))
        .collect();

    let rsi_values: Vec<f64> = ticks
        .into_iter()
        .tflo(|t| {
            t.timestamp(|x| x.ts);
            let price = t.prop(|x| x.price);
            price.rsi(14usize)
        })
        .collect();

    println!("RSI(14) output ({} values):", rsi_values.len());
    for (i, val) in rsi_values.iter().enumerate() {
        println!("  [{i:>2}] = {:.10}", val);
    }

    // ---- What a golden vector looks like ----
    println!();
    println!("--- Golden vector format ---");
    println!("{{");
    println!("  \"metadata\": {{");
    println!("    \"indicator\": \"rsi_count\",");
    println!("    \"source\": \"tflo-fintech\",");
    println!("    \"version\": \"0.1.0\"");
    println!("  }},");
    println!("  \"params\": {{ \"period\": 14 }},");
    println!("  \"input\": {:?},", sample_rsi_prices());
    println!("  \"expected\": [");
    for val in &rsi_values {
        println!("    {:.10},", val);
    }
    println!("  ]");
    println!("}}");
    println!();

    // ---- Using built-in Bollinger Bands as composition reference ----
    println!("--- Using built-in Bollinger Bands as composition reference ---");
    let prices = sample_prices();
    let manual: Vec<(f64, f64, f64)> = prices
        .clone()
        .into_iter()
        .tflo(|t| {
            t.timestamp(|x| x.ts);
            let price = t.prop(|x| x.price);
            let middle = price.sma(4usize);
            let std = price.std(4usize);
            let band_width = &std * 2.0;
            let upper = &middle + &band_width;
            let lower = &middle - &band_width;
            (middle, upper, lower)
        })
        .collect();

    let builtin: Vec<(f64, f64, f64)> = prices
        .into_iter()
        .tflo(|t| {
            t.timestamp(|x| x.ts);
            let price = t.prop(|x| x.price);
            price.bollinger_bands(4usize, 2.0)
        })
        .collect();

    println!("Manual vs Built-in Bollinger Bands (4, 2.0):");
    for (i, ((m, u, l), (mb, ub, lb))) in manual.iter().zip(&builtin).enumerate() {
        let diff_m = (m - mb).abs();
        let diff_u = (u - ub).abs();
        let diff_l = (l - lb).abs();
        println!(
            "  [{i}] manual=({m:.4},{u:.4},{l:.4}) builtin=({mb:.4},{ub:.4},{lb:.4}) diff=({diff_m:.6},{diff_u:.6},{diff_l:.6})"
        );
    }

    println!();
    println!("--- Key golden testing practices ---");
    println!("1. Capture exact floating-point output of reference implementation");
    println!("2. Store as JSON fixture with metadata (source, version, params)");
    println!("3. Compare against tolerance (e.g., 1e-6)");
    println!("4. Update golden vectors when changing reference implementation deliberately");
    println!("5. Fail CI if output shifts beyond tolerance unexpectedly");
}
