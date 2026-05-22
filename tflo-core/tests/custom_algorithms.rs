#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
//! Integration tests that prove all documented custom-algorithm approaches work.
//!
//! These tests mirror the examples in `README.md` and `tflo-core/src/lib.rs` docs.
//! If they pass, the docs are accurate and complete.

use tflo_core::prelude::*;

// ═══════════════════════════════════════════════════════════════════════════
// Test data
// ═══════════════════════════════════════════════════════════════════════════

#[derive(Clone, Debug)]
struct Tick {
    ts: i64,
    price: f64,
    volume: f64,
}

fn test_ticks() -> Vec<Tick> {
    vec![
        Tick {
            ts: 1000,
            price: 100.0,
            volume: 10.0,
        },
        Tick {
            ts: 2000,
            price: 101.0,
            volume: 12.0,
        },
        Tick {
            ts: 3000,
            price: 99.0,
            volume: 15.0,
        },
        Tick {
            ts: 4000,
            price: 102.0,
            volume: 11.0,
        },
        Tick {
            ts: 5000,
            price: 103.0,
            volume: 13.0,
        },
    ]
}

// ═══════════════════════════════════════════════════════════════════════════
// Composite extension traits — the README/lib.rs documented pattern
// ═══════════════════════════════════════════════════════════════════════════

/// Extension trait exactly matching the docs example.
pub trait MyAlgorithms<R: 'static> {
    fn normalized_score<W: Into<Window>>(&self, window: W) -> Comp<R, f64>;
    fn mean_band<W: Into<Window>>(
        &self,
        window: W,
        k: f64,
    ) -> (Comp<R, f64>, Comp<R, f64>, Comp<R, f64>);
    fn spread_ratio(&self, other: &Comp<R, f64>) -> Comp<R, f64>;
}

impl<R: 'static> MyAlgorithms<R> for Comp<R, f64> {
    fn normalized_score<W: Into<Window>>(&self, window: W) -> Comp<R, f64> {
        let w: Window = window.into();
        let mean = self.sma(w);
        let std = self.std(w);
        (self - &mean) / &std
    }

    fn mean_band<W: Into<Window>>(
        &self,
        window: W,
        k: f64,
    ) -> (Comp<R, f64>, Comp<R, f64>, Comp<R, f64>) {
        let w: Window = window.into();
        let middle = self.sma(w);
        let std = self.std(w);
        let band = &std * k;
        let upper = &middle + &band;
        let lower = &middle - &band;
        (middle, upper, lower)
    }

    fn spread_ratio(&self, other: &Comp<R, f64>) -> Comp<R, f64> {
        (self - other) / other
    }
}

#[test]
fn composite_normalized_score_first_non_nan_is_nan() {
    let data = test_ticks();
    let r: Vec<f64> = data
        .into_iter()
        .tflo(|t| {
            t.timestamp(|x| x.ts);
            t.prop(|x| x.price).normalized_score(3)
        })
        .collect();
    assert!(r[0].is_nan());
}

#[test]
fn composite_normalized_score_second_output_is_correct() {
    let data = test_ticks();
    let r: Vec<f64> = data
        .into_iter()
        .tflo(|t| {
            t.timestamp(|x| x.ts);
            t.prop(|x| x.price).normalized_score(3)
        })
        .collect();
    assert!((r[1] - 1.0).abs() < 1e-10);
}

#[test]
fn composite_normalized_score_third_output_is_correct() {
    let data = test_ticks();
    let r: Vec<f64> = data
        .into_iter()
        .tflo(|t| {
            t.timestamp(|x| x.ts);
            t.prop(|x| x.price).normalized_score(3)
        })
        .collect();
    assert!((r[2] + 1.224744871391589).abs() < 1e-10);
}

#[test]
fn composite_mean_band_middle_is_sma() {
    let data = test_ticks();
    let r: Vec<(f64, f64, f64)> = data
        .into_iter()
        .tflo(|t| {
            t.timestamp(|x| x.ts);
            t.prop(|x| x.price).mean_band(3, 2.0)
        })
        .collect();
    assert!((r[2].0 - 100.0).abs() < 1e-10);
}

#[test]
fn composite_mean_band_upper_is_middle_plus_band() {
    let data = test_ticks();
    let r: Vec<(f64, f64, f64)> = data
        .into_iter()
        .tflo(|t| {
            t.timestamp(|x| x.ts);
            t.prop(|x| x.price).mean_band(3, 2.0)
        })
        .collect();
    assert!((r[2].1 - 101.632_993_161_855_76).abs() < 1e-10);
}

#[test]
fn composite_mean_band_lower_is_middle_minus_band() {
    let data = test_ticks();
    let r: Vec<(f64, f64, f64)> = data
        .into_iter()
        .tflo(|t| {
            t.timestamp(|x| x.ts);
            t.prop(|x| x.price).mean_band(3, 2.0)
        })
        .collect();
    assert!((r[2].2 - 98.367_006_838_144_24).abs() < 1e-10);
}

#[test]
fn composite_spread_ratio_at_baseline_is_zero() {
    let data = test_ticks();
    let r: Vec<f64> = data
        .into_iter()
        .tflo(|t| {
            t.timestamp(|x| x.ts);
            let price = t.prop(|x| x.price);
            let baseline = t.constant(100.0);
            price.spread_ratio(&baseline)
        })
        .collect();
    assert!((r[0] - 0.0).abs() < 1e-10);
}

#[test]
fn composite_spread_ratio_above_baseline_is_positive() {
    let data = test_ticks();
    let r: Vec<f64> = data
        .into_iter()
        .tflo(|t| {
            t.timestamp(|x| x.ts);
            let price = t.prop(|x| x.price);
            let baseline = t.constant(100.0);
            price.spread_ratio(&baseline)
        })
        .collect();
    assert!((r[1] - 0.01).abs() < 1e-10);
}

#[test]
fn composite_spread_ratio_below_baseline_is_negative() {
    let data = test_ticks();
    let r: Vec<f64> = data
        .into_iter()
        .tflo(|t| {
            t.timestamp(|x| x.ts);
            let price = t.prop(|x| x.price);
            let baseline = t.constant(100.0);
            price.spread_ratio(&baseline)
        })
        .collect();
    assert!((r[2] + 0.01).abs() < 1e-10);
}

#[test]
fn composite_chaining_returns_correct_count() {
    let data = test_ticks();
    let r: Vec<f64> = data
        .into_iter()
        .tflo(|t| {
            t.timestamp(|x| x.ts);
            let price = t.prop(|x| x.price);
            let threshold = t.constant(0.0);
            price.normalized_score(3).sma(2).spread_ratio(&threshold)
        })
        .collect();
    assert_eq!(r.len(), 5);
}

// ═══════════════════════════════════════════════════════════════════════════
// Functional graph primitives — the README/lib.rs documented closure APIs
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn map_f64_stateless_unary_first_output() {
    let data = test_ticks();
    let r: Vec<f64> = data
        .into_iter()
        .tflo(|t| {
            t.timestamp(|x| x.ts);
            t.prop(|x| x.price).map_f64(|x| x.max(0.0))
        })
        .collect();
    assert!((r[0] - 100.0).abs() < 1e-10);
}

#[test]
fn map_f64_named_does_not_change_behavior() {
    let data = test_ticks();
    let r: Vec<f64> = data
        .into_iter()
        .tflo(|t| {
            t.timestamp(|x| x.ts);
            t.prop(|x| x.price)
                .map_f64(|x| x * 100.0)
                .named("scaled_price")
        })
        .collect();
    assert!((r[0] - 10_000.0).abs() < 1e-10);
}

#[test]
fn map_f64_named_produces_correct_second_output() {
    let data = test_ticks();
    let r: Vec<f64> = data
        .into_iter()
        .tflo(|t| {
            t.timestamp(|x| x.ts);
            t.prop(|x| x.price)
                .map_f64(|x| x * 100.0)
                .named("scaled_price")
        })
        .collect();
    assert!((r[1] - 10_100.0).abs() < 1e-10);
}

#[test]
fn map2_f64_binary_first_output() {
    let data = test_ticks();
    let r: Vec<f64> = data
        .into_iter()
        .tflo(|t| {
            t.timestamp(|x| x.ts);
            let price = t.prop(|x| x.price);
            let volume = t.prop(|x| x.volume);
            price.map2_f64(&volume, |p, v| p * v)
        })
        .collect();
    assert!((r[0] - 1_000.0).abs() < 1e-10);
}

#[test]
fn map2_f64_binary_second_output() {
    let data = test_ticks();
    let r: Vec<f64> = data
        .into_iter()
        .tflo(|t| {
            t.timestamp(|x| x.ts);
            let price = t.prop(|x| x.price);
            let volume = t.prop(|x| x.volume);
            price.map2_f64(&volume, |p, v| p * v)
        })
        .collect();
    assert!((r[1] - 1_212.0).abs() < 1e-10);
}

#[test]
fn filter_f64_drops_value_at_threshold() {
    let data = test_ticks();
    let r: Vec<f64> = data
        .into_iter()
        .tflo(|t| {
            t.timestamp(|x| x.ts);
            t.prop(|x| x.price).filter_f64(|x| x > 100.0)
        })
        .collect();
    assert!(r[0].is_nan());
}

#[test]
fn filter_f64_passes_above_threshold() {
    let data = test_ticks();
    let r: Vec<f64> = data
        .into_iter()
        .tflo(|t| {
            t.timestamp(|x| x.ts);
            t.prop(|x| x.price).filter_f64(|x| x > 100.0)
        })
        .collect();
    assert!((r[1] - 101.0).abs() < 1e-10);
}

#[test]
fn filter_map_f64_suppresses_when_none() {
    let data = test_ticks();
    let r: Vec<f64> = data
        .into_iter()
        .tflo(|t| {
            t.timestamp(|x| x.ts);
            t.prop(|x| x.price)
                .filter_map_f64(|x| if x > 100.0 { Some(x * 2.0) } else { None })
        })
        .collect();
    assert!(r[0].is_nan());
}

#[test]
fn filter_map_f64_transforms_when_some() {
    let data = test_ticks();
    let r: Vec<f64> = data
        .into_iter()
        .tflo(|t| {
            t.timestamp(|x| x.ts);
            t.prop(|x| x.price)
                .filter_map_f64(|x| if x > 100.0 { Some(x * 2.0) } else { None })
        })
        .collect();
    assert!((r[1] - 202.0).abs() < 1e-10);
}

#[test]
fn scan_f64_custom_ema_first_output() {
    let data = test_ticks();
    let r: Vec<f64> = data
        .into_iter()
        .tflo(|t| {
            t.timestamp(|x| x.ts);
            t.prop(|x| x.price).scan_f64(
                || 0.0,
                |s, x| {
                    *s = 0.9 * *s + 0.1 * x;
                    *s
                },
            )
        })
        .collect();
    assert!((r[0] - 10.0).abs() < 1e-10);
}

#[test]
fn scan_f64_custom_ema_second_output() {
    let data = test_ticks();
    let r: Vec<f64> = data
        .into_iter()
        .tflo(|t| {
            t.timestamp(|x| x.ts);
            t.prop(|x| x.price).scan_f64(
                || 0.0,
                |s, x| {
                    *s = 0.9 * *s + 0.1 * x;
                    *s
                },
            )
        })
        .collect();
    assert!((r[1] - 19.1).abs() < 1e-10);
}

#[test]
fn scan_f64_custom_ema_third_output() {
    let data = test_ticks();
    let r: Vec<f64> = data
        .into_iter()
        .tflo(|t| {
            t.timestamp(|x| x.ts);
            t.prop(|x| x.price).scan_f64(
                || 0.0,
                |s, x| {
                    *s = 0.9 * *s + 0.1 * x;
                    *s
                },
            )
        })
        .collect();
    assert!((r[2] - 27.09).abs() < 1e-10);
}

// ═══════════════════════════════════════════════════════════════════════════
// Composition with window-based operators
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn map_f64_before_count_window_first_output() {
    let data = test_ticks();
    let r: Vec<f64> = data
        .into_iter()
        .tflo(|t| {
            t.timestamp(|x| x.ts);
            t.prop(|x| x.price).map_f64(|x| x.max(0.0)).sma(2)
        })
        .collect();
    assert!((r[0] - 100.0).abs() < 1e-10);
}

#[test]
fn map_f64_before_count_window_second_output() {
    let data = test_ticks();
    let r: Vec<f64> = data
        .into_iter()
        .tflo(|t| {
            t.timestamp(|x| x.ts);
            t.prop(|x| x.price).map_f64(|x| x.max(0.0)).sma(2)
        })
        .collect();
    assert!((r[1] - 100.5).abs() < 1e-10);
}

#[test]
fn map_f64_before_time_window_first_output() {
    let data = test_ticks();
    let r: Vec<f64> = data
        .into_iter()
        .tflo(|t| {
            t.timestamp(|x| x.ts);
            t.prop(|x| x.price)
                .map_f64(|x| x.max(0.0))
                .sma(3_u64.secs())
        })
        .collect();
    assert!((r[0] - 100.0).abs() < 0.01);
}

#[test]
fn map_f64_before_time_window_second_output() {
    let data = test_ticks();
    let r: Vec<f64> = data
        .into_iter()
        .tflo(|t| {
            t.timestamp(|x| x.ts);
            t.prop(|x| x.price)
                .map_f64(|x| x.max(0.0))
                .sma(3_u64.secs())
        })
        .collect();
    assert!((r[1] - 100.5).abs() < 0.01);
}

#[test]
fn map_f64_after_count_window_first_output() {
    let data = test_ticks();
    let r: Vec<f64> = data
        .into_iter()
        .tflo(|t| {
            t.timestamp(|x| x.ts);
            let avg = t.prop(|x| x.price).sma(2);
            avg.map_f64(|x| x / 2.0)
        })
        .collect();
    assert!((r[0] - 50.0).abs() < 1e-10);
}

#[test]
fn map_f64_after_count_window_second_output() {
    let data = test_ticks();
    let r: Vec<f64> = data
        .into_iter()
        .tflo(|t| {
            t.timestamp(|x| x.ts);
            let avg = t.prop(|x| x.price).sma(2);
            avg.map_f64(|x| x / 2.0)
        })
        .collect();
    assert!((r[1] - 50.25).abs() < 1e-10);
}

#[test]
fn scan_then_sma_count_first_output() {
    let data = test_ticks();
    let r: Vec<f64> = data
        .into_iter()
        .tflo(|t| {
            t.timestamp(|x| x.ts);
            let cumsum = t.prop(|x| x.price).scan_f64(
                || 0.0,
                |s, x| {
                    *s += x;
                    *s
                },
            );
            cumsum.sma(2)
        })
        .collect();
    assert!((r[0] - 100.0).abs() < 1e-10);
}

#[test]
fn scan_then_sma_count_second_output() {
    let data = test_ticks();
    let r: Vec<f64> = data
        .into_iter()
        .tflo(|t| {
            t.timestamp(|x| x.ts);
            let cumsum = t.prop(|x| x.price).scan_f64(
                || 0.0,
                |s, x| {
                    *s += x;
                    *s
                },
            );
            cumsum.sma(2)
        })
        .collect();
    assert!((r[1] - 150.5).abs() < 1e-10);
}

#[test]
fn composition_before_and_after_first_output() {
    let data = test_ticks();
    let r: Vec<f64> = data
        .into_iter()
        .tflo(|t| {
            t.timestamp(|x| x.ts);
            let clamped = t.prop(|x| x.price).map_f64(|x| x.max(0.0));
            clamped.sma(2).map_f64(|x| x / 100.0)
        })
        .collect();
    assert!((r[0] - 1.0).abs() < 1e-10);
}

#[test]
fn composition_before_and_after_second_output() {
    let data = test_ticks();
    let r: Vec<f64> = data
        .into_iter()
        .tflo(|t| {
            t.timestamp(|x| x.ts);
            let clamped = t.prop(|x| x.price).map_f64(|x| x.max(0.0));
            clamped.sma(2).map_f64(|x| x / 100.0)
        })
        .collect();
    assert!((r[1] - 1.005).abs() < 1e-10);
}

// ═══════════════════════════════════════════════════════════════════════════
// Edge cases
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn unnamed_custom_nodes_remain_valid() {
    let data = test_ticks();
    let r: Vec<f64> = data
        .into_iter()
        .tflo(|t| {
            t.timestamp(|x| x.ts);
            t.prop(|x| x.price).map_f64(|x| x * 2.0)
        })
        .collect();
    assert!((r[0] - 200.0).abs() < 1e-10);
}

#[test]
fn empty_input_produces_empty_output() {
    let data: Vec<Tick> = vec![];
    let r: Vec<f64> = data
        .into_iter()
        .tflo(|t| {
            t.timestamp(|x| x.ts);
            t.prop(|x| x.price).map_f64(|x| x * 2.0).named("empty_test")
        })
        .collect();
    assert!(r.is_empty());
}

#[test]
fn readme_quick_start_example_runs() {
    let ticks = vec![
        Tick {
            ts: 1000,
            price: 100.0,
            volume: 0.0,
        },
        Tick {
            ts: 2000,
            price: 101.0,
            volume: 0.0,
        },
        Tick {
            ts: 3000,
            price: 99.0,
            volume: 0.0,
        },
    ];
    let r: Vec<f64> = ticks
        .into_iter()
        .tflo(|t| {
            t.timestamp(|x| x.ts);
            t.prop(|x| x.price).sma(2_u64.secs())
        })
        .collect();
    assert_eq!(r.len(), 3);
}
