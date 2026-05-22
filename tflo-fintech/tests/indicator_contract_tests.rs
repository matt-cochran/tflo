#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use tflo_core::prelude::*;
use tflo_fintech::prelude::*;

#[derive(Clone)]
struct Bar {
    idx: i64,
    close: f64,
    high: f64,
    low: f64,
}

fn bars(values: &[f64]) -> Vec<Bar> {
    values
        .iter()
        .enumerate()
        .map(|(idx, &close)| Bar {
            idx: idx as i64,
            close,
            high: close,
            low: close,
        })
        .collect()
}

#[test]
fn comp_ema_count_uses_talib_sma_seed_contract() {
    let output: Vec<f64> = bars(&[1.0, 2.0, 3.0, 4.0])
        .into_iter()
        .tflo(|t| {
            t.timestamp(|b: &Bar| b.idx);
            t.prop(|b: &Bar| b.close).ema(3)
        })
        .collect();

    assert!(output[0].is_nan());
    assert!(output[1].is_nan());
    assert_eq!(output[2], 2.0);
    assert_eq!(output[3], 3.0);
}

#[test]
fn comp_kama_count_matches_talib_seed_contract() {
    let output: Vec<f64> = bars(&[1.0, 2.0, 3.0, 4.0])
        .into_iter()
        .tflo(|t| {
            t.timestamp(|b: &Bar| b.idx);
            t.prop(|b: &Bar| b.close).kama_n(2)
        })
        .collect();

    assert!(output[0].is_nan());
    assert!(output[1].is_nan());
    assert!((output[2] - 2.444_444_444_444_444_6).abs() < 1e-12);
    assert!((output[3] - 3.135_802_469_135_802_6).abs() < 1e-12);
}

#[test]
fn comp_ppo_uses_sma_price_oscillator_contract() {
    let output: Vec<f64> = bars(&[1.0, 2.0, 3.0, 4.0])
        .into_iter()
        .tflo(|t| {
            t.timestamp(|b: &Bar| b.idx);
            t.prop(|b: &Bar| b.close).ppo_n(2, 3)
        })
        .collect();

    assert!(output[0].is_nan());
    assert!(output[1].is_nan());
    assert_eq!(output[2], 25.0);
    assert!((output[3] - 16.666_666_666_666_664).abs() < 1e-12);
}

#[test]
fn comp_adx_flat_series_contract_outputs_zero_after_lookback() {
    let output: Vec<f64> = bars(&[1.0, 1.0, 1.0, 1.0, 1.0])
        .into_iter()
        .tflo(|t| {
            t.timestamp(|b: &Bar| b.idx);
            let close = t.prop(|b: &Bar| b.close);
            let high = t.prop(|b: &Bar| b.high);
            let low = t.prop(|b: &Bar| b.low);
            close.adx_n(&high, &low, 2)
        })
        .collect();

    assert!(output[0].is_nan());
    assert!(output[1].is_nan());
    assert!(output[2].is_nan());
    assert_eq!(output[3], 0.0);
    assert_eq!(output[4], 0.0);
}

fn ema_series(data: &[f64], period: usize) -> Vec<Option<f64>> {
    let mut out = vec![None; data.len()];
    if data.len() < period || period == 0 {
        return out;
    }
    let alpha = 2.0 / (period as f64 + 1.0);
    let mut ema = data[..period].iter().sum::<f64>() / period as f64;
    out[period - 1] = Some(ema);
    for i in period..data.len() {
        ema = alpha * data[i] + (1.0 - alpha) * ema;
        out[i] = Some(ema);
    }
    out
}

#[test]
fn trima_matches_triangular_weights() {
    let prices = vec![10.0, 11.0, 12.0, 13.0, 14.0, 15.0, 16.0, 17.0];
    let output: Vec<f64> = prices
        .clone()
        .into_iter()
        .tflo(|t| {
            t.timestamp(|_| 0i64);
            let c = t.prop(|p: &f64| *p);
            c.trima(4)
        })
        .collect();

    for i in 3..prices.len() {
        let expected =
            (prices[i - 3] + 2.0 * prices[i - 2] + 2.0 * prices[i - 1] + prices[i]) / 6.0;
        assert!(
            (output[i] - expected).abs() < 1e-10,
            "TRIMA triangular weights mismatch at {i}: {} vs {expected}",
            output[i]
        );
    }
}

#[test]
fn dema_matches_talib_formula_on_defined_ema_series() {
    let prices = vec![
        10.0, 12.0, 11.0, 14.0, 13.0, 16.0, 15.0, 18.0, 17.0, 20.0, 19.0, 22.0, 21.0, 24.0, 23.0,
    ];
    let period = 4usize;
    let output: Vec<f64> = prices
        .clone()
        .into_iter()
        .tflo(|t| {
            t.timestamp(|_| 0i64);
            let c = t.prop(|p: &f64| *p);
            c.dema_n(period)
        })
        .collect();

    let ema1 = ema_series(&prices, period);
    let ema1_defined: Vec<f64> = ema1.iter().filter_map(|&v| v).collect();
    let ema2 = ema_series(&ema1_defined, period);
    for i in (2 * period - 2)..prices.len() {
        let e1 = ema1[i].unwrap();
        let e2 = ema2[i - (period - 1)].unwrap();
        let expected = 2.0 * e1 - e2;
        assert!((output[i] - expected).abs() < 1e-10, "DEMA mismatch at {i}");
    }
}

#[test]
fn tema_matches_talib_formula_on_defined_ema_series() {
    let prices = vec![
        10.0, 12.0, 11.0, 14.0, 13.0, 16.0, 15.0, 18.0, 17.0, 20.0, 19.0, 22.0, 21.0, 24.0, 23.0,
    ];
    let period = 4usize;
    let output: Vec<f64> = prices
        .clone()
        .into_iter()
        .tflo(|t| {
            t.timestamp(|_| 0i64);
            let c = t.prop(|p: &f64| *p);
            c.tema_n(period)
        })
        .collect();

    let ema1 = ema_series(&prices, period);
    let ema1_defined: Vec<f64> = ema1.iter().filter_map(|&v| v).collect();
    let ema2 = ema_series(&ema1_defined, period);
    let ema2_defined: Vec<f64> = ema2.iter().filter_map(|&v| v).collect();
    let ema3 = ema_series(&ema2_defined, period);
    for i in (3 * period - 3)..prices.len() {
        let e1 = ema1[i].unwrap();
        let e2 = ema2[i - (period - 1)].unwrap();
        let e3 = ema3[i - 2 * (period - 1)].unwrap();
        let expected = 3.0 * e1 - 3.0 * e2 + e3;
        assert!((output[i] - expected).abs() < 1e-10, "TEMA mismatch at {i}");
    }
}

#[test]
fn true_range_handles_overnight_gap_up() {
    let records = vec![
        (0i64, 100.0f64, 101.0, 99.0),
        (1i64, 105.0f64, 106.0, 104.0),
    ];

    let trs: Vec<f64> = records
        .into_iter()
        .tflo(|t| {
            t.timestamp(|r: &(i64, f64, f64, f64)| r.0);
            let close = t.prop(|r: &(i64, f64, f64, f64)| r.1);
            let high = t.prop(|r: &(i64, f64, f64, f64)| r.2);
            let low = t.prop(|r: &(i64, f64, f64, f64)| r.3);
            close.true_range(&high, &low)
        })
        .collect();

    assert!(
        (trs[1] - 6.0).abs() < 1e-10,
        "Expected TR=6.0, got {}",
        trs[1]
    );
}

#[test]
fn stochastic_k_equals_close_only_ohlc() {
    let prices = vec![
        10.0, 11.0, 12.0, 11.5, 10.5, 10.0, 10.5, 11.0, 11.5, 12.0, 11.0, 10.0, 9.5, 10.0, 10.5,
    ];
    let period = 5usize;

    let ks: Vec<f64> = prices
        .clone()
        .into_iter()
        .tflo(|t| {
            t.timestamp(|_| 0i64);
            let c = t.prop(|p: &f64| *p);
            c.stochastic_k(period)
        })
        .collect();

    let ks_ohlc: Vec<f64> = prices
        .into_iter()
        .tflo(|t| {
            t.timestamp(|_| 0i64);
            let c = t.prop(|p: &f64| *p);
            c.stochastic_ohlc_n(&c, &c, period, 1).0
        })
        .collect();

    for (k, ko) in ks.iter().zip(ks_ohlc.iter()) {
        if k.is_nan() || ko.is_nan() {
            continue;
        }
        assert!(
            (k - ko).abs() < 1e-10,
            "stochastic_k != close-only OHLC: {k} vs {ko}"
        );
    }
}

#[test]
fn vwap_bounded_by_price_range() {
    let records: Vec<(i64, f64, f64)> = vec![
        (0, 10.0, 100.0),
        (1, 11.0, 200.0),
        (2, 10.5, 150.0),
        (3, 12.0, 300.0),
        (4, 11.5, 250.0),
        (5, 13.0, 400.0),
    ];

    let vwaps: Vec<f64> = records
        .into_iter()
        .tflo(|t| {
            t.timestamp(|r: &(i64, f64, f64)| r.0);
            let price = t.prop(|r: &(i64, f64, f64)| r.1);
            let volume = t.prop(|r: &(i64, f64, f64)| r.2);
            price.vwap(&volume)
        })
        .collect();

    for v in &vwaps {
        if v.is_nan() {
            continue;
        }
        assert!(*v >= 10.0, "VWAP {v} below min price 10.0");
        assert!(*v <= 13.0, "VWAP {v} above max price 13.0");
    }
    assert!(vwaps.last().unwrap() > &11.5);
}

#[test]
fn obv_increases_on_monotonic_uptrend() {
    let prices: Vec<f64> = (0..20).map(|i| i as f64).collect();
    let results: Vec<f64> = prices
        .into_iter()
        .tflo(|t| {
            t.timestamp(|_| 0i64);
            let close = t.prop(|p: &f64| *p);
            let volume = t.constant(1.0);
            close.obv(&volume)
        })
        .collect();

    for w in results.windows(2) {
        if w[0].is_nan() || w[1].is_nan() {
            continue;
        }
        assert!(
            w[1] >= w[0],
            "OBV decreased in uptrend: {} -> {}",
            w[0],
            w[1]
        );
    }
}

#[test]
fn atr_wilder_seeded_with_sma_of_first_tr_values() {
    let records = vec![
        (0i64, 100.0, 102.0, 99.0),
        (1i64, 101.0, 103.0, 100.0),
        (2i64, 102.0, 104.0, 101.0),
        (3i64, 101.0, 103.0, 100.0),
        (4i64, 103.0, 105.0, 102.0),
        (5i64, 104.0, 106.0, 103.0),
    ];

    let atrs: Vec<f64> = records
        .into_iter()
        .tflo(|t| {
            t.timestamp(|r: &(i64, f64, f64, f64)| r.0);
            let close = t.prop(|r: &(i64, f64, f64, f64)| r.1);
            let high = t.prop(|r: &(i64, f64, f64, f64)| r.2);
            let low = t.prop(|r: &(i64, f64, f64, f64)| r.3);
            close.atr_wilder_n(&high, &low, 3)
        })
        .collect();

    assert!(atrs[0].is_nan());
    assert!(atrs[1].is_nan());
    assert!(atrs[2].is_nan());
    assert!(!atrs[3].is_nan());
    assert!(
        (atrs[3] - 3.0).abs() < 1e-10,
        "ATR seed should be 3.0, got {}",
        atrs[3]
    );
}
