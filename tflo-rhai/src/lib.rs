#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing, clippy::arithmetic_side_effects))]
#![deny(clippy::print_stdout)] // library code must not write to stdout
// `Arc<rhai::Engine>` shares the (`!Sync`) script engine across single-threaded
// iterator adapters; the `into_*` conversion traits deliberately borrow.
#![allow(clippy::arc_with_non_send_sync, clippy::wrong_self_convention)]
//! # tflo-rhai
//!
//! Rhai scripting integration for tflo.
//!
//! This crate provides embedded scripting capabilities using Rhai,
//! allowing runtime-configurable filtering, transformation, and
//! custom logic.
//!
//! ## Stability: beta
//!
//! Per the workspace's three-tier stability convention (see
//! `tflo-cel` crate docs for the full definition), `tflo-rhai` is
//! **beta**: the public surface (`RhaiFilterExt`, `RhaiMapExt`,
//! `IntoRhaiContext`) is covered by inline unit tests and used in
//! the `docs-scripting` example. Rhai is a `!Sync` script engine,
//! so this crate runs in single-threaded iterator adapters only;
//! multi-threaded keyed execution is not in scope.
//!
//! ## Quick Start
//!
//! ```ignore
//! use tflo_rhai::prelude::*;
//!
//! // Filter using Rhai script
//! let filtered: Vec<Detection> = detections.into_iter()
//!     .rhai_filter("snr > 10.0 && is_valid(freq)")
//!     .collect();
//!
//! // Transform using Rhai script
//! let enriched: Vec<EnrichedDetection> = detections.into_iter()
//!     .rhai_map("enrich(detection)")
//!     .collect();
//! ```

#![warn(missing_docs)]
#![warn(missing_debug_implementations)]
#![deny(unsafe_code)]

pub mod context;
pub mod error;
pub mod filter;
pub mod options;
pub mod script;
pub mod traits;
pub mod transform;
pub mod script_exec;

/// Prelude for convenient imports
pub mod prelude {
    pub use crate::error::{RhaiError, RhaiResult};
    pub use crate::filter::RhaiFilterExt;
    pub use crate::options::RhaiOptions;
    pub use crate::script::ScriptEngine;
    pub use crate::traits::IntoRhaiScope;
    pub use crate::transform::RhaiMapExt;
}
