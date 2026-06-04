//! Signal system for tflo - extensible and composable with combinators.
//!
//! This module provides the core signal abstraction `Signal<TMode, TPayload>`
//! that enables domain-specific signal types while maintaining composability
//! with stream combinators.
//!
//! # Note on `ThresholdCrossEventMode` duplication
//!
//! [`ThresholdCrossEventMode`] is defined here **and** in
//! `tflo_ops::events::ThresholdCrossEventMode`. They are distinct types.
//!
//! The copy here is kept because:
//!
//! 1. It is the `TMode` parameter for [`Signal`] and the [`EdgeSignal`] type
//!    alias — both are part of `tflo-core`'s public API surface.
//! 2. `wasm.rs` (the `tflo-wasm` crate) references it directly for the WASM bridge's
//!    `CompiledGraph<_, ThresholdCrossEventMode, _>` output extraction.
//! 3. Removing it would be a breaking change to any downstream crate that
//!    uses `tflo_core::event::ThresholdCrossEventMode` as an `EventMode`
//!    type parameter with `Signal`.
//!
//! The `tflo-ops` copy is used exclusively by the detector operators in
//! `tflo_ops::ops::detectors` and is the type returned by `.cross()` /
//! `.cross_above()` / `.cross_below()`. The two copies have the same repr
//! and semantics but are not interchangeable without a `From` conversion.

pub mod signal;

pub use crate::event::signal::{
    EdgeSignal, EventMode, PulseEventMode, PulseMetadata, PulseSignal, Signal,
    ThresholdCrossEventMode, ZoneEventMode, ZoneSignal,
};
