use tflo_core::prelude::*;
use tflo_fintech::prelude::*;

// A process-signal reading from a sensor on a conveyor line (e.g. fill level).
#[derive(Clone, Debug)]
struct ProcessSample {
    ts: i64,
    level: f64,
}

impl ProcessSample {
    fn new(ts: i64, level: f64) -> Self {
        Self { ts, level }
    }
}

// Process samples from a conveyor sensor.
fn sample_process() -> Vec<ProcessSample> {
    vec![
        ProcessSample::new(1000, 100.0),
        ProcessSample::new(2000, 101.0),
        ProcessSample::new(3000, 99.0),
        ProcessSample::new(4000, 102.0),
        ProcessSample::new(5000, 103.0),
        ProcessSample::new(6000, 105.0),
        ProcessSample::new(7000, 104.0),
        ProcessSample::new(8000, 106.0),
        ProcessSample::new(9000, 108.0),
        ProcessSample::new(10000, 107.0),
    ]
}

// A longer run used for momentum/MACD detectors below.
fn sample_process_run() -> Vec<ProcessSample> {
    vec![
        ProcessSample::new(1000, 100.0),
        ProcessSample::new(2000, 101.0),
        ProcessSample::new(3000, 99.0),
        ProcessSample::new(4000, 102.0),
        ProcessSample::new(5000, 103.0),
        ProcessSample::new(6000, 105.0),
        ProcessSample::new(7000, 104.0),
        ProcessSample::new(8000, 106.0),
    ]
}

fn main() {
    let process = sample_process();
    let process_run = sample_process_run();

    // ---- Cross detection ----
    let crosses: Vec<ThresholdCrossEventMode> = process
        .clone()
        .into_iter()
        .tflo(|t| {
            t.timestamp(|x| x.ts);
            let level = t.prop(|x| x.level);
            let sma3 = level.sma(3usize);
            level.cross(&sma3)
        })
        .collect();
    let cross_count = crosses
        .iter()
        .filter(|c| **c != ThresholdCrossEventMode::None)
        .count();
    assert!(cross_count <= process.len());

    // ---- CrossAbove / CrossUnder ----
    let above_under: Vec<(ThresholdCrossEventMode, ThresholdCrossEventMode)> = process
        .clone()
        .into_iter()
        .tflo(|t| {
            t.timestamp(|x| x.ts);
            let level = t.prop(|x| x.level);
            let sma3 = level.sma(3usize);
            let above = level.cross_above(&sma3);
            let under = level.cross_under(&sma3);
            (above, under)
        })
        .collect();
    let above_count = above_under
        .iter()
        .filter(|(a, _)| *a == ThresholdCrossEventMode::Rising)
        .count();
    let under_count = above_under
        .iter()
        .filter(|(_, u)| *u == ThresholdCrossEventMode::Falling)
        .count();

    // ---- Hysteresis cross ----
    let hysteresis: Vec<ThresholdCrossEventMode> = process
        .clone()
        .into_iter()
        .tflo(|t| {
            t.timestamp(|x| x.ts);
            let level = t.prop(|x| x.level);
            let alarm_limit = t.constant(103.0);
            level.cross_hysteresis(&alarm_limit, 1.0)
        })
        .collect();
    let hyst_count = hysteresis
        .iter()
        .filter(|c| **c != ThresholdCrossEventMode::None)
        .count();

    // ---- Momentum threshold cross ----
    let rsi_crosses: Vec<(f64, ThresholdCrossEventMode, ThresholdCrossEventMode)> = process_run
        .clone()
        .into_iter()
        .tflo(|t| {
            t.timestamp(|x| x.ts);
            let level = t.prop(|x| x.level);
            let rsi = level.rsi(14usize);
            let overshoot = rsi.cross_above(&t.constant(70.0));
            let undershoot = rsi.cross_under(&t.constant(30.0));
            (rsi, overshoot, undershoot)
        })
        .collect();
    let ob_count = rsi_crosses
        .iter()
        .filter(|(_, o, _)| *o == ThresholdCrossEventMode::Rising)
        .count();
    let os_count = rsi_crosses
        .iter()
        .filter(|(_, _, o)| *o == ThresholdCrossEventMode::Falling)
        .count();

    // ---- MACD signal cross ----
    let _macd_crosses: Vec<(f64, f64, ThresholdCrossEventMode)> = process_run
        .clone()
        .into_iter()
        .tflo(|t| {
            t.timestamp(|x| x.ts);
            let level = t.prop(|x| x.level);
            let (macd_line, signal_line, _hist) = level.macd_n(12, 26, 9);
            let trending_up = macd_line.cross_above(&signal_line);
            (macd_line, signal_line, trending_up)
        })
        .collect();

    // ---- Comparison operators ----
    let compars: Vec<f64> = process
        .clone()
        .into_iter()
        .tflo(|t| {
            t.timestamp(|x| x.ts);
            let level = t.prop(|x| x.level);
            let alarm_limit = t.constant(103.0);
            let above = level.gt(&alarm_limit);
            let below = level.lt(&alarm_limit);
            above - &below
        })
        .collect();
    let pos_count = compars.iter().filter(|v| **v > 0.0).count();
    let neg_count = compars.iter().filter(|v| **v < 0.0).count();

    // ---- Manual graph + step ----
    let step_results: Vec<ThresholdCrossEventMode> = process
        .clone()
        .into_iter()
        .tflo(|t| {
            t.timestamp(|x| x.ts);
            let level = t.prop(|x| x.level);
            let sma3 = level.sma(3usize);
            level.cross_above(&sma3)
        })
        .collect();
    let step_cnt = step_results
        .iter()
        .filter(|c| **c == ThresholdCrossEventMode::Rising)
        .count();

    println!("Cross detected count={cross_count}");
    println!("Above count={above_count} Under count={under_count}");
    println!("Hysteresis count={hyst_count}");
    println!("Overshoot count={ob_count} Undershoot count={os_count}");
    println!("Positive count={pos_count} Negative count={neg_count}");
    println!("Manual step rising count={step_cnt}");
}
