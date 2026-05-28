//! Tests for `comp::custom`, split out via `#[path = "..."]` to keep the
//! operational code and the test fixtures readable independently. The tests
//! still live in the same `tests` module of `custom`, so `use super::*`
//! continues to reach private internals like `add_node`.


    use crate::iter_ext::TFlowIteratorExt;

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
