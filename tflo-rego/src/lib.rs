#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing))]
// The `into_rego_input` conversion trait deliberately borrows rather than
// consuming `self`.
#![allow(clippy::wrong_self_convention)]
//! # tflow-rego
//!
//! OPA/Rego policy engine integration for tflow.
//!
//! This crate provides policy-based filtering and decision-making using
//! the Rego policy language from Open Policy Agent (OPA).
//!
//! ## Quick Start
//!
//! ```ignore
//! use tflo_rego::prelude::*;
//!
//! // Load policies
//! let engine = PolicyEngine::new()?;
//! engine.add_policy("spectrum", r#"
//!     package spectrum
//!     
//!     default allow = false
//!     
//!     allow {
//!         input.snr > 10.0
//!         not protected_band
//!     }
//!     
//!     protected_band {
//!         input.freq_mhz >= 118.0
//!         input.freq_mhz <= 137.0
//!     }
//! "#)?;
//!
//! // Filter using policy
//! let allowed: Vec<Detection> = detections.into_iter()
//!     .rego_filter(&engine, "data.spectrum.allow")
//!     .collect();
//! ```

#![warn(missing_docs)]
#![warn(missing_debug_implementations)]
#![deny(unsafe_code)]

pub mod config;
pub mod context;
pub mod error;
pub mod filter;
pub mod policy;
pub mod policy_loader;
pub mod traits;
pub mod value_codec;

/// Prelude for convenient imports
pub mod prelude {
    pub use crate::error::{RegoError, RegoResult};
    pub use crate::filter::RegoFilterExt;
    pub use crate::policy::PolicyEngine;
    pub use crate::traits::IntoRegoInput;
}
