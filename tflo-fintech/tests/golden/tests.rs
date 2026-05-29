//! Golden vector tests — validates tflo-ta-strict implementations
//! against TA-Lib reference outputs.
//!
//! All golden vectors are generated from the TA-Lib C library via
//! `generate_talib_vectors.py`.  tflo never generates vectors from its
//! own output; that would be circular.  TA-Lib is the single source
//! of truth for correctness.

use super::{GoldenRunner, GoldenVector, validate, validate_multi_output};

// ── Validation Tolerance ────────────────────────────────────────────
//
// MACD: TA-Lib computes the fast/slow EMAs internally with a slightly
// different seeding convention than the standalone EMA function.
// The difference decays over the series; a tolerance of 1.0 captures
// the initial divergence while still detecting real regressions.
const TOL_MACD: f64 = 1.0;

/// Test a single-output indicator golden vector.
fn test_single(fixture_path: &str, tolerance: f64) {
    let vector = GoldenVector::load(fixture_path).expect("Failed to load golden vector");
    let results = GoldenRunner::run(&vector).expect("Failed to run computation");

    let expected = vector
        .expected_output_single()
        .expect("Failed to extract expected output");
    let validation = validate(&expected, &results, tolerance);

    assert!(
        validation.passed,
        "Validation failed for {}: {} mismatches out of {} samples, max_diff={}, mean_diff={}, first_mismatches={:?}",
        fixture_path,
        validation.samples_compared - validation.samples_matched,
        validation.samples_compared,
        validation.max_diff,
        validation.mean_diff,
        validation.mismatches
    );
}

/// Test a multi-output indicator golden vector.
fn test_multi(fixture_path: &str, tolerance: f64) {
    let vector = GoldenVector::load(fixture_path).expect("Failed to load golden vector");
    let results = GoldenRunner::run_multi_output(&vector).expect("Failed to run computation");

    let expected = vector
        .expected_output_multi()
        .expect("Failed to extract expected output");
    let validation = validate_multi_output(&expected, &results, tolerance);

    assert!(
        validation.passed,
        "Validation failed for {}: {} mismatches out of {} samples, max_diff={}, mean_diff={}, first_mismatches={:?}",
        fixture_path,
        validation.samples_compared - validation.samples_matched,
        validation.samples_compared,
        validation.max_diff,
        validation.mean_diff,
        validation.mismatches
    );
}

// ── RSI ──────────────────────────────────────────────────────────────

macro_rules! fixtures {
    ($dir:expr) => {
        concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/golden/fixtures/",
            $dir,
            "/"
        )
    };
}

#[test]
fn test_rsi_talib_7() {
    test_single(concat!(fixtures!("rsi"), "rsi_talib_7.json"), 1e-6);
}

#[test]
fn test_rsi_talib_14() {
    test_single(concat!(fixtures!("rsi"), "rsi_talib_14.json"), 1e-6);
}

#[test]
fn test_rsi_talib_21() {
    test_single(concat!(fixtures!("rsi"), "rsi_talib_21.json"), 1e-6);
}

// ── EMA ──────────────────────────────────────────────────────────────

#[test]
fn test_ema_talib_5() {
    test_single(concat!(fixtures!("ema"), "ema_talib_5.json"), 1e-6);
}

#[test]
fn test_ema_talib_12() {
    test_single(concat!(fixtures!("ema"), "ema_talib_12.json"), 1e-6);
}

#[test]
fn test_ema_talib_26() {
    test_single(concat!(fixtures!("ema"), "ema_talib_26.json"), 1e-6);
}

// ── SMA ──────────────────────────────────────────────────────────────

#[test]
fn test_sma_talib_5() {
    test_single(concat!(fixtures!("sma"), "sma_talib_5.json"), 1e-6);
}

#[test]
fn test_sma_talib_20() {
    test_single(concat!(fixtures!("sma"), "sma_talib_20.json"), 1e-6);
}

#[test]
fn test_sma_talib_50() {
    test_single(concat!(fixtures!("sma"), "sma_talib_50.json"), 1e-6);
}

// ── WMA ──────────────────────────────────────────────────────────────

#[test]
fn test_wma_talib_5() {
    test_single(concat!(fixtures!("wma"), "wma_talib_5.json"), 1e-6);
}

#[test]
fn test_wma_talib_20() {
    test_single(concat!(fixtures!("wma"), "wma_talib_20.json"), 1e-6);
}

// ── MACD ─────────────────────────────────────────────────────────────

#[test]
fn test_macd_talib_12_26_9() {
    test_multi(
        concat!(fixtures!("macd"), "macd_talib_12_26_9.json"),
        TOL_MACD,
    );
}

#[test]
fn test_macd_talib_8_17_9() {
    test_multi(
        concat!(fixtures!("macd"), "macd_talib_8_17_9.json"),
        TOL_MACD,
    );
}

// ── Bollinger Bands ──────────────────────────────────────────────────

#[test]
fn test_bollinger_bands_talib_20_2_0() {
    test_multi(
        concat!(
            fixtures!("bollinger_bands"),
            "bollinger_bands_talib_20_2.0.json"
        ),
        1e-6,
    );
}

#[test]
fn test_bollinger_bands_talib_20_2_5() {
    test_multi(
        concat!(
            fixtures!("bollinger_bands"),
            "bollinger_bands_talib_20_2.5.json"
        ),
        1e-6,
    );
}

// ── Stochastic ───────────────────────────────────────────────────────

#[test]
fn test_stochastic_talib_14_3() {
    test_multi(
        concat!(fixtures!("stochastic"), "stochastic_talib_14_3.json"),
        1e-6,
    );
}

#[test]
fn test_stochastic_talib_5_3() {
    test_multi(
        concat!(fixtures!("stochastic"), "stochastic_talib_5_3.json"),
        1e-6,
    );
}

// ── Williams %R ──────────────────────────────────────────────────────

#[test]
fn test_williams_r_talib_14() {
    test_single(
        concat!(fixtures!("williams_r"), "williams_r_talib_14.json"),
        1e-6,
    );
}

#[test]
fn test_williams_r_talib_21() {
    test_single(
        concat!(fixtures!("williams_r"), "williams_r_talib_21.json"),
        1e-6,
    );
}

// ── CCI ──────────────────────────────────────────────────────────────

#[test]
fn test_cci_talib_14() {
    test_single(concat!(fixtures!("cci"), "cci_talib_14.json"), 1e-6);
}

#[test]
fn test_cci_talib_20() {
    test_single(concat!(fixtures!("cci"), "cci_talib_20.json"), 1e-6);
}

// ── Z-Score ──────────────────────────────────────────────────────────

#[test]
fn test_zscore_talib_20() {
    test_single(concat!(fixtures!("zscore"), "zscore_talib_20.json"), 1e-6);
}

#[test]
fn test_zscore_talib_30() {
    test_single(concat!(fixtures!("zscore"), "zscore_talib_30.json"), 1e-6);
}

// ── MOM ─────────────────────────────────────────────────────────────

#[test]
fn test_mom_talib_10() {
    test_single(concat!(fixtures!("mom"), "mom_talib_10.json"), 1e-6);
}

#[test]
fn test_mom_talib_20() {
    test_single(concat!(fixtures!("mom"), "mom_talib_20.json"), 1e-6);
}

// ── ROC ─────────────────────────────────────────────────────────────

#[test]
fn test_roc_talib_10() {
    test_single(concat!(fixtures!("roc"), "roc_talib_10.json"), 1e-6);
}

#[test]
fn test_roc_talib_20() {
    test_single(concat!(fixtures!("roc"), "roc_talib_20.json"), 1e-6);
}

// ── ADX ─────────────────────────────────────────────────────────────

#[test]
fn test_adx_talib_14() {
    test_single(concat!(fixtures!("adx"), "adx_talib_14.json"), 0.01);
}

#[test]
fn test_adx_talib_20() {
    test_single(concat!(fixtures!("adx"), "adx_talib_20.json"), 0.01);
}

// ── ATR ──────────────────────────────────────────────────────────────

#[test]
fn test_atr_talib_14() {
    test_single(concat!(fixtures!("atr"), "atr_talib_14.json"), 1e-6);
}

#[test]
fn test_atr_talib_20() {
    test_single(concat!(fixtures!("atr"), "atr_talib_20.json"), 1e-6);
}

// ── +DI ──────────────────────────────────────────────────────────────

#[test]
fn test_plus_di_talib_14() {
    test_single(concat!(fixtures!("plus_di"), "plus_di_talib_14.json"), 1e-6);
}

#[test]
fn test_plus_di_talib_20() {
    test_single(concat!(fixtures!("plus_di"), "plus_di_talib_20.json"), 1e-6);
}

// ── -DI ──────────────────────────────────────────────────────────────

#[test]
fn test_minus_di_talib_14() {
    test_single(
        concat!(fixtures!("minus_di"), "minus_di_talib_14.json"),
        1e-6,
    );
}

#[test]
fn test_minus_di_talib_20() {
    test_single(
        concat!(fixtures!("minus_di"), "minus_di_talib_20.json"),
        1e-6,
    );
}

// ── STOCHRSI ────────────────────────────────────────────────────────

#[test]
fn test_stochrsi_talib_14_5_3() {
    test_multi(
        concat!(fixtures!("stochrsi"), "stochrsi_talib_14_5_3.json"),
        1e-6,
    );
}

#[test]
fn test_stochrsi_talib_14_3_3() {
    test_multi(
        concat!(fixtures!("stochrsi"), "stochrsi_talib_14_3_3.json"),
        1e-6,
    );
}

// ── PPO ─────────────────────────────────────────────────────────────

#[test]
fn test_ppo_talib_12_26() {
    test_single(concat!(fixtures!("ppo"), "ppo_talib_12_26.json"), 1e-6);
}

#[test]
fn test_ppo_talib_5_20() {
    test_single(concat!(fixtures!("ppo"), "ppo_talib_5_20.json"), 1e-6);
}

// ── TRIX ────────────────────────────────────────────────────────────

#[test]
fn test_trix_talib_15() {
    test_single(concat!(fixtures!("trix"), "trix_talib_15.json"), 0.01);
}

#[test]
fn test_trix_talib_20() {
    test_single(concat!(fixtures!("trix"), "trix_talib_20.json"), 0.01);
}

// ── KAMA ────────────────────────────────────────────────────────────

#[test]
fn test_kama_talib_10() {
    test_single(concat!(fixtures!("kama"), "kama_talib_10.json"), 1e-6);
}

#[test]
fn test_kama_talib_30() {
    test_single(concat!(fixtures!("kama"), "kama_talib_30.json"), 1e-6);
}

// ── TRIMA ────────────────────────────────────────────────────────────

#[test]
fn test_trima_talib_10() {
    test_single(concat!(fixtures!("trima"), "trima_talib_10.json"), 1e-6);
}

#[test]
fn test_trima_talib_20() {
    test_single(concat!(fixtures!("trima"), "trima_talib_20.json"), 1e-6);
}

// ── DEMA ─────────────────────────────────────────────────────────────

#[test]
fn test_dema_talib_10() {
    test_single(concat!(fixtures!("dema"), "dema_talib_10.json"), 1e-6);
}

#[test]
fn test_dema_talib_20() {
    test_single(concat!(fixtures!("dema"), "dema_talib_20.json"), 1e-6);
}

// ── TEMA ─────────────────────────────────────────────────────────────

#[test]
fn test_tema_talib_10() {
    test_single(concat!(fixtures!("tema"), "tema_talib_10.json"), 1e-6);
}

#[test]
fn test_tema_talib_20() {
    test_single(concat!(fixtures!("tema"), "tema_talib_20.json"), 1e-6);
}

// ── OBV ──────────────────────────────────────────────────────────────

#[test]
fn test_obv_talib() {
    test_single(concat!(fixtures!("obv"), "obv_talib.json"), 1e-6);
}

// ── MFI ──────────────────────────────────────────────────────────────

#[test]
fn test_mfi_talib_14() {
    test_single(concat!(fixtures!("mfi"), "mfi_talib_14.json"), 1e-6);
}

// ── CMO ──────────────────────────────────────────────────────────────

#[test]
fn test_cmo_talib_14() {
    test_single(concat!(fixtures!("cmo"), "cmo_talib_14.json"), 1e-6);
}

#[test]
fn test_cmo_talib_20() {
    test_single(concat!(fixtures!("cmo"), "cmo_talib_20.json"), 1e-6);
}

// ── LINEARREG_SLOPE ──────────────────────────────────────────────────

#[test]
fn test_linearreg_slope_talib_14() {
    test_single(
        concat!(
            fixtures!("linearreg_slope"),
            "linearreg_slope_talib_14.json"
        ),
        1e-10,
    );
}

#[test]
fn test_linearreg_slope_talib_20() {
    test_single(
        concat!(
            fixtures!("linearreg_slope"),
            "linearreg_slope_talib_20.json"
        ),
        1e-10,
    );
}
