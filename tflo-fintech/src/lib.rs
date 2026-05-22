//! Financial technical-analysis indicators for the `tflo` temporal event processing engine.
//!
//! `tflo-fintech` is the finance domain plugin for [`tflo-core`]. It layers
//! technical-analysis indicators — MACD, Stochastic, Williams %R, ADX, ATR,
//! KAMA, and more — onto the generic `tflo` computation graph.
//!
//! Indicators are exposed two ways:
//!
//! - [`FintechIndicators`](composites::FintechIndicators) — an extension trait
//!   on `Comp` providing the full indicator set (`macd_n`, `adx_n`, `kama_n`, …).
//! - [`FintechAliases`](aliases::FintechAliases) — finance-named aliases for
//!   generic outlier/trend operations that live in `tflo-core`
//!   (`bollinger_bands` → `deviation_band`, `drawdown` → `peak_decline`, …).
//!
//! Bring both into scope with `use tflo_fintech::prelude::*`.

#![warn(missing_docs)]

pub mod aliases;
pub mod composites;
pub mod nodes;

/// Convenient re-exports: the indicator and alias traits plus the custom node types.
pub mod prelude {
    pub use crate::aliases::FintechAliases;
    pub use crate::composites::FintechIndicators;
    pub use crate::nodes::{AdxNode, AtrNode, KamaNode, MinusDiNode, PlusDiNode};
}
