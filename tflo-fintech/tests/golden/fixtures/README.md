# Golden Vector Fixtures

All golden vectors are generated from **TA-Lib** via
`scripts/generate_talib_vectors.py`.  There are no self-referential
"baseline" vectors.  Every vector is a correctness check against the
TA-Lib C library.

## Current Status

- **55 golden vectors** across 23 indicator types
- **1 source**: TA-Lib C library
- **55 tests** in `tests/golden/tests.rs`

## Directory Structure

```
fixtures/
├── rsi/              # RSI (periods 7, 14, 21)
├── ema/              # EMA (periods 5, 12, 26)
├── sma/              # SMA (periods 5, 20, 50)
├── wma/              # WMA (periods 5, 20)
├── macd/             # MACD (12/26/9, 8/17/9)
├── bollinger_bands/  # Bollinger Bands (20/2.0, 20/2.5)
├── stochastic/       # Stochastic (14/3, 5/3)
├── williams_r/       # Williams %R (14, 21)
├── cci/              # CCI (14, 20)
├── zscore/           # Z-Score (20, 30)
├── atr/              # ATR (14, 20)
├── plus_di/          # +DI (14, 20)
├── minus_di/         # -DI (14, 20)
├── mom/              # Momentum (10, 20)
├── roc/              # Rate of Change (10, 20)
├── ppo/              # PPO (12/26, 5/20)
├── trix/             # TRIX (15, 20)
├── kama/             # KAMA (10, 30)
├── trima/            # TRIMA (10, 20)
├── dema/             # DEMA (10, 20)
├── tema/             # TEMA (10, 20)
├── obv/              # OBV (constant volume)
├── mfi/              # MFI (14, synthetic OHLCV)
├── cmo/              # CMO (14, 20)
└── linearreg_slope/  # LINEARREG_SLOPE (14, 20)
```

## Format

Each fixture is a JSON file:

```json
{
  "metadata": {
    "indicator": "rsi_talib_count",
    "source": "talib",
    "version": "0.4.0",
    "license": "BSD",
    "provenance": "Generated from TA-Lib C library using Python bindings"
  },
  "params": { "period": 14 },
  "input": [98.34, 99.17, ...],
  "expected_output": [null, null, ..., 52.31, 54.82, ...],
  "warmup_samples": 15
}
```

For multi-output indicators (MACD, Bollinger Bands, Stochastic),
`expected_output` is an array of arrays — one per output channel.

## Generating Vectors

Requires TA-Lib C library and Python dependencies:

```bash
python3 tflo-core/tests/golden/scripts/generate_talib_vectors.py --all --output-dir tflo-core/tests/golden/fixtures
```

See `../INSTALL_TA_LIB.md` for platform-specific TA-Lib installation.

## OHLCV-Derived Fixtures

The JSON schema stores one primary `input` close series. For indicators requiring OHLCV, both the Python generator and Rust runner derive the same synthetic fields:

- `high = close * 1.01`
- `low = close * 0.99`
- `volume = 1.0`

See `../README.md` for compatibility notes on TA-Lib-specific OBV, MFI, and CMO behavior.
