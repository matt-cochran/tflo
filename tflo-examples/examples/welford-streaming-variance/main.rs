use tflo_core::prelude::*;
use tflo_examples::*;

/// A measured machined part coming off a CNC line for quality control.
#[derive(Clone, Debug)]
struct Part {
    /// Timestamp in milliseconds.
    ts: i64,
    /// Measured part diameter in millimetres.
    diameter_mm: f64,
    /// Spindle temperature in degrees Celsius when the part was cut.
    spindle_temp_c: f64,
}

impl Part {
    fn new(ts: i64, diameter_mm: f64, spindle_temp_c: f64) -> Self {
        Self {
            ts,
            diameter_mm,
            spindle_temp_c,
        }
    }
}

/// A run of machined parts measured one per second on the QC station.
/// The nominal target diameter is 25.0 mm.
fn sample_parts() -> Vec<Part> {
    vec![
        Part::new(1000, 25.00, 40.0),
        Part::new(2000, 25.01, 40.2),
        Part::new(3000, 24.99, 39.8),
        Part::new(4000, 25.02, 40.4),
        Part::new(5000, 25.03, 40.6),
        Part::new(6000, 25.05, 41.0),
        Part::new(7000, 25.04, 40.8),
        Part::new(8000, 25.06, 41.2),
        Part::new(9000, 25.08, 41.6),
        Part::new(10000, 25.07, 41.4),
    ]
}

fn main() {
    let parts = sample_parts();

    // ---- Running variance and std ----
    let variance: Vec<f64> = parts.clone().into_iter()
        .tflo(|t| {
            t.timestamp(|x| x.ts);
            let diameter = t.prop(|x| x.diameter_mm);
            diameter.variance(5usize)
        })
        .collect();
    print_summary("Variance(5)", &variance);

    let stddev: Vec<f64> = parts.clone().into_iter()
        .tflo(|t| {
            t.timestamp(|x| x.ts);
            let diameter = t.prop(|x| x.diameter_mm);
            diameter.std(5usize)
        })
        .collect();
    print_summary("StdDev(5)", &stddev);

    // ---- Higher moments ----
    let skewness: Vec<f64> = parts.clone().into_iter()
        .tflo(|t| {
            t.timestamp(|x| x.ts);
            let diameter = t.prop(|x| x.diameter_mm);
            diameter.skewness(6usize)
        })
        .collect();
    print_summary("Skewness(6)", &skewness);

    let kurtosis: Vec<f64> = parts.clone().into_iter()
        .tflo(|t| {
            t.timestamp(|x| x.ts);
            let diameter = t.prop(|x| x.diameter_mm);
            diameter.kurtosis(6usize)
        })
        .collect();
    print_summary("Kurtosis(6)", &kurtosis);

    // ---- Correlation between diameter and spindle temperature ----
    let correlation: Vec<f64> = parts.clone().into_iter()
        .tflo(|t| {
            t.timestamp(|x| x.ts);
            let diameter = t.prop(|x| x.diameter_mm);
            let spindle_temp = t.prop(|x| x.spindle_temp_c);
            diameter.correlation(&spindle_temp, 5usize)
        })
        .collect();
    print_summary("Correlation(5) diameter/temp", &correlation);

    let covariance: Vec<f64> = parts.clone().into_iter()
        .tflo(|t| {
            t.timestamp(|x| x.ts);
            let diameter = t.prop(|x| x.diameter_mm);
            let spindle_temp = t.prop(|x| x.spindle_temp_c);
            diameter.covariance(&spindle_temp, 5usize)
        })
        .collect();
    print_summary("Covariance(5) diameter/temp", &covariance);

    // ---- Z-score ----
    let zscores: Vec<f64> = parts.clone().into_iter()
        .tflo(|t| {
            t.timestamp(|x| x.ts);
            let diameter = t.prop(|x| x.diameter_mm);
            diameter.zscore(5usize)
        })
        .collect();
    print_summary("Z-score(5)", &zscores);

    // ---- Median ----
    let medians: Vec<f64> = parts.into_iter()
        .tflo(|t| {
            t.timestamp(|x| x.ts);
            let diameter = t.prop(|x| x.diameter_mm);
            diameter.median(5usize)
        })
        .collect();
    print_summary("Median(5)", &medians);
}
