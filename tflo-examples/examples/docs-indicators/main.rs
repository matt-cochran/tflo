use tflo_core::prelude::*;
use tflo_examples::*;
use tflo_fintech::prelude::*;
use tflo_ops::prelude::*;

/// A web analytics traffic metric: a timestamp and a page-view count.
#[derive(Clone, Debug)]
struct Metric {
    ts: i64,
    views: f64,
}

impl Metric {
    const fn new(ts: i64, views: f64) -> Self {
        Self { ts, views }
    }
}

/// Sample page-view counts collected per analytics interval.
fn sample_metrics() -> Vec<Metric> {
    vec![
        Metric::new(1000, 1200.0),
        Metric::new(2000, 1320.0),
        Metric::new(3000, 1180.0),
        Metric::new(4000, 1450.0),
        Metric::new(5000, 1530.0),
        Metric::new(6000, 1710.0),
        Metric::new(7000, 1620.0),
        Metric::new(8000, 1840.0),
        Metric::new(9000, 1990.0),
        Metric::new(10000, 1880.0),
    ]
}

fn main() {
    let metrics = sample_metrics();

    // ---- SMA count-based ----
    let sma3: Vec<f64> = metrics
        .clone()
        .into_iter()
        .tflo(|t| {
            t.timestamp(|x| x.ts);
            let views = t.prop(|x| x.views);
            views.sma(4usize)
        })
        .collect();
    print_summary("SMA(4)", &sma3);

    // ---- SMA time-based ----
    let sma_time: Vec<f64> = metrics
        .clone()
        .into_iter()
        .tflo(|t| {
            t.timestamp(|x| x.ts);
            let views = t.prop(|x| x.views);
            views.sma(5_u64.secs())
        })
        .collect();
    print_summary("SMA(5s)", &sma_time);

    // ---- EMA ----
    let ema5: Vec<f64> = metrics
        .clone()
        .into_iter()
        .tflo(|t| {
            t.timestamp(|x| x.ts);
            let views = t.prop(|x| x.views);
            views.ema(4usize)
        })
        .collect();
    print_summary("EMA(4)", &ema5);

    // ---- RSI ----
    let rsi14: Vec<f64> = metrics
        .clone()
        .into_iter()
        .tflo(|t| {
            t.timestamp(|x| x.ts);
            let views = t.prop(|x| x.views);
            views.rsi(14usize)
        })
        .collect();
    print_summary("RSI(14)", &rsi14);

    // ---- MACD ----
    let _macd: Vec<(f64, f64, f64)> = metrics
        .clone()
        .into_iter()
        .tflo(|t| {
            t.timestamp(|x| x.ts);
            let views = t.prop(|x| x.views);
            views.macd_n(12, 26, 9)
        })
        .collect();

    // ---- Bollinger Bands ----
    let _bb: Vec<(f64, f64, f64)> = metrics
        .clone()
        .into_iter()
        .tflo(|t| {
            t.timestamp(|x| x.ts);
            let views = t.prop(|x| x.views);
            views.bollinger_bands(4usize, 2.0)
        })
        .collect();

    // ---- CCI ----
    let cci: Vec<f64> = metrics
        .clone()
        .into_iter()
        .tflo(|t| {
            t.timestamp(|x| x.ts);
            let views = t.prop(|x| x.views);
            views.cci_n(14)
        })
        .collect();
    print_summary("CCI(14)", &cci);

    // ---- Stochastic ----
    let _stoch: Vec<(f64, f64)> = metrics
        .clone()
        .into_iter()
        .tflo(|t| {
            t.timestamp(|x| x.ts);
            let views = t.prop(|x| x.views);
            views.stochastic_n(14, 3)
        })
        .collect();

    // ---- Williams %R ----
    let wr: Vec<f64> = metrics
        .clone()
        .into_iter()
        .tflo(|t| {
            t.timestamp(|x| x.ts);
            let views = t.prop(|x| x.views);
            views.williams_r_n(14)
        })
        .collect();
    print_summary("Williams %R(14)", &wr);

    // ---- Z-Score ----
    let zs: Vec<f64> = metrics
        .clone()
        .into_iter()
        .tflo(|t| {
            t.timestamp(|x| x.ts);
            let views = t.prop(|x| x.views);
            views.zscore(4usize)
        })
        .collect();
    print_summary("Z-Score(4)", &zs);

    // ---- Multiple indicators combined ----
    let combined: Vec<(f64, f64)> = metrics
        .into_iter()
        .tflo(|t| {
            t.timestamp(|x| x.ts);
            let views = t.prop(|x| x.views);
            let sma = views.sma(4usize);
            let rsi = views.rsi(14usize);
            (sma, rsi)
        })
        .collect();
    let sma_vals: Vec<f64> = combined.iter().map(|(s, _)| *s).collect();
    print_summary("SMA(4) from combined", &sma_vals);
}
