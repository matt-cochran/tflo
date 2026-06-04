# tflo-fintech

Financial technical-analysis indicators for the [tflo](../README.md)
temporal event processing engine.

`tflo-fintech` is the finance domain plugin for `tflo-core`. It layers
technical-analysis indicators onto the generic `tflo` computation graph — none
of this code lives in `tflo-core` itself.

## Usage

```rust
use tflo_core::prelude::*;
use tflo_fintech::prelude::*;

let macd = prices
    .into_iter()
    .tflo(|t| {
        t.timestamp(|p| p.ts);
        let close = t.prop(|p| p.close);
        close.macd_n(12, 26, 9)
    })
    .collect::<Vec<_>>();
```

## What it provides

- **`FintechIndicators`** — extension trait on `Comp`: MACD, Stochastic,
  Williams %R, CCI, ATR, TRIMA/DEMA/TEMA, VWAP, OBV, MFI, CMO,
  linear-regression slope, PPO, TRIX, StochRSI, and the OHLC indicators below.
- **Runtime `CustomNode` indicators** — ADX, +DI, -DI, Wilder ATR, KAMA. These
  need TA-Lib-exact lookback and seeding, so they are real stateful graph
  nodes built on `tflo-core`'s `CustomNode` plugin API.
- **`FintechAliases`** — finance-named aliases for the generic outlier/trend
  operations that live in `tflo-core` (`bollinger_bands` → `deviation_band`,
  `drawdown` → `peak_decline`, `roc_n` → `rate_of_change`, `mom_n` →
  `momentum`).

All indicators are validated bit-exact (1e-6) against the TA-Lib C library via
a golden-vector test suite under `tests/golden/`.

## License

Licensed under either of MIT or Apache-2.0 at your option.
