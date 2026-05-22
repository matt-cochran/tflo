use tflo_core::prelude::*;
use tflo_ops::events::ThresholdCrossEventMode;
use tflo_ops::prelude::*;

// A vibration reading from an accelerometer on a pump or motor.
#[derive(Clone, Debug)]
struct VibrationSample {
    ts: i64,
    amplitude_g: f64,
}

impl VibrationSample {
    fn new(ts: i64, amplitude_g: f64) -> Self {
        Self { ts, amplitude_g }
    }
}

// Vibration trace from a pump bearing, amplitude in g.
fn sample_vibration() -> Vec<VibrationSample> {
    vec![
        VibrationSample::new(1000, 1.00),
        VibrationSample::new(2000, 1.01),
        VibrationSample::new(3000, 0.99),
        VibrationSample::new(4000, 1.02),
        VibrationSample::new(5000, 1.03),
        VibrationSample::new(6000, 1.05),
        VibrationSample::new(7000, 1.04),
        VibrationSample::new(8000, 1.06),
        VibrationSample::new(9000, 1.08),
        VibrationSample::new(10000, 1.07),
    ]
}

// A longer trace used for the momentum-gauge detector below.
fn sample_motor_run() -> Vec<VibrationSample> {
    vec![
        VibrationSample::new(1000, 1.00),
        VibrationSample::new(2000, 1.01),
        VibrationSample::new(3000, 0.99),
        VibrationSample::new(4000, 1.02),
        VibrationSample::new(5000, 1.03),
        VibrationSample::new(6000, 1.05),
        VibrationSample::new(7000, 1.04),
        VibrationSample::new(8000, 1.06),
    ]
}

fn main() {
    let vibration = sample_vibration();
    let motor_run = sample_motor_run();

    // ---- 1. Cross detection ----
    println!("=== 1. CrossDetector: amplitude crossing rolling baseline ===");
    let crosses: Vec<ThresholdCrossEventMode> = vibration
        .clone()
        .into_iter()
        .tflo(|t| {
            t.timestamp(|x| x.ts);
            let amplitude = t.prop(|x| x.amplitude_g);
            let baseline = amplitude.sma(3usize);
            amplitude.cross(&baseline)
        })
        .collect();
    for (ts, cross) in vibration.iter().map(|s| s.ts).zip(&crosses) {
        if *cross != ThresholdCrossEventMode::None {
            println!("  ts={:>6} cross={:?}", ts, cross);
        }
    }

    // ---- 2. CrossAbove / CrossUnder ----
    println!("\n=== 2. Directional cross: CrossAbove / CrossUnder ===");
    let cross_directional: Vec<(ThresholdCrossEventMode, ThresholdCrossEventMode)> = vibration
        .clone()
        .into_iter()
        .tflo(|t| {
            t.timestamp(|x| x.ts);
            let amplitude = t.prop(|x| x.amplitude_g);
            let baseline = amplitude.sma(3usize);
            let above = amplitude.cross_above(&baseline);
            let under = amplitude.cross_under(&baseline);
            (above, under)
        })
        .collect();
    for (ts, (above, under)) in vibration.iter().map(|s| s.ts).zip(&cross_directional) {
        if *above == ThresholdCrossEventMode::Rising {
            println!("  ts={:>6} → AMPLITUDE CROSSED ABOVE BASELINE", ts);
        }
        if *under == ThresholdCrossEventMode::Falling {
            println!("  ts={:>6} → AMPLITUDE CROSSED BELOW BASELINE", ts);
        }
    }

    // ---- 3. Cross with hysteresis ----
    println!("\n=== 3. Cross with hysteresis (noise immunity) ===");
    let hysteresis: Vec<ThresholdCrossEventMode> = vibration
        .clone()
        .into_iter()
        .tflo(|t| {
            t.timestamp(|x| x.ts);
            let amplitude = t.prop(|x| x.amplitude_g);
            let alarm_limit = t.constant(1.03);
            amplitude.cross_hysteresis(&alarm_limit, 1.0)
        })
        .collect();
    for (ts, cross) in vibration.iter().map(|s| s.ts).zip(&hysteresis) {
        if *cross != ThresholdCrossEventMode::None {
            println!(
                "  ts={:>6} cross with hysteresis (margin=1.0): {:?}",
                ts, cross
            );
        }
    }

    // ---- 4. Comparison operators as signal detectors ----
    println!("\n=== 4. Comparison operators (gt, lt, gte, lte) ===");
    let comparisons: Vec<f64> = vibration
        .clone()
        .into_iter()
        .tflo(|t| {
            t.timestamp(|x| x.ts);
            let amplitude = t.prop(|x| x.amplitude_g);
            let alarm_limit = t.constant(1.03);
            let above = amplitude.gt(&alarm_limit);
            let below = amplitude.lt(&alarm_limit);
            // Combine: +1 for above, -1 for below, 0 for equal
            above - &below
        })
        .collect();
    for (ts, val) in vibration.iter().map(|s| s.ts).zip(&comparisons) {
        if *val != 0.0 {
            println!(
                "  ts={:>6} {} alarm_limit=1.03 g",
                ts,
                if *val > 0.0 { "ABOVE" } else { "BELOW" }
            );
        }
    }

    // ---- 5. RSI cross signals ----
    println!("\n=== 5. Vibration momentum threshold cross signals ===");
    let rsi_crosses: Vec<(f64, ThresholdCrossEventMode, ThresholdCrossEventMode)> = motor_run
        .into_iter()
        .tflo(|t| {
            t.timestamp(|x| x.ts);
            let amplitude = t.prop(|x| x.amplitude_g);
            let rsi = amplitude.rsi(14usize);
            let overstressed = rsi.cross_above(&t.constant(70.0));
            let settling = rsi.cross_under(&t.constant(30.0));
            (rsi, overstressed, settling)
        })
        .collect();
    for (ts, (rsi, overstressed, settling)) in
        sample_motor_run().iter().map(|s| s.ts).zip(&rsi_crosses)
    {
        if *overstressed == ThresholdCrossEventMode::Rising {
            println!("  ts={:>6} momentum={:.2} → OVERSTRESSED (>70)", ts, rsi);
        }
        if *settling == ThresholdCrossEventMode::Falling {
            println!("  ts={:>6} momentum={:.2} → SETTLING (<30)", ts, rsi);
        }
    }
}
