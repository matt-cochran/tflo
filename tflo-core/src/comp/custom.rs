//! Closure-based functional graph primitives on `Comp`.
//!
//! These methods let consumers define their own stateless transforms,
//! filters, filter-maps, and stateful scans without modifying `tflo-core`.
//!
//! # Naming
//!
//! All methods accept an optional `.named("...")` for graph-inspection
//! readability.  When omitted a generic label is used.

use super::{Comp, Node};
use crate::compile::{Absent, Computed};
use std::sync::Arc;

// ── closure type aliases ───────────────────────────────────────────────────

/// Thread-safe closure alias for internal storage — `f64 → f64`.
type Fn64 = Arc<dyn Fn(f64) -> f64 + Send + Sync>;
/// Thread-safe closure alias — `(f64, f64) → f64`.
type Fn64Bin = Arc<dyn Fn(f64, f64) -> f64 + Send + Sync>;
/// Thread-safe closure alias — `f64 → bool`.
type Fn64Bool = Arc<dyn Fn(f64) -> bool + Send + Sync>;
/// Thread-safe closure alias — `f64 → Option<f64>`.
type Fn64Opt = Arc<dyn Fn(f64) -> Option<f64> + Send + Sync>;

// ═══════════════════════════════════════════════════════════════════════════
// Public API
// ═══════════════════════════════════════════════════════════════════════════

impl<R: 'static> Comp<R, f64> {
    // ── map_f64 ──────────────────────────────────────────────────────────

    /// Apply a stateless unary transform to this computation.
    ///
    /// The closure receives one `f64` value and returns one `f64`.
    ///
    /// # Optional naming
    ///
    /// ```rust,ignore
    /// price.map_f64(|x| x.ln() * 2.0).named("log_scaled_price")
    /// ```
    #[must_use]
    pub fn map_f64<F>(&self, f: F) -> Comp<R>
    where
        F: Fn(f64) -> f64 + Send + Sync + 'static,
    {
        let closure: Fn64 = Arc::new(f);
        Self::add_node_to_state(
            &self.state,
            Node::MapF64 {
                input: self.id,
                f: closure,
                name: None,
            },
        )
    }

    // ── map2_f64 ────────────────────────────────────────────────────────

    /// Apply a stateless binary transform from this and another computation.
    ///
    /// The closure receives two `f64` values `(self, other)` and returns one `f64`.
    #[must_use]
    pub fn map2_f64<F>(&self, other: &Comp<R>, f: F) -> Comp<R>
    where
        F: Fn(f64, f64) -> f64 + Send + Sync + 'static,
    {
        let closure: Fn64Bin = Arc::new(f);
        Self::add_node_to_state(
            &self.state,
            Node::Map2F64 {
                a: self.id,
                b: other.id,
                f: closure,
                name: None,
            },
        )
    }

    // ── filter_f64 ──────────────────────────────────────────────────────

    /// Keep only values where the predicate returns `true`.
    ///
    /// Suppressed values do not appear in the output stream.
    #[must_use]
    pub fn filter_f64<F>(&self, f: F) -> Comp<R>
    where
        F: Fn(f64) -> bool + Send + Sync + 'static,
    {
        let predicate: Fn64Bool = Arc::new(f);
        Self::add_node_to_state(
            &self.state,
            Node::FilterF64 {
                input: self.id,
                predicate,
                name: None,
            },
        )
    }

    // ── filter_map_f64 ──────────────────────────────────────────────────

    /// Apply a transform that may suppress the output.
    ///
    /// Returns `Some(value)` to emit, `None` to suppress.
    #[must_use]
    pub fn filter_map_f64<F>(&self, f: F) -> Comp<R>
    where
        F: Fn(f64) -> Option<f64> + Send + Sync + 'static,
    {
        let closure: Fn64Opt = Arc::new(f);
        Self::add_node_to_state(
            &self.state,
            Node::FilterMapF64 {
                input: self.id,
                f: closure,
                name: None,
            },
        )
    }

    // ── scan_f64 ────────────────────────────────────────────────────────

    /// Stateful unary scan.
    ///
    /// `init` produces fresh state when the graph is compiled.
    /// `step` receives `(&mut S, f64)` and returns one `f64` per record.
    ///
    /// # Optional naming
    ///
    /// ```rust,ignore
    /// price.scan_f64(|| 0.0, |state, x| { *state = 0.9 * *state + 0.1 * x; *state })
    ///     .named("ema_custom")
    /// ```
    #[must_use]
    pub fn scan_f64<S, Init, Step>(&self, init: Init, step: Step) -> Comp<R>
    where
        S: Send + Sync + 'static,
        Init: Fn() -> S + Send + Sync + 'static,
        Step: Fn(&mut S, f64) -> f64 + Send + Sync + 'static,
    {
        let state_factory: Arc<dyn Fn() -> Box<dyn std::any::Any + Send + Sync> + Send + Sync> =
            Arc::new(move || Box::new(init()));
        let step_fn: Arc<
            dyn Fn(&mut Box<dyn std::any::Any + Send + Sync>, f64) -> Computed + Send + Sync,
        > = Arc::new(move |state, x| match state.downcast_mut::<S>() {
            Some(s) => Ok(step(s, x)),
            // The compiler always pairs a `ScanF64` op with a `ScanState` of
            // the matching type; a mismatch can only mean an uninitialised
            // state, so degrade to "warming up" rather than panicking.
            None => Err(Absent::WarmingUp),
        });
        Self::add_node_to_state(
            &self.state,
            Node::ScanF64 {
                input: self.id,
                ctor: state_factory,
                step: step_fn,
                name: None,
            },
        )
    }

    // ── scan2_f64 ───────────────────────────────────────────────────────

    /// Stateful binary scan.
    ///
    /// `init` produces fresh state when the graph is compiled.
    /// `step` receives `(&mut S, f64, f64)` and returns one `f64` per record.
    #[must_use]
    pub fn scan2_f64<S, Init, Step>(&self, other: &Comp<R>, init: Init, step: Step) -> Comp<R>
    where
        S: Send + Sync + 'static,
        Init: Fn() -> S + Send + Sync + 'static,
        Step: Fn(&mut S, f64, f64) -> f64 + Send + Sync + 'static,
    {
        let state_factory: Arc<dyn Fn() -> Box<dyn std::any::Any + Send + Sync> + Send + Sync> =
            Arc::new(move || Box::new(init()));
        let step_fn: Arc<
            dyn Fn(&mut Box<dyn std::any::Any + Send + Sync>, f64, f64) -> Computed + Send + Sync,
        > = Arc::new(move |state, a, b| match state.downcast_mut::<S>() {
            Some(s) => Ok(step(s, a, b)),
            None => Err(Absent::WarmingUp),
        });
        Self::add_node_to_state(
            &self.state,
            Node::Scan2F64 {
                a: self.id,
                b: other.id,
                ctor: state_factory,
                step: step_fn,
                name: None,
            },
        )
    }

    // ── .named(...) ──────────────────────────────────────────────────────

    /// Attach a human-readable name to this custom functional node for
    /// graph-plan, debug, and diagnostic output.
    ///
    /// The name is **optional metadata only** — it has no effect on
    /// semantics, correctness, or type-checking.
    ///
    /// Calling `.named(...)` on a non-custom built-in node is a no-op.
    #[must_use]
    pub fn named(self, name: &str) -> Self {
        let id = self.id;
        {
            let mut state = self.state.borrow_mut();
            if let Some((
                _,
                Node::MapF64 { name: n, .. }
                | Node::Map2F64 { name: n, .. }
                | Node::FilterF64 { name: n, .. }
                | Node::FilterMapF64 { name: n, .. }
                | Node::ScanF64 { name: n, .. }
                | Node::Scan2F64 { name: n, .. },
            )) = state.nodes.iter_mut().find(|(nid, _)| *nid == id)
            {
                *n = Some(name.to_string());
            }
        }
        self
    }
}

#[cfg(test)]
mod tests {
    use crate::iter_ext::TFlowIteratorExt;
    use crate::prelude::IntoDuration;

    #[derive(Clone, Debug)]
    struct TestRecord {
        ts: i64,
        price: f64,
        volume: f64,
    }

    fn test_data() -> Vec<TestRecord> {
        vec![
            TestRecord {
                ts: 1000,
                price: 100.0,
                volume: 10.0,
            },
            TestRecord {
                ts: 2000,
                price: 101.0,
                volume: 12.0,
            },
            TestRecord {
                ts: 3000,
                price: 99.0,
                volume: 15.0,
            },
            TestRecord {
                ts: 4000,
                price: 102.0,
                volume: 11.0,
            },
            TestRecord {
                ts: 5000,
                price: 103.0,
                volume: 13.0,
            },
        ]
    }

    // ── map_f64 ────────────────────────────────────────────────────────

    #[test]
    fn map_f64_doubles_positive_value() {
        let data = test_data();
        let r: Vec<_> = data
            .into_iter()
            .tflo(|t| {
                t.timestamp(|x| x.ts);
                t.prop(|x| x.price).map_f64(|x| x * 2.0)
            })
            .collect();
        assert!((r[0] - 200.0).abs() < 1e-10);
    }

    #[test]
    fn map_f64_produces_correct_second_output() {
        let data = test_data();
        let r: Vec<_> = data
            .into_iter()
            .tflo(|t| {
                t.timestamp(|x| x.ts);
                t.prop(|x| x.price).map_f64(|x| x * 2.0)
            })
            .collect();
        assert!((r[1] - 202.0).abs() < 1e-10);
    }

    #[test]
    fn map_f64_produces_correct_third_output() {
        let data = test_data();
        let r: Vec<_> = data
            .into_iter()
            .tflo(|t| {
                t.timestamp(|x| x.ts);
                t.prop(|x| x.price).map_f64(|x| x * 2.0)
            })
            .collect();
        assert!((r[2] - 198.0).abs() < 1e-10);
    }

    #[test]
    fn map_f64_returns_correct_total_count() {
        let data = test_data();
        let r: Vec<_> = data
            .into_iter()
            .tflo(|t| {
                t.timestamp(|x| x.ts);
                t.prop(|x| x.price).map_f64(|x| x * 2.0)
            })
            .collect();
        assert_eq!(r.len(), 5);
    }

    // ── map2_f64 ───────────────────────────────────────────────────────

    #[test]
    fn map2_f64_multiplies_first_pair_correctly() {
        let data = test_data();
        let r: Vec<_> = data
            .into_iter()
            .tflo(|t| {
                t.timestamp(|x| x.ts);
                let price = t.prop(|x| x.price);
                let volume = t.prop(|x| x.volume);
                price.map2_f64(&volume, |p, v| p * v)
            })
            .collect();
        assert!((r[0] - 1000.0).abs() < 1e-10);
    }

    #[test]
    fn map2_f64_multiplies_second_pair_correctly() {
        let data = test_data();
        let r: Vec<_> = data
            .into_iter()
            .tflo(|t| {
                t.timestamp(|x| x.ts);
                let price = t.prop(|x| x.price);
                let volume = t.prop(|x| x.volume);
                price.map2_f64(&volume, |p, v| p * v)
            })
            .collect();
        assert!((r[1] - 1212.0).abs() < 1e-10);
    }

    #[test]
    fn map2_f64_returns_correct_total_count() {
        let data = test_data();
        let r: Vec<_> = data
            .into_iter()
            .tflo(|t| {
                t.timestamp(|x| x.ts);
                let price = t.prop(|x| x.price);
                let volume = t.prop(|x| x.volume);
                price.map2_f64(&volume, |p, v| p * v)
            })
            .collect();
        assert_eq!(r.len(), 5);
    }

    // ── filter_f64 ─────────────────────────────────────────────────────

    #[test]
    fn filter_f64_drops_value_at_threshold_exact_match() {
        let data = test_data();
        let r: Vec<_> = data
            .into_iter()
            .tflo(|t| {
                t.timestamp(|x| x.ts);
                t.prop(|x| x.price).filter_f64(|x| x > 100.0)
            })
            .collect();
        assert!(r[0].is_nan());
    }

    #[test]
    fn filter_f64_passes_value_above_threshold() {
        let data = test_data();
        let r: Vec<_> = data
            .into_iter()
            .tflo(|t| {
                t.timestamp(|x| x.ts);
                t.prop(|x| x.price).filter_f64(|x| x > 100.0)
            })
            .collect();
        assert!((r[1] - 101.0).abs() < 1e-10);
    }

    #[test]
    fn filter_f64_drops_value_below_threshold() {
        let data = test_data();
        let r: Vec<_> = data
            .into_iter()
            .tflo(|t| {
                t.timestamp(|x| x.ts);
                t.prop(|x| x.price).filter_f64(|x| x > 100.0)
            })
            .collect();
        assert!(r[2].is_nan());
    }

    // ── filter_map_f64 ─────────────────────────────────────────────────

    #[test]
    fn filter_map_f64_suppresses_when_none() {
        let data = test_data();
        let r: Vec<_> = data
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
        let data = test_data();
        let r: Vec<_> = data
            .into_iter()
            .tflo(|t| {
                t.timestamp(|x| x.ts);
                t.prop(|x| x.price)
                    .filter_map_f64(|x| if x > 100.0 { Some(x * 2.0) } else { None })
            })
            .collect();
        assert!((r[1] - 202.0).abs() < 1e-10);
    }

    // ── scan_f64 ───────────────────────────────────────────────────────

    #[test]
    fn scan_f64_cumsum_first_record() {
        let data = test_data();
        let r: Vec<_> = data
            .into_iter()
            .tflo(|t| {
                t.timestamp(|x| x.ts);
                t.prop(|x| x.price).scan_f64(
                    || 0.0,
                    |s, x| {
                        *s += x;
                        *s
                    },
                )
            })
            .collect();
        assert!((r[0] - 100.0).abs() < 1e-10);
    }

    #[test]
    fn scan_f64_cumsum_second_record() {
        let data = test_data();
        let r: Vec<_> = data
            .into_iter()
            .tflo(|t| {
                t.timestamp(|x| x.ts);
                t.prop(|x| x.price).scan_f64(
                    || 0.0,
                    |s, x| {
                        *s += x;
                        *s
                    },
                )
            })
            .collect();
        assert!((r[1] - 201.0).abs() < 1e-10);
    }

    #[test]
    fn scan_f64_cumsum_third_record() {
        let data = test_data();
        let r: Vec<_> = data
            .into_iter()
            .tflo(|t| {
                t.timestamp(|x| x.ts);
                t.prop(|x| x.price).scan_f64(
                    || 0.0,
                    |s, x| {
                        *s += x;
                        *s
                    },
                )
            })
            .collect();
        assert!((r[2] - 300.0).abs() < 1e-10);
    }

    #[test]
    fn scan_f64_cumsum_fourth_record() {
        let data = test_data();
        let r: Vec<_> = data
            .into_iter()
            .tflo(|t| {
                t.timestamp(|x| x.ts);
                t.prop(|x| x.price).scan_f64(
                    || 0.0,
                    |s, x| {
                        *s += x;
                        *s
                    },
                )
            })
            .collect();
        assert!((r[3] - 402.0).abs() < 1e-10);
    }

    #[test]
    fn scan_f64_cumsum_fifth_record() {
        let data = test_data();
        let r: Vec<_> = data
            .into_iter()
            .tflo(|t| {
                t.timestamp(|x| x.ts);
                t.prop(|x| x.price).scan_f64(
                    || 0.0,
                    |s, x| {
                        *s += x;
                        *s
                    },
                )
            })
            .collect();
        assert!((r[4] - 505.0).abs() < 1e-10);
    }

    #[test]
    fn scan_f64_returns_correct_total_count() {
        let data = test_data();
        let r: Vec<_> = data
            .into_iter()
            .tflo(|t| {
                t.timestamp(|x| x.ts);
                t.prop(|x| x.price).scan_f64(
                    || 0.0,
                    |s, x| {
                        *s += x;
                        *s
                    },
                )
            })
            .collect();
        assert_eq!(r.len(), 5);
    }

    // ── scan2_f64 ──────────────────────────────────────────────────────

    #[test]
    fn scan2_f64_cumulative_dollar_volume_first() {
        let data = test_data();
        let r: Vec<_> = data
            .into_iter()
            .tflo(|t| {
                t.timestamp(|x| x.ts);
                let price = t.prop(|x| x.price);
                let volume = t.prop(|x| x.volume);
                price.scan2_f64(
                    &volume,
                    || 0.0,
                    |s, p, v| {
                        *s += p * v;
                        *s
                    },
                )
            })
            .collect();
        assert!((r[0] - 1000.0).abs() < 1e-10);
    }

    #[test]
    fn scan2_f64_cumulative_dollar_volume_second() {
        let data = test_data();
        let r: Vec<_> = data
            .into_iter()
            .tflo(|t| {
                t.timestamp(|x| x.ts);
                let price = t.prop(|x| x.price);
                let volume = t.prop(|x| x.volume);
                price.scan2_f64(
                    &volume,
                    || 0.0,
                    |s, p, v| {
                        *s += p * v;
                        *s
                    },
                )
            })
            .collect();
        assert!((r[1] - 2212.0).abs() < 1e-10);
    }

    #[test]
    fn scan2_f64_cumulative_dollar_volume_third() {
        let data = test_data();
        let r: Vec<_> = data
            .into_iter()
            .tflo(|t| {
                t.timestamp(|x| x.ts);
                let price = t.prop(|x| x.price);
                let volume = t.prop(|x| x.volume);
                price.scan2_f64(
                    &volume,
                    || 0.0,
                    |s, p, v| {
                        *s += p * v;
                        *s
                    },
                )
            })
            .collect();
        assert!((r[2] - 3697.0).abs() < 1e-10);
    }

    // ── .named(...) ────────────────────────────────────────────────────

    #[test]
    fn named_metadata_does_not_affect_output_value() {
        let data = test_data();
        let r: Vec<_> = data
            .into_iter()
            .tflo(|t| {
                t.timestamp(|x| x.ts);
                t.prop(|x| x.price).map_f64(|x| x * 2.0).named("doubled")
            })
            .collect();
        assert!((r[0] - 200.0).abs() < 1e-10);
    }

    #[test]
    fn unnamed_custom_node_remains_valid() {
        let data = test_data();
        let r: Vec<_> = data
            .into_iter()
            .tflo(|t| {
                t.timestamp(|x| x.ts);
                t.prop(|x| x.price).map_f64(|x| x * 2.0)
            })
            .collect();
        assert!((r[0] - 200.0).abs() < 1e-10);
    }

    // ── Composition with count windows ─────────────────────────────────

    #[test]
    fn map_f64_before_sma2_first_output_is_partial_mean() {
        let data = test_data();
        let r: Vec<_> = data
            .into_iter()
            .tflo(|t| {
                t.timestamp(|x| x.ts);
                t.prop(|x| x.price).map_f64(|x| x.max(0.0)).sma(2)
            })
            .collect();
        assert!((r[0] - 100.0).abs() < 1e-10);
    }

    #[test]
    fn map_f64_before_sma2_second_output_is_rolling_mean() {
        let data = test_data();
        let r: Vec<_> = data
            .into_iter()
            .tflo(|t| {
                t.timestamp(|x| x.ts);
                t.prop(|x| x.price).map_f64(|x| x.max(0.0)).sma(2)
            })
            .collect();
        assert!((r[1] - 100.5).abs() < 1e-10);
    }

    // ── Composition with time windows ──────────────────────────────────

    #[test]
    fn map_f64_before_time_window_first_output_is_partial_mean() {
        let data = test_data();
        let r: Vec<_> = data
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
    fn map_f64_before_time_window_second_output_is_rolling_mean() {
        let data = test_data();
        let r: Vec<_> = data
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

    // ── Custom after window ────────────────────────────────────────────

    #[test]
    fn map_f64_after_sma2_first_output_is_half_of_partial_mean() {
        let data = test_data();
        let r: Vec<_> = data
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
    fn map_f64_after_sma2_second_output_is_half_of_rolling_mean() {
        let data = test_data();
        let r: Vec<_> = data
            .into_iter()
            .tflo(|t| {
                t.timestamp(|x| x.ts);
                let avg = t.prop(|x| x.price).sma(2);
                avg.map_f64(|x| x / 2.0)
            })
            .collect();
        assert!((r[1] - 50.25).abs() < 1e-10);
    }

    // ── Scan then window ───────────────────────────────────────────────

    #[test]
    fn scan_cumsum_then_sma2_first_output_is_partial_cumsum_mean() {
        let data = test_data();
        let r: Vec<_> = data
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
    fn scan_cumsum_then_sma2_second_output_is_rolling_cumsum_mean() {
        let data = test_data();
        let r: Vec<_> = data
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
}
