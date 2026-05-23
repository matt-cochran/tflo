use tflo_core::prelude::*;
use tflo_examples::*;
use tflo_ops::prelude::*;

// ============================================================================
// Extension trait for custom composite algorithms
// ============================================================================

pub trait CustomCompositeExt<R: 'static> {
    fn spread_ratio(&self, other: &Comp<R, f64>) -> Comp<R, f64>;
    fn mean_band<W: Into<Window>>(
        &self,
        window: W,
        k: f64,
    ) -> (Comp<R, f64>, Comp<R, f64>, Comp<R, f64>);
    fn normalized_score<W: Into<Window>>(&self, window: W) -> Comp<R, f64>;
}

impl<R: 'static> CustomCompositeExt<R> for Comp<R, f64> {
    fn spread_ratio(&self, other: &Self) -> Self {
        (self - other) / other
    }

    fn mean_band<W: Into<Window>>(
        &self,
        window: W,
        k: f64,
    ) -> (Self, Self, Self) {
        let w: Window = window.into();
        let middle = self.sma(w);
        let std = self.std(w);
        let band_width = &std * k;
        let upper = &middle + &band_width;
        let lower = &middle - &band_width;
        (middle, upper, lower)
    }

    fn normalized_score<W: Into<Window>>(&self, window: W) -> Self {
        let w: Window = window.into();
        let mean = self.sma(w);
        let std = self.std(w);
        (self - &mean) / &std
    }
}

/// A patient vital sign: a timestamp and a heart rate in beats per minute.
#[derive(Clone, Debug)]
struct Vital {
    ts: i64,
    bpm: f64,
}

impl Vital {
    const fn new(ts: i64, bpm: f64) -> Self {
        Self { ts, bpm }
    }
}

/// A vital sign sampled together with respiration rate (breaths per minute).
#[derive(Clone, Debug)]
struct VitalPair {
    ts: i64,
    bpm: f64,
    resp_rate: f64,
}

impl VitalPair {
    const fn new(ts: i64, bpm: f64, resp_rate: f64) -> Self {
        Self { ts, bpm, resp_rate }
    }
}

/// Sample heart-rate readings from a bedside patient monitor.
fn sample_vitals() -> Vec<Vital> {
    vec![
        Vital::new(1000, 72.0),
        Vital::new(2000, 74.0),
        Vital::new(3000, 71.0),
        Vital::new(4000, 78.0),
        Vital::new(5000, 80.0),
        Vital::new(6000, 85.0),
        Vital::new(7000, 83.0),
        Vital::new(8000, 88.0),
        Vital::new(9000, 92.0),
        Vital::new(10000, 90.0),
    ]
}

/// Sample paired heart-rate and respiration readings.
fn sample_vital_pairs() -> Vec<VitalPair> {
    vec![
        VitalPair::new(1000, 72.0, 14.0),
        VitalPair::new(2000, 74.0, 15.0),
        VitalPair::new(3000, 71.0, 13.0),
        VitalPair::new(4000, 78.0, 16.0),
        VitalPair::new(5000, 80.0, 15.0),
        VitalPair::new(6000, 85.0, 18.0),
        VitalPair::new(7000, 83.0, 17.0),
        VitalPair::new(8000, 88.0, 19.0),
    ]
}

fn main() {
    let vitals = sample_vitals();
    let vital_pairs = sample_vital_pairs();

    // ---- Single output: normalized_score ----
    let scores: Vec<f64> = vitals
        .clone()
        .into_iter()
        .tflo(|t| {
            t.timestamp(|x| x.ts);
            let bpm = t.prop(|x| x.bpm);
            bpm.normalized_score(3usize)
        })
        .collect();
    print_summary("Normalized score(3)", &scores);

    // ---- Tuple output: mean_band ----
    let bands: Vec<(f64, f64, f64)> = vitals
        .clone()
        .into_iter()
        .tflo(|t| {
            t.timestamp(|x| x.ts);
            let bpm = t.prop(|x| x.bpm);
            bpm.mean_band(4usize, 2.0)
        })
        .collect();
    print_tuple3_summary("Mean band(4, 2.0)", &bands);

    // ---- Composing custom with built-in ----
    let composed: Vec<(f64, f64)> = vital_pairs
        .clone()
        .into_iter()
        .tflo(|t| {
            t.timestamp(|x| x.ts);
            let bpm = t.prop(|x| x.bpm);
            let resp_rate = t.prop(|x| x.resp_rate);
            let bpm_sma = bpm.sma(3usize);
            let resp_sma = resp_rate.sma(3usize);
            let ratio = bpm_sma.spread_ratio(&resp_sma);
            let score = bpm.normalized_score(5_u64.secs());
            (ratio, score)
        })
        .collect();
    for (ts, (ratio, score)) in vital_pairs.iter().map(|v| v.ts).zip(&composed) {
        println!(
            "  ts={ts:>6} spread_ratio={ratio:.4} normalized_score={score:.4}"
        );
    }

    // ---- Chaining: custom -> built-in -> custom ----
    let chained: Vec<f64> = vitals
        .into_iter()
        .tflo(|t| {
            t.timestamp(|x| x.ts);
            let bpm = t.prop(|x| x.bpm);
            let score = bpm.normalized_score(3usize);
            let smoothed_score = score.sma(3usize);
            let threshold = t.constant(1.5);
            smoothed_score.spread_ratio(&threshold)
        })
        .collect();
    print_summary("Chained: norm_score->sma->spread_ratio", &chained);
}
