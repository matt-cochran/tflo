//! Example: Custom Composite Algorithms via Extension Traits
//!
//! This example shows how consumers of `tflo-core` can define their own
//! composite algorithms by adding an extension trait to [`Comp<R, f64>`].
//!
//! **Composite algorithms** build entirely on existing [`Comp`] methods (`sma`,
//! `std`, arithmetic, comparisons, etc.) and **do not require** new runtime
//! nodes, internal crate modifications, or access to private APIs like `Node`,
//! `NodeOp`, `NodeState`, or `ValueStore`.
//!
//! This is the recommended path for custom algorithms that can be expressed
//! using existing operations.
//!
//! Run with:
//!
//! ```sh
//! cargo run --example custom_composite -p tflo-core
//! ```

use tflo_core::prelude::*;

// ============================================================================
// Extension trait for custom composite algorithms
// ============================================================================

/// Extension trait providing custom composite algorithms for [`Comp<R, f64>`].
///
/// These methods are built entirely from existing public [`Comp`] APIs and
/// do not require any new primitive nodes or modifications to `tflo-core`.
///
/// This pattern is the recommended path for consumer-defined algorithms
/// that can be expressed using existing operations.
pub trait CustomCompositeExt<R: 'static> {
    /// Spread ratio: `(self - other) / other`.
    ///
    /// Computes the relative spread between two values as a fraction of the
    /// reference value. Useful for measuring relative differences (e.g., basis
    /// points, percentage spreads).
    fn spread_ratio(&self, other: &Comp<R, f64>) -> Comp<R, f64>;

    /// Mean band: returns `(middle, upper, lower)`.
    ///
    /// - `middle` = SMA over window
    /// - `upper` = SMA + k * std
    /// - `lower` = SMA - k * std
    ///
    /// This is a custom Bollinger-style band. Accepts either a [`Duration`]
    /// (time-based) or [`usize`] (count-based) window via `impl Into<Window>`.
    fn mean_band<W: Into<Window>>(
        &self,
        window: W,
        k: f64,
    ) -> (Comp<R, f64>, Comp<R, f64>, Comp<R, f64>);

    /// Normalized score: `(value - mean) / std`.
    ///
    /// Measures how many standard deviations the current value is from the
    /// rolling mean. Similar to a z-score.
    fn normalized_score<W: Into<Window>>(&self, window: W) -> Comp<R, f64>;
}

// ============================================================================
// Implementation
// ============================================================================

impl<R: 'static> CustomCompositeExt<R> for Comp<R, f64> {
    fn spread_ratio(&self, other: &Comp<R, f64>) -> Comp<R, f64> {
        (self - other) / other
    }

    fn mean_band<W: Into<Window>>(
        &self,
        window: W,
        k: f64,
    ) -> (Comp<R, f64>, Comp<R, f64>, Comp<R, f64>) {
        let w: Window = window.into();
        let middle = self.sma(w);
        let std = self.std(w);
        let band_width = &std * k;
        let upper = &middle + &band_width;
        let lower = &middle - &band_width;
        (middle, upper, lower)
    }

    fn normalized_score<W: Into<Window>>(&self, window: W) -> Comp<R, f64> {
        let w: Window = window.into();
        let mean = self.sma(w);
        let std = self.std(w);
        (self - &mean) / &std
    }
}

// ============================================================================
// Demonstration
// ============================================================================

#[derive(Clone)]
struct Tick {
    ts: i64,
    price: f64,
    volume: f64,
}

fn main() {
    let ticks = vec![
        Tick {
            ts: 1000,
            price: 100.0,
            volume: 1000.0,
        },
        Tick {
            ts: 2000,
            price: 101.0,
            volume: 1200.0,
        },
        Tick {
            ts: 3000,
            price: 99.0,
            volume: 1100.0,
        },
        Tick {
            ts: 4000,
            price: 102.0,
            volume: 1300.0,
        },
        Tick {
            ts: 5000,
            price: 103.0,
            volume: 1000.0,
        },
        Tick {
            ts: 6000,
            price: 105.0,
            volume: 1500.0,
        },
        Tick {
            ts: 7000,
            price: 104.0,
            volume: 1400.0,
        },
        Tick {
            ts: 8000,
            price: 106.0,
            volume: 1200.0,
        },
    ];

    // ------------------------------------------------------------------
    // Example 1: Single output — `normalized_score`
    // ------------------------------------------------------------------
    let _scores: Vec<f64> = ticks
        .clone()
        .into_iter()
        .tflo(|t| {
            t.timestamp(|x| x.ts);
            let price = t.prop(|x| x.price);
            // Use custom composite method via extension trait
            price.normalized_score(3)
        })
        .collect();

    println!("Normalized scores (window=3): {:?}", &_scores);

    // ------------------------------------------------------------------
    // Example 2: Tuple output — `mean_band`
    // ------------------------------------------------------------------
    let _bands: Vec<(f64, f64, f64)> = ticks
        .clone()
        .into_iter()
        .tflo(|t| {
            t.timestamp(|x| x.ts);
            let price = t.prop(|x| x.price);
            // Returns (middle, upper, lower) tuple
            price.mean_band(4, 2.0)
        })
        .collect();

    println!("Mean bands (window=4, k=2.0): {:?}", &_bands);

    // ------------------------------------------------------------------
    // Example 3: Composing custom methods with built-ins
    // ------------------------------------------------------------------
    let _results: Vec<(f64, f64)> = ticks
        .clone()
        .into_iter()
        .tflo(|t| {
            t.timestamp(|x| x.ts);
            let price = t.prop(|x| x.price);
            let volume = t.prop(|x| x.volume);

            // Compose built-in SMA with custom spread_ratio
            let price_sma = price.sma(3);
            let vol_sma = volume.sma(3);
            let ratio = price_sma.spread_ratio(&vol_sma);

            // Also compute a normalized score with a time-based window
            let score = price.normalized_score(5_u64.secs());

            (ratio, score)
        })
        .collect();

    println!("Custom methods composing with built-ins: {:?}", &_results);

    // ------------------------------------------------------------------
    // Example 4: Chaining custom -> built-in -> custom
    // ------------------------------------------------------------------
    let _chained: Vec<f64> = ticks
        .into_iter()
        .tflo(|t| {
            t.timestamp(|x| x.ts);
            let price = t.prop(|x| x.price);

            // Chain: normalized_score -> sma -> spread_ratio with constant
            let score = price.normalized_score(3);
            let smoothed_score = score.sma(3);
            let threshold = t.constant(1.5);
            smoothed_score.spread_ratio(&threshold)
        })
        .collect();

    println!("Chained custom -> built-in -> custom: {:?}", &_chained);
}
