//! Operator catalog for the `tflo` CEP engine.
//!
//! `tflo-ops` contains the full catalog of windowed, statistical, stateful,
//! detector, math, and composite operators. Operators are exposed as extension
//! traits on `Comp` so that call sites read naturally — e.g. `price.sma(20)`.
//!
//! Import the prelude to bring all extension traits into scope:
//!
//! ```ignore
//! use tflo_ops::prelude::*;
//! ```

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]
// ── Phase 5 intent-allows for the numeric streaming engine ─────────────
// `tflo-ops` is the operator catalog: windowed statistics, detectors,
// trackers. Every operator works in `f64`; integer counts and i64
// timestamps cross into `f64` constantly. Detectors compare against
// caller-supplied thresholds with `==`/`<`/`>`. `mul_add` rewriting
// would change rounding and break the `tflo-fintech` golden-fixture
// bit-equality suite. Allow these per-crate rather than workspace-wide
// — they are *not* universally acceptable, only inside the numeric
// engine.
#![allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    clippy::cast_lossless,
    clippy::float_cmp,
    clippy::suboptimal_flops
)]

pub mod checkpoint;
pub mod events;
pub mod ops;
pub mod prelude;
pub mod primitives;
pub mod shapes;
