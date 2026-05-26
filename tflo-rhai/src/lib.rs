#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]
// `Arc<rhai::Engine>` shares the (`!Sync`) script engine across single-threaded
// iterator adapters; the `into_*` conversion traits deliberately borrow.
#![allow(clippy::arc_with_non_send_sync, clippy::wrong_self_convention)]
//! # tflow-rhai
//!
//! Rhai scripting integration for tflow.
//!
//! This crate provides embedded scripting capabilities using Rhai,
//! allowing runtime-configurable filtering, transformation, and
//! custom logic.
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
pub mod script;
pub mod traits;
pub mod transform;
pub mod script_exec;

/// Prelude for convenient imports
pub mod prelude {
    pub use crate::error::{RhaiError, RhaiResult};
    pub use crate::filter::RhaiFilterExt;
    pub use crate::script::ScriptEngine;
    pub use crate::traits::IntoRhaiScope;
    pub use crate::transform::RhaiMapExt;
}
