#!/usr/bin/env python3
"""Generate golden vectors from TA-Lib C library.

This script uses TA-Lib Python bindings to generate reference outputs
for all technical analysis indicators, creating golden vector JSON files
that can be used to validate tflo-ta-strict implementations.

Usage:
    python3 generate_talib_vectors.py --indicator rsi --period 14
    python3 generate_talib_vectors.py --all

Structural note:
    This file is flagged as a god file (1011 LOC, 62 functions) but is
    deliberately exempt from decomposition. Reasoning:

    1. It's a code generator with naturally parallel per-indicator
       structure (one `<indicator>_talib()` wrapper + one
       `generate_<indicator>_vectors()` orchestrator per indicator
       family). Splitting one family per file would create ~10 tiny
       modules without any maintainability benefit.

    2. Its correctness contract is the byte-exact content of the JSON
       fixture files it writes under tests/golden/fixtures/, not the
       organization of the source. Any structural refactor must be
       gated by `git diff tests/golden/fixtures/` returning empty.

    3. Float arithmetic isn't associative; reordering operations across
       module boundaries (even imports) can change the last-ULP output.
       The risk of breaking the entire `tflo-fintech` golden-fixture
       bit-equality suite is real and the upside is cosmetic.

    If a future lint-exemption mechanism lands, replace this
    note with a formal exemption entry. Until
    then, this docstring is the human-readable record of the decision.
"""
# pyright: reportUntypedFunctionDecorator=false, reportAny=false, reportAttributeAccessIssue=false
# ta-lib C extension type stubs are incomplete; all talib.* calls are valid at runtime

import argparse
import json
import sys
from pathlib import Path

import numpy as np
import talib
from talib import MA_Type


def generate_test_data(n_samples: int = 200) -> list[float]:
    """Generate realistic test price data."""
    # Generate synthetic price data with realistic characteristics
    np.random.seed(42)  # Reproducible
    base_price = 100.0
    returns = list(np.random.normal(0.001, 0.02, n_samples))  # Small daily returns
    prices = [base_price]
    for ret in returns:
        prices.append(prices[-1] * (1 + ret))
    return prices[1:]  # Skip initial base_price


def rsi_talib(input_data: list[float], period: int) -> list[float | None]:
    """Generate RSI using TA-Lib."""
    data = np.array(input_data, dtype=np.float64)
    result = talib.RSI(data, timeperiod=period)
    # Convert NaN to None for JSON serialization
    return [None if np.isnan(x) else float(x) for x in result]


def ema_talib(input_data: list[float], period: int) -> list[float | None]:
    """Generate EMA using TA-Lib."""
    data = np.array(input_data, dtype=np.float64)
    result = talib.EMA(data, timeperiod=period)
    return [None if np.isnan(x) else float(x) for x in result]


def sma_talib(input_data: list[float], period: int) -> list[float | None]:
    """Generate SMA using TA-Lib."""
    data = np.array(input_data, dtype=np.float64)
    result = talib.SMA(data, timeperiod=period)
    return [None if np.isnan(x) else float(x) for x in result]


def wma_talib(input_data: list[float], period: int) -> list[float | None]:
    """Generate WMA using TA-Lib."""
    data = np.array(input_data, dtype=np.float64)
    result = talib.WMA(data, timeperiod=period)
    return [None if np.isnan(x) else float(x) for x in result]


def macd_talib(
    input_data: list[float], fast: int, slow: int, signal: int
) -> tuple[list[float | None], list[float | None], list[float | None]]:
    """Generate MACD using TA-Lib. Returns (macd, signal, histogram)."""
    data = np.array(input_data, dtype=np.float64)
    macd, signal_line, histogram = talib.MACD(
        data, fastperiod=fast, slowperiod=slow, signalperiod=signal
    )

    def to_optional_list(arr):  # type: ignore[no-any-explicit]
        """Convert numpy array to list with None for NaN values."""
        return [None if np.isnan(x) else float(x) for x in arr]

    return (
        to_optional_list(macd),
        to_optional_list(signal_line),
        to_optional_list(histogram),
    )


def bollinger_bands_talib(
    input_data: list[float], period: int, nbdev: float
) -> tuple[list[float | None], list[float | None], list[float | None]]:
    """Generate Bollinger Bands using TA-Lib. Returns (upper, middle, lower)."""
    data = np.array(input_data, dtype=np.float64)
    upper, middle, lower = talib.BBANDS(
        data, timeperiod=period, nbdevup=nbdev, nbdevdn=nbdev, matype=MA_Type.SMA
    )

    def to_optional_list(arr):  # type: ignore[no-any-explicit]
        """Convert numpy array to list with None for NaN values."""
        return [None if np.isnan(x) else float(x) for x in arr]

    return to_optional_list(upper), to_optional_list(middle), to_optional_list(lower)


def stochastic_talib(
    high: list[float],
    low: list[float],
    close: list[float],
    k_period: int,
    d_period: int,
) -> tuple[list[float | None], list[float | None]]:
    """Generate Stochastic using TA-Lib. Returns (%K, %D)."""
    high_arr = np.array(high, dtype=np.float64)
    low_arr = np.array(low, dtype=np.float64)
    close_arr = np.array(close, dtype=np.float64)

    slowk, slowd = talib.STOCH(
        high_arr,
        low_arr,
        close_arr,
        fastk_period=k_period,
        slowk_period=d_period,
        slowk_matype=MA_Type.SMA,
        slowd_period=d_period,
        slowd_matype=MA_Type.SMA,
    )

    def to_optional_list(arr):  # type: ignore[no-any-explicit]
        """Convert numpy array to list with None for NaN values."""
        return [None if np.isnan(x) else float(x) for x in arr]

    return to_optional_list(slowk), to_optional_list(slowd)


def williams_r_talib(
    high: list[float], low: list[float], close: list[float], period: int
) -> list[float | None]:
    """Generate Williams %R using TA-Lib."""
    high_arr = np.array(high, dtype=np.float64)
    low_arr = np.array(low, dtype=np.float64)
    close_arr = np.array(close, dtype=np.float64)

    result = talib.WILLR(high_arr, low_arr, close_arr, timeperiod=period)
    return [None if np.isnan(x) else float(x) for x in result]


def cci_talib(
    high: list[float], low: list[float], close: list[float], period: int
) -> list[float | None]:
    """Generate CCI using TA-Lib."""
    high_arr = np.array(high, dtype=np.float64)
    low_arr = np.array(low, dtype=np.float64)
    close_arr = np.array(close, dtype=np.float64)

    result = talib.CCI(high_arr, low_arr, close_arr, timeperiod=period)
    return [None if np.isnan(x) else float(x) for x in result]


def zscore_talib(input_data: list[float], period: int) -> list[float | None]:
    """Generate Z-Score using TA-Lib (custom calculation)."""
    data = np.array(input_data, dtype=np.float64)
    sma = talib.SMA(data, timeperiod=period)
    std = talib.STDDEV(data, timeperiod=period)

    result = np.where(std != 0, (data - sma) / std, np.nan)
    return [None if np.isnan(x) else float(x) for x in result]


def create_golden_vector(
    indicator: str,
    source: str,
    params: dict[str, object],
    input_data: list[float],
    expected_output: list[float | None],
    warmup_samples: int,
    provenance: str = "Generated from TA-Lib C library using Python bindings",
) -> dict[str, object]:
    """Create a golden vector JSON structure."""
    return {
        "metadata": {
            "indicator": indicator,
            "source": source,
            "version": "0.4.0",  # TA-Lib version
            "license": "BSD",
            "provenance": provenance,
        },
        "params": params,
        "input": input_data,
        "expected_output": expected_output,
        "warmup_samples": warmup_samples,
    }


def create_multi_output_golden_vector(
    indicator: str,
    source: str,
    params: dict[str, object],
    input_data: list[float],
    expected_outputs: list[list[float | None]],
    warmup_samples: int,
    provenance: str = "Generated from TA-Lib C library using Python bindings",
) -> dict[str, object]:
    """Create a golden vector for multi-output indicators."""
    return {
        "metadata": {
            "indicator": indicator,
            "source": source,
            "version": "0.4.0",
            "license": "BSD",
            "provenance": provenance,
        },
        "params": params,
        "input": input_data,
        "expected_output": expected_outputs,  # List of lists for multi-output
        "warmup_samples": warmup_samples,
    }


def save_vector(vector: dict[str, object], output_path: Path):
    """Save golden vector to JSON file."""
    output_path.parent.mkdir(parents=True, exist_ok=True)
    with open(output_path, "w") as f:
        json.dump(vector, f, indent=2)


def generate_rsi_vectors(test_data: list[float], output_dir: Path):
    """Generate RSI golden vectors for multiple periods."""
    periods = [7, 14, 21]
    for period in periods:
        expected = rsi_talib(test_data, period)
        warmup = period + 1  # RSI needs period + 1 samples

        vector = create_golden_vector(
            indicator="rsi_talib_count",
            source="talib",
            params={"period": period},
            input_data=test_data,
            expected_output=expected,
            warmup_samples=warmup,
        )

        output_path = output_dir / "rsi" / f"rsi_talib_{period}.json"
        save_vector(vector, output_path)
        print(f"Generated: {output_path}")


def generate_ema_vectors(test_data: list[float], output_dir: Path):
    """Generate EMA golden vectors for multiple periods."""
    periods = [5, 12, 26]
    for period in periods:
        expected = ema_talib(test_data, period)
        warmup = period

        vector = create_golden_vector(
            indicator="ema_talib_count",
            source="talib",
            params={"period": period},
            input_data=test_data,
            expected_output=expected,
            warmup_samples=warmup,
        )

        output_path = output_dir / "ema" / f"ema_talib_{period}.json"
        save_vector(vector, output_path)
        print(f"Generated: {output_path}")


def generate_sma_vectors(test_data: list[float], output_dir: Path):
    """Generate SMA golden vectors for multiple periods."""
    periods = [5, 20, 50]
    for period in periods:
        expected = sma_talib(test_data, period)
        warmup = period

        vector = create_golden_vector(
            indicator="sma_talib_count",
            source="talib",
            params={"period": period},
            input_data=test_data,
            expected_output=expected,
            warmup_samples=warmup,
        )

        output_path = output_dir / "sma" / f"sma_talib_{period}.json"
        save_vector(vector, output_path)
        print(f"Generated: {output_path}")


def generate_wma_vectors(test_data: list[float], output_dir: Path):
    """Generate WMA golden vectors for multiple periods."""
    periods = [5, 20]
    for period in periods:
        expected = wma_talib(test_data, period)
        warmup = period

        vector = create_golden_vector(
            indicator="wma_talib_count",
            source="talib",
            params={"period": period},
            input_data=test_data,
            expected_output=expected,
            warmup_samples=warmup,
        )

        output_path = output_dir / "wma" / f"wma_talib_{period}.json"
        save_vector(vector, output_path)
        print(f"Generated: {output_path}")


def generate_macd_vectors(test_data: list[float], output_dir: Path):
    """Generate MACD golden vectors."""
    configs = [(12, 26, 9), (8, 17, 9)]
    for fast, slow, signal in configs:
        macd, signal_line, histogram = macd_talib(test_data, fast, slow, signal)
        warmup = slow + signal  # Need slow period + signal period

        vector = create_multi_output_golden_vector(
            indicator="macd_talib_count",
            source="talib",
            params={"fast": fast, "slow": slow, "signal": signal},
            input_data=test_data,
            expected_outputs=[macd, signal_line, histogram],
            warmup_samples=warmup,
        )

        output_path = output_dir / "macd" / f"macd_talib_{fast}_{slow}_{signal}.json"
        save_vector(vector, output_path)
        print(f"Generated: {output_path}")


def generate_bollinger_vectors(test_data: list[float], output_dir: Path):
    """Generate Bollinger Bands golden vectors."""
    configs = [(20, 2.0), (20, 2.5)]
    for period, nbdev in configs:
        upper, middle, lower = bollinger_bands_talib(test_data, period, nbdev)
        warmup = period

        vector = create_multi_output_golden_vector(
            indicator="bollinger_bands_talib_count",
            source="talib",
            params={"period": period, "nbdev": nbdev},
            input_data=test_data,
            expected_outputs=[upper, middle, lower],
            warmup_samples=warmup,
        )

        output_path = (
            output_dir
            / "bollinger_bands"
            / f"bollinger_bands_talib_{period}_{nbdev}.json"
        )
        save_vector(vector, output_path)
        print(f"Generated: {output_path}")


def generate_stochastic_vectors(test_data: list[float], output_dir: Path):
    """Generate Stochastic golden vectors."""
    # For stochastic, we need high/low/close - use test_data with variations
    high = [x * 1.01 for x in test_data]
    low = [x * 0.99 for x in test_data]
    close = test_data

    configs = [(14, 3), (5, 3)]
    for k_period, d_period in configs:
        k, d = stochastic_talib(high, low, close, k_period, d_period)
        warmup = k_period + d_period

        vector = create_multi_output_golden_vector(
            indicator="stochastic_talib_count",
            source="talib",
            params={"k_period": k_period, "d_period": d_period},
            input_data=close,  # Use close as primary input
            expected_outputs=[k, d],
            warmup_samples=warmup,
        )

        output_path = (
            output_dir / "stochastic" / f"stochastic_talib_{k_period}_{d_period}.json"
        )
        save_vector(vector, output_path)
        print(f"Generated: {output_path}")


def generate_williams_r_vectors(test_data: list[float], output_dir: Path):
    """Generate Williams %R golden vectors."""
    high = [x * 1.01 for x in test_data]
    low = [x * 0.99 for x in test_data]
    close = test_data

    periods = [14, 21]
    for period in periods:
        expected = williams_r_talib(high, low, close, period)
        warmup = period

        vector = create_golden_vector(
            indicator="williams_r_talib_count",
            source="talib",
            params={"period": period},
            input_data=close,
            expected_output=expected,
            warmup_samples=warmup,
        )

        output_path = output_dir / "williams_r" / f"williams_r_talib_{period}.json"
        save_vector(vector, output_path)
        print(f"Generated: {output_path}")


def generate_cci_vectors(test_data: list[float], output_dir: Path):
    """Generate CCI golden vectors."""
    high = [x * 1.01 for x in test_data]
    low = [x * 0.99 for x in test_data]
    close = test_data

    periods = [14, 20]
    for period in periods:
        expected = cci_talib(high, low, close, period)
        warmup = period

        vector = create_golden_vector(
            indicator="cci_talib_count",
            source="talib",
            params={"period": period},
            input_data=close,
            expected_output=expected,
            warmup_samples=warmup,
        )

        output_path = output_dir / "cci" / f"cci_talib_{period}.json"
        save_vector(vector, output_path)
        print(f"Generated: {output_path}")


def generate_zscore_vectors(test_data: list[float], output_dir: Path):
    """Generate Z-Score golden vectors."""
    periods = [20, 30]
    for period in periods:
        expected = zscore_talib(test_data, period)
        warmup = period

        vector = create_golden_vector(
            indicator="zscore_talib_count",
            source="talib",
            params={"period": period},
            input_data=test_data,
            expected_output=expected,
            warmup_samples=warmup,
        )

        output_path = output_dir / "zscore" / f"zscore_talib_{period}.json"
        save_vector(vector, output_path)
        print(f"Generated: {output_path}")


# ── New indicators ────────────────────────────────────────────────


def mom_talib(input_data: list[float], period: int) -> list[float | None]:
    """MOM — Momentum: close - close(period ago)."""
    data = np.array(input_data, dtype=np.float64)
    result = talib.MOM(data, timeperiod=period)
    return [None if np.isnan(x) else float(x) for x in result]


def roc_talib(input_data: list[float], period: int) -> list[float | None]:
    """ROC — Rate of Change: (close / close(period ago) - 1) * 100."""
    data = np.array(input_data, dtype=np.float64)
    result = talib.ROC(data, timeperiod=period)
    return [None if np.isnan(x) else float(x) for x in result]


def adx_talib(
    high: list[float], low: list[float], close: list[float], period: int
) -> list[float | None]:
    """ADX — Average Directional Index (trend strength, 0-100)."""
    high_arr = np.array(high, dtype=np.float64)
    low_arr = np.array(low, dtype=np.float64)
    close_arr = np.array(close, dtype=np.float64)
    result = talib.ADX(high_arr, low_arr, close_arr, timeperiod=period)
    return [None if np.isnan(x) else float(x) for x in result]


def stochrsi_talib(
    input_data: list[float], period: int, fastk: int, fastd: int
) -> list[float | None]:
    """STOCHRSI — Stochastic of RSI."""
    data = np.array(input_data, dtype=np.float64)
    fastk_out, fastd_out = talib.STOCHRSI(
        data,
        timeperiod=period,
        fastk_period=fastk,
        fastd_period=fastd,
        fastd_matype=MA_Type.SMA,
    )
    return [None if np.isnan(x) else float(x) for x in fastk_out], [
        None if np.isnan(x) else float(x) for x in fastd_out
    ]


def ppo_talib(
    input_data: list[float], fast: int, slow: int, matype: int = 0
) -> list[float | None]:
    """PPO — Percentage Price Oscillator: ((EMA(fast)-EMA(slow))/EMA(slow))*100.

    Uses TA-Lib's default ``matype=MA_Type.EMA`` to match the canonical
    EMA-based definition. The previous SMA-based variant masked seeding
    differences and required a coarse (1.0) tolerance to pass; with EMA
    we can validate to 1e-6 against the regenerated fixtures.
    """
    data = np.array(input_data, dtype=np.float64)
    result = talib.PPO(data, fastperiod=fast, slowperiod=slow, matype=MA_Type.EMA)
    return [None if np.isnan(x) else float(x) for x in result]


def trix_talib(input_data: list[float], period: int) -> list[float | None]:
    """TRIX — Triple Smooth EMA: 1-period ROC of triple-EMA."""
    data = np.array(input_data, dtype=np.float64)
    result = talib.TRIX(data, timeperiod=period)
    return [None if np.isnan(x) else float(x) for x in result]


def kama_talib(input_data: list[float], period: int) -> list[float | None]:
    """KAMA — Kaufman Adaptive Moving Average."""
    data = np.array(input_data, dtype=np.float64)
    result = talib.KAMA(data, timeperiod=period)
    return [None if np.isnan(x) else float(x) for x in result]


# ── generators ───────────────────────────────────────────────────


def generate_mom_vectors(test_data: list[float], output_dir: Path):
    for period in [10, 20]:
        expected = mom_talib(test_data, period)
        vector = create_golden_vector(
            indicator="mom_talib_count",
            source="talib",
            params={"period": period},
            input_data=test_data,
            expected_output=expected,
            warmup_samples=period,
        )
        save_vector(vector, output_dir / "mom" / f"mom_talib_{period}.json")
        print(f"Generated: {output_dir / 'mom' / f'mom_talib_{period}.json'}")


def generate_roc_vectors(test_data: list[float], output_dir: Path):
    for period in [10, 20]:
        expected = roc_talib(test_data, period)
        vector = create_golden_vector(
            indicator="roc_talib_count",
            source="talib",
            params={"period": period},
            input_data=test_data,
            expected_output=expected,
            warmup_samples=period,
        )
        save_vector(vector, output_dir / "roc" / f"roc_talib_{period}.json")
        print(f"Generated: {output_dir / 'roc' / f'roc_talib_{period}.json'}")


def generate_adx_vectors(test_data: list[float], output_dir: Path):
    high = [x * 1.01 for x in test_data]
    low = [x * 0.99 for x in test_data]
    for period in [14, 20]:
        expected = adx_talib(high, low, test_data, period)
        vector = create_golden_vector(
            indicator="adx_talib_count",
            source="talib",
            params={"period": period},
            input_data=test_data,
            expected_output=expected,
            warmup_samples=period * 2,
        )
        save_vector(vector, output_dir / "adx" / f"adx_talib_{period}.json")
        print(f"Generated: {output_dir / 'adx' / f'adx_talib_{period}.json'}")


def generate_stochrsi_vectors(test_data: list[float], output_dir: Path):
    for period, fastk, fastd in [(14, 5, 3), (14, 3, 3)]:
        k, d = stochrsi_talib(test_data, period, fastk, fastd)
        vector = create_multi_output_golden_vector(
            indicator="stochrsi_talib_count",
            source="talib",
            params={"period": period, "fastk": fastk, "fastd": fastd},
            input_data=test_data,
            expected_outputs=[k, d],
            warmup_samples=period + fastk + fastd,
        )
        save_vector(
            vector,
            output_dir / "stochrsi" / f"stochrsi_talib_{period}_{fastk}_{fastd}.json",
        )
        print(
            f"Generated: {output_dir / 'stochrsi' / f'stochrsi_talib_{period}_{fastk}_{fastd}.json'}"
        )


def generate_ppo_vectors(test_data: list[float], output_dir: Path):
    for fast, slow in [(12, 26), (5, 20)]:
        expected = ppo_talib(test_data, fast, slow)
        vector = create_golden_vector(
            indicator="ppo_talib_count",
            source="talib",
            params={"fast": fast, "slow": slow},
            input_data=test_data,
            expected_output=expected,
            warmup_samples=slow,
        )
        save_vector(vector, output_dir / "ppo" / f"ppo_talib_{fast}_{slow}.json")
        print(f"Generated: {output_dir / 'ppo' / f'ppo_talib_{fast}_{slow}.json'}")


def generate_trix_vectors(test_data: list[float], output_dir: Path):
    for period in [15, 20]:
        expected = trix_talib(test_data, period)
        vector = create_golden_vector(
            indicator="trix_talib_count",
            source="talib",
            params={"period": period},
            input_data=test_data,
            expected_output=expected,
            warmup_samples=period * 3,
        )
        save_vector(vector, output_dir / "trix" / f"trix_talib_{period}.json")
        print(f"Generated: {output_dir / 'trix' / f'trix_talib_{period}.json'}")


def generate_kama_vectors(test_data: list[float], output_dir: Path):
    for period in [10, 30]:
        expected = kama_talib(test_data, period)
        vector = create_golden_vector(
            indicator="kama_talib_count",
            source="talib",
            params={"period": period},
            input_data=test_data,
            expected_output=expected,
            warmup_samples=period,
        )
        save_vector(vector, output_dir / "kama" / f"kama_talib_{period}.json")
        print(f"Generated: {output_dir / 'kama' / f'kama_talib_{period}.json'}")


# ── TRIMA ─────────────────────────────────────────────────────────


def trima_talib(input_data: list[float], period: int) -> list[float | None]:
    """TRIMA — Triangular Moving Average."""
    data = np.array(input_data, dtype=np.float64)
    result = talib.TRIMA(data, timeperiod=period)
    return [None if np.isnan(x) else float(x) for x in result]


def generate_trima_vectors(test_data: list[float], output_dir: Path):
    for period in [10, 20]:
        expected = trima_talib(test_data, period)
        vector = create_golden_vector(
            indicator="trima_talib_count",
            source="talib",
            params={"period": period},
            input_data=test_data,
            expected_output=expected,
            warmup_samples=period * 2 - 1,
        )
        save_vector(vector, output_dir / "trima" / f"trima_talib_{period}.json")
        print(f"Generated: {output_dir / 'trima' / f'trima_talib_{period}.json'}")


# ── DEMA ─────────────────────────────────────────────────────────


def dema_talib(input_data: list[float], period: int) -> list[float | None]:
    """DEMA — Double Exponential Moving Average."""
    data = np.array(input_data, dtype=np.float64)
    result = talib.DEMA(data, timeperiod=period)
    return [None if np.isnan(x) else float(x) for x in result]


def generate_dema_vectors(test_data: list[float], output_dir: Path):
    for period in [10, 20]:
        expected = dema_talib(test_data, period)
        vector = create_golden_vector(
            indicator="dema_talib_count",
            source="talib",
            params={"period": period},
            input_data=test_data,
            expected_output=expected,
            warmup_samples=period * 2 - 1,
        )
        save_vector(vector, output_dir / "dema" / f"dema_talib_{period}.json")
        print(f"Generated: {output_dir / 'dema' / f'dema_talib_{period}.json'}")


# ── TEMA ─────────────────────────────────────────────────────────


def tema_talib(input_data: list[float], period: int) -> list[float | None]:
    """TEMA — Triple Exponential Moving Average."""
    data = np.array(input_data, dtype=np.float64)
    result = talib.TEMA(data, timeperiod=period)
    return [None if np.isnan(x) else float(x) for x in result]


def generate_tema_vectors(test_data: list[float], output_dir: Path):
    for period in [10, 20]:
        expected = tema_talib(test_data, period)
        vector = create_golden_vector(
            indicator="tema_talib_count",
            source="talib",
            params={"period": period},
            input_data=test_data,
            expected_output=expected,
            warmup_samples=period * 3 - 2,
        )
        save_vector(vector, output_dir / "tema" / f"tema_talib_{period}.json")
        print(f"Generated: {output_dir / 'tema' / f'tema_talib_{period}.json'}")


# ── ATR ───────────────────────────────────────────────────────────


# ── OBV ────────────────────────────────────────────────────────────


def obv_talib_constant_volume(
    close: list[float], volume: float = 1.0
) -> list[float | None]:
    """OBV with constant volume — validates cumsum(sign(diff))*vol logic."""
    close_arr = np.array(close, dtype=np.float64)
    vol_arr = np.full_like(close_arr, volume)
    result = talib.OBV(close_arr, vol_arr)
    return [None if np.isnan(x) else float(x) for x in result]


# ── MFI ────────────────────────────────────────────────────────────


def mfi_talib_constant_volume(
    high: list[float],
    low: list[float],
    close: list[float],
    volume: float,
    period: int,
) -> list[float | None]:
    """MFI with configurable constant volume."""
    high_arr = np.array(high, dtype=np.float64)
    low_arr = np.array(low, dtype=np.float64)
    close_arr = np.array(close, dtype=np.float64)
    vol_arr = np.full_like(close_arr, volume)
    result = talib.MFI(high_arr, low_arr, close_arr, vol_arr, timeperiod=period)
    return [None if np.isnan(x) else float(x) for x in result]


# ── ATR ───────────────────────────────────────────────────────────


def atr_talib(
    high: list[float], low: list[float], close: list[float], period: int
) -> list[float | None]:
    """ATR — Average True Range with Wilder smoothing."""
    high_arr = np.array(high, dtype=np.float64)
    low_arr = np.array(low, dtype=np.float64)
    close_arr = np.array(close, dtype=np.float64)
    result = talib.ATR(high_arr, low_arr, close_arr, timeperiod=period)
    return [None if np.isnan(x) else float(x) for x in result]


def generate_obv_vectors(test_data: list[float], output_dir: Path):
    """Generate OBV golden vectors (constant volume)."""
    expected = obv_talib_constant_volume(test_data)
    # TA-Lib OBV starts at the first bar's volume. With constant volume=1.0,
    # the first expected value is 1.0, followed by +/- 1 steps.
    warmup = 1
    vector = create_golden_vector(
        indicator="obv_talib_count",
        source="talib",
        params={},
        input_data=test_data,
        expected_output=expected,
        warmup_samples=warmup,
    )
    output_path = output_dir / "obv" / "obv_talib.json"
    save_vector(vector, output_path)
    print(f"Generated: {output_path}")


def generate_mfi_vectors(test_data: list[float], output_dir: Path):
    """Generate MFI golden vectors (constant volume)."""
    high = [x * 1.01 for x in test_data]
    low = [x * 0.99 for x in test_data]
    for period in [14]:
        expected = mfi_talib_constant_volume(high, low, test_data, 1.0, period)
        warmup = period  # TA_MFI_Lookback = period
        vector = create_golden_vector(
            indicator="mfi_talib_count",
            source="talib",
            params={"period": period},
            input_data=test_data,
            expected_output=expected,
            warmup_samples=warmup,
        )
        output_path = output_dir / "mfi" / f"mfi_talib_{period}.json"
        save_vector(vector, output_path)
        print(f"Generated: {output_path}")


def generate_atr_vectors(test_data: list[float], output_dir: Path):
    """Generate ATR golden vectors for periods 14 and 20."""
    high = [x * 1.01 for x in test_data]
    low = [x * 0.99 for x in test_data]
    for period in [14, 20]:
        expected = atr_talib(high, low, test_data, period)
        warmup = period  # TA_ATR_Lookback = period
        vector = create_golden_vector(
            indicator="atr_talib_count",
            source="talib",
            params={"period": period},
            input_data=test_data,
            expected_output=expected,
            warmup_samples=warmup,
        )
        output_path = output_dir / "atr" / f"atr_talib_{period}.json"
        save_vector(vector, output_path)
        print(f"Generated: {output_path}")


# ── +DI / -DI ─────────────────────────────────────────────────────


def plus_di_talib(
    high: list[float], low: list[float], close: list[float], period: int
) -> list[float | None]:
    """+DI — Plus Directional Indicator."""
    high_arr = np.array(high, dtype=np.float64)
    low_arr = np.array(low, dtype=np.float64)
    close_arr = np.array(close, dtype=np.float64)
    result = talib.PLUS_DI(high_arr, low_arr, close_arr, timeperiod=period)
    return [None if np.isnan(x) else float(x) for x in result]


def minus_di_talib(
    high: list[float], low: list[float], close: list[float], period: int
) -> list[float | None]:
    """-DI — Minus Directional Indicator."""
    high_arr = np.array(high, dtype=np.float64)
    low_arr = np.array(low, dtype=np.float64)
    close_arr = np.array(close, dtype=np.float64)
    result = talib.MINUS_DI(high_arr, low_arr, close_arr, timeperiod=period)
    return [None if np.isnan(x) else float(x) for x in result]


# ── CMO ────────────────────────────────────────────────────────────


def cmo_talib(input_data: list[float], period: int) -> list[float | None]:
    """CMO — Chande Momentum Oscillator."""
    data = np.array(input_data, dtype=np.float64)
    result = talib.CMO(data, timeperiod=period)
    return [None if np.isnan(x) else float(x) for x in result]


# ── LINEARREG_SLOPE ────────────────────────────────────────────────


def linearreg_slope_talib(
    input_data: list[float],
    period: int,
) -> list[float | None]:
    """LINEARREG_SLOPE — Linear Regression Slope."""
    data = np.array(input_data, dtype=np.float64)
    result = talib.LINEARREG_SLOPE(data, timeperiod=period)
    return [None if np.isnan(x) else float(x) for x in result]


# ── Generator functions ────────────────────────────────────────────


def generate_plus_di_vectors(test_data: list[float], output_dir: Path):
    """Generate +DI golden vectors for periods 14 and 20."""
    high = [x * 1.01 for x in test_data]
    low = [x * 0.99 for x in test_data]
    for period in [14, 20]:
        expected = plus_di_talib(high, low, test_data, period)
        warmup = period  # TA_PLUS_DI_Lookback = period
        vector = create_golden_vector(
            indicator="plus_di_talib_count",
            source="talib",
            params={"period": period},
            input_data=test_data,
            expected_output=expected,
            warmup_samples=warmup,
        )
        output_path = output_dir / "plus_di" / f"plus_di_talib_{period}.json"
        save_vector(vector, output_path)
        print(f"Generated: {output_path}")


def generate_minus_di_vectors(test_data: list[float], output_dir: Path):
    """Generate -DI golden vectors for periods 14 and 20."""
    high = [x * 1.01 for x in test_data]
    low = [x * 0.99 for x in test_data]
    for period in [14, 20]:
        expected = minus_di_talib(high, low, test_data, period)
        warmup = period
        vector = create_golden_vector(
            indicator="minus_di_talib_count",
            source="talib",
            params={"period": period},
            input_data=test_data,
            expected_output=expected,
            warmup_samples=warmup,
        )
        output_path = output_dir / "minus_di" / f"minus_di_talib_{period}.json"
        save_vector(vector, output_path)
        print(f"Generated: {output_path}")


def generate_cmo_vectors(test_data: list[float], output_dir: Path):
    """Generate CMO golden vectors for periods 14 and 20."""
    for period in [14, 20]:
        expected = cmo_talib(test_data, period)
        warmup = period + 1  # TA_CMO_Lookback = period + 1
        vector = create_golden_vector(
            indicator="cmo_talib_count",
            source="talib",
            params={"period": period},
            input_data=test_data,
            expected_output=expected,
            warmup_samples=warmup,
        )
        save_vector(vector, output_dir / "cmo" / f"cmo_talib_{period}.json")
        print(f"Generated: {output_dir / 'cmo' / f'cmo_talib_{period}.json'}")


def generate_linearreg_slope_vectors(test_data: list[float], output_dir: Path):
    """Generate LINEARREG_SLOPE golden vectors for periods 14 and 20."""
    for period in [14, 20]:
        expected = linearreg_slope_talib(test_data, period)
        warmup = period
        vector = create_golden_vector(
            indicator="linearreg_slope_talib_count",
            source="talib",
            params={"period": period},
            input_data=test_data,
            expected_output=expected,
            warmup_samples=warmup,
        )
        save_vector(
            vector,
            output_dir / "linearreg_slope" / f"linearreg_slope_talib_{period}.json",
        )
        print(
            f"Generated: {output_dir / 'linearreg_slope' / f'linearreg_slope_talib_{period}.json'}"
        )


def main():
    parser = argparse.ArgumentParser(description="Generate TA-Lib golden vectors")  # type: ignore[no-untyped-def]
    _ = parser.add_argument("--indicator", help="Specific indicator to generate")
    _ = parser.add_argument(
        "--period", type=int, help="Period for single-output indicators"
    )
    _ = parser.add_argument("--all", action="store_true", help="Generate all vectors")
    _ = parser.add_argument(
        "--output-dir", type=str, default="fixtures", help="Output directory"
    )
    _ = parser.add_argument(
        "--samples", type=int, default=200, help="Number of test samples"
    )

    args = parser.parse_args()

    samples: int = args.samples  # type: ignore[no-any-explicit]
    test_data = generate_test_data(samples)
    output_dir = Path(args.output_dir)  # type: ignore[no-any-explicit]

    if args.all:  # type: ignore[no-any-explicit]
        print("Generating all TA-Lib golden vectors...")
        generate_rsi_vectors(test_data, output_dir)  # type: ignore[no-any-explicit]
        generate_ema_vectors(test_data, output_dir)
        generate_sma_vectors(test_data, output_dir)
        generate_wma_vectors(test_data, output_dir)
        generate_macd_vectors(test_data, output_dir)
        generate_bollinger_vectors(test_data, output_dir)
        generate_stochastic_vectors(test_data, output_dir)
        generate_williams_r_vectors(test_data, output_dir)
        generate_cci_vectors(test_data, output_dir)
        generate_zscore_vectors(test_data, output_dir)
        generate_mom_vectors(test_data, output_dir)
        generate_roc_vectors(test_data, output_dir)
        generate_adx_vectors(test_data, output_dir)
        generate_stochrsi_vectors(test_data, output_dir)
        generate_ppo_vectors(test_data, output_dir)
        generate_trix_vectors(test_data, output_dir)
        generate_kama_vectors(test_data, output_dir)
        generate_trima_vectors(test_data, output_dir)
        generate_dema_vectors(test_data, output_dir)
        generate_tema_vectors(test_data, output_dir)
        generate_atr_vectors(test_data, output_dir)
        generate_plus_di_vectors(test_data, output_dir)
        generate_minus_di_vectors(test_data, output_dir)
        generate_obv_vectors(test_data, output_dir)
        generate_mfi_vectors(test_data, output_dir)
        generate_cmo_vectors(test_data, output_dir)
        generate_linearreg_slope_vectors(test_data, output_dir)
        print("Done!")
    elif args.indicator:  # type: ignore[no-any-explicit]
        # Generate specific indicator
        if args.indicator == "rsi" and args.period:  # type: ignore[no-any-explicit]
            _ = generate_rsi_vectors(test_data, output_dir)
        elif args.indicator == "ema" and args.period:
            _ = generate_ema_vectors(test_data, output_dir)
        else:
            print(f"Unknown indicator or missing period: {args.indicator}")
    else:
        parser.print_help()


if __name__ == "__main__":
    main()
