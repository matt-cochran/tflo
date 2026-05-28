#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing, clippy::arithmetic_side_effects))]
// The examples crate is allowed to use `println!` for demo output.
#![allow(clippy::print_stdout)]
//! Shared data structures and helpers for tflo-examples.
//!
//! Each blog article named `{name}.mdx` has a corresponding example
//! directory `examples/{name}/main.rs` that demonstrates the concepts
//! in the article as a complete, compilable, runnable program.

/// A tick with price data, used by most examples.
#[derive(Clone, Debug)]
pub struct Tick {
    pub ts: i64,
    pub price: f64,
}

impl Tick {
    pub const fn new(ts: i64, price: f64) -> Self {
        Self { ts, price }
    }
}

/// A tick with both price and volume.
#[derive(Clone, Debug)]
pub struct TradeTick {
    pub ts: i64,
    pub price: f64,
    pub volume: f64,
}

impl TradeTick {
    pub const fn new(ts: i64, price: f64, volume: f64) -> Self {
        Self { ts, price, volume }
    }
}

/// A tick with OHLC data.
#[derive(Clone, Debug)]
pub struct OhlcTick {
    pub ts: i64,
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
}

impl OhlcTick {
    pub const fn new(ts: i64, open: f64, high: f64, low: f64, close: f64) -> Self {
        Self {
            ts,
            open,
            high,
            low,
            close,
        }
    }
}

/// A detection event (used by signal detector examples).
#[derive(Clone, Debug)]
pub struct Detection {
    pub ts: i64,
    pub snr: f64,
    pub freq_mhz: f64,
}

impl Detection {
    pub const fn new(ts: i64, snr: f64, freq_mhz: f64) -> Self {
        Self { ts, snr, freq_mhz }
    }
}

/// Generate a simple price series for examples.
pub fn sample_prices() -> Vec<Tick> {
    vec![
        Tick::new(1000, 100.0),
        Tick::new(2000, 101.0),
        Tick::new(3000, 99.0),
        Tick::new(4000, 102.0),
        Tick::new(5000, 103.0),
        Tick::new(6000, 105.0),
        Tick::new(7000, 104.0),
        Tick::new(8000, 106.0),
        Tick::new(9000, 108.0),
        Tick::new(10000, 107.0),
    ]
}

/// Generate a price series with occasional NaN values (for NaN handling examples).
pub fn sample_prices_with_nan() -> Vec<Tick> {
    vec![
        Tick::new(1000, 100.0),
        Tick::new(2000, f64::NAN),
        Tick::new(3000, 99.0),
        Tick::new(4000, 102.0),
        Tick::new(5000, 103.0),
        Tick::new(6000, f64::NAN),
        Tick::new(7000, 104.0),
        Tick::new(8000, 106.0),
    ]
}

/// Generate a price series for trading examples.
pub fn sample_trades() -> Vec<TradeTick> {
    vec![
        TradeTick::new(1000, 100.0, 1000.0),
        TradeTick::new(2000, 101.0, 1200.0),
        TradeTick::new(3000, 99.0, 1100.0),
        TradeTick::new(4000, 102.0, 1300.0),
        TradeTick::new(5000, 103.0, 1000.0),
        TradeTick::new(6000, 105.0, 1500.0),
        TradeTick::new(7000, 104.0, 1400.0),
        TradeTick::new(8000, 106.0, 1200.0),
    ]
}

/// RSI-style sample data: 15 prices used for RSI demonstrations.
pub fn sample_rsi_prices() -> Vec<f64> {
    vec![
        44.0, 44.25, 44.5, 43.75, 44.5, 44.25, 44.0, 43.5, 43.25, 43.0, 43.25, 43.5, 43.75, 44.0,
        44.25,
    ]
}

/// Pretty-print a summary line for an example.
pub fn print_summary(name: &str, values: &[f64]) {
    let count = values.len();
    let last = values.last().copied().unwrap_or(f64::NAN);
    println!("{name:>40}: count={count:>4}, last={last:>8.4}, values={values:?}");
}

/// Pretty-print a summary for tuple outputs.
pub fn print_tuple3_summary(name: &str, values: &[(f64, f64, f64)]) {
    let count = values.len();
    if let Some(last) = values.last() {
        println!(
            "{name:>40}: count={count:>4}, last=({:.4}, {:.4}, {:.4})",
            last.0, last.1, last.2
        );
    }
}
