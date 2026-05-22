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

pub mod checkpoint;
pub mod ops;
pub mod prelude;
pub mod shapes;
