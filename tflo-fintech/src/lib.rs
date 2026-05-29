#![cfg_attr(
    test,
    allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::indexing_slicing,
        clippy::arithmetic_side_effects
    )
)]
#![deny(clippy::print_stdout)]
// library code must not write to stdout
// ── Phase 5 intent-allows for the numeric streaming engine ─────────────
// `tflo-fintech` is the canonical numeric domain plugin. Bit-exact
// outputs are pinned by the golden-fixture suite; `mul_add` rewriting
// is the most dangerous change. Integer→f64 casts and exact-compare
// against thresholds are part of the indicator contract.
#![allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    clippy::cast_lossless,
    clippy::float_cmp,
    clippy::suboptimal_flops
)]
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
