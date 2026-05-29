#![cfg_attr(
    test,
    allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::indexing_slicing,
        clippy::arithmetic_side_effects,
        clippy::missing_panics_doc,
        clippy::missing_errors_doc
    )
)]
#![deny(clippy::print_stdout)]
#![warn(missing_docs)]

//! Closure-based event-pattern matching for the
//! [`tflo`](https://github.com/matt-cochran/tflo) temporal event processing
//! engine.
//!
//! `tflo-cep` is the CEP sibling crate: it composes the typed `Signal`
//! stream the rest of the engine produces into multi-event domain signals
//! ("user added to cart then did not purchase within 5 minutes"). The whole
//! crate is closure-based — it slots cleanly next to `tflo-cel`,
//! `tflo-rhai`, and `tflo-rego` in the existing iterator-adapter extension
//! model, and compiles for `wasm32-unknown-unknown` the same way.
//!
//! # Quick start
//!
//! ```rust
//! use tflo_cep::prelude::*;
//! use std::time::Duration;
//!
//! #[derive(Clone, Debug)]
//! struct Event {
//!     ts: i64,
//!     action: &'static str,
//! }
//!
//! // "Added to cart but did not purchase within 5 minutes."
//! let abandoned_cart = Pattern::<Event>::new("abandoned_cart")
//!     .timestamp(|e| e.ts)
//!     .when(|e| e.action == "add_to_cart")
//!     .not_then(|e| e.action == "purchase")
//!     .within(Duration::from_secs(300))
//!     .emit(|m| format!("abandoned: cart added at ts={}", m.first().ts))
//!     .expect("pattern is well-formed");
//!
//! let events = vec![
//!     Event { ts: 0,      action: "add_to_cart" },
//!     Event { ts: 60_000, action: "view_page" },
//!     // No purchase within 5 minutes — the not_then fires.
//! ];
//!
//! let signals: Vec<String> = events.into_iter()
//!     .match_pattern(abandoned_cart)
//!     .collect();
//!
//! assert_eq!(signals.len(), 1);
//! ```
//!
//! See [`Pattern`] for the full builder surface.

pub mod engine;
mod matched;
mod pattern;
mod runtime;

pub use matched::Match;
pub use pattern::{ArcEmit, ArcPredicate, ArcTimestamp, Pattern, PatternError};
pub use runtime::{MatchPatternIter, PatternIter};

/// Re-exports for convenient `use tflo_cep::prelude::*;` access.
pub mod prelude {
    pub use crate::{Match, Pattern, PatternError, PatternIter};
}
