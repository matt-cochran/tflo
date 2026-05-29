#![cfg_attr(
    test,
    allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::indexing_slicing,
        clippy::arithmetic_side_effects,
        clippy::let_underscore_must_use
    )
)]
#![deny(clippy::print_stdout)]
// library code must not write to stdout
// The `into_cel_context` conversion trait deliberately borrows rather than
// consuming `self`.
#![allow(clippy::wrong_self_convention)]
//! # tflo-cel
//!
//! CEL (Common Expression Language) rule engine integration for tflo.
//!
//! This crate provides runtime-configurable filtering and routing using CEL
//! expressions, allowing rules to be changed without recompilation.
//!
//! ## Stability: beta
//!
//! Per the workspace's three-tier stability convention:
//!
//! - **alpha** — API may change without notice; no test coverage SLA.
//! - **beta** — API stable across patch releases; covered by unit
//!   tests; no SLA on bug-fix turnaround for pre-1.0 releases.
//! - **stable** — API stable + SLA (post-1.0 only).
//!
//! `tflo-cel` is **beta**: the public surface (`CelFilterExt`,
//! `IntoCelContext`, `RuleEngine`) is covered by inline unit tests
//! and used in the `docs-scripting` example. It has not yet been
//! exercised under sustained production load; treat it as
//! production-eligible for use cases where CEL evaluation latency
//! is not on the critical path, and report issues for triage.
//!
//! ## Quick Start
//!
//! ```rust
//! use tflo_cel::prelude::*;
//! use cel_interpreter::Context;
//!
//! // Items must implement IntoCelContext to be filtered with CEL
//! struct Detection {
//!     power_dbm: f64,
//!     snr_db: f64,
//! }
//!
//! impl IntoCelContext for Detection {
//!     fn into_cel_context(&self) -> Context<'static> {
//!         let mut ctx = Context::default();
//!         ctx.add_variable("power_dbm", self.power_dbm).unwrap();
//!         ctx.add_variable("snr_db", self.snr_db).unwrap();
//!         ctx
//!     }
//! }
//!
//! let detections = vec![
//!     Detection { power_dbm: -70.0, snr_db: 15.0 },
//!     Detection { power_dbm: -90.0, snr_db: 5.0 },
//! ];
//!
//! // Simple filtering with CEL expressions (Result-returning, canonical).
//! let filtered: Vec<Detection> = detections.into_iter()
//!     .cel_filter_result("snr_db > 10.0 && power_dbm > -80.0")
//!     .expect("expression compiles")
//!     .filter_map(Result::ok)
//!     .collect();
//!
//! assert_eq!(filtered.len(), 1);
//! ```

#![warn(missing_docs)]
#![warn(missing_debug_implementations)]
#![deny(unsafe_code)]

pub mod config;
pub mod context;
pub mod error;
pub mod filter;
pub mod router;
pub mod rule_engine;
pub mod rule_loader;
pub mod traits;

/// WebAssembly bridge (only compiled for wasm32 targets).
#[cfg(target_arch = "wasm32")]
pub mod wasm;

/// Prelude for convenient imports
pub mod prelude {
    pub use crate::error::{CelError, CelResult};
    pub use crate::filter::{CelFilterExt, CelFilterResult, CelOptions};
    pub use crate::router::CelRouterExt;
    pub use crate::rule_engine::{Action, CompiledRule, RuleEngine};
    pub use crate::traits::IntoCelContext;
}
