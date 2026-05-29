//! # tflo
#![deny(clippy::print_stdout)] // library code must not write to stdout
//!
//! tflo (temporal flow) — a temporal event processing engine for domain-driven
//! applications. Model your domain events; layer streaming temporal analysis
//! on top.
//!
//! This crate is the Rust implementation; a TypeScript/Node port of the same
//! engine is also available.
//!
//! This library provides a fluent API for defining time-windowed computations
//! over streaming data, with support for:
//!
//! - **Time-based windows**: SMA, EMA, std, max, min, sum, count over duration
//! - **Cross detection**: Signal generation when values cross thresholds
//! - **Lookback**: Previous values, lag, delta
//! - **Arithmetic**: Composition of computations
//! - **Stream combinators**: Merge, join, batch, dedupe, rate limit
//!
//! # Custom Composite Algorithms
//!
//! Composite algorithms are graph-building helpers built entirely from existing
//! [`Comp`] methods. They do **not** require new runtime nodes, internal crate
//! modifications, or access to private APIs (`Node`, `NodeOp`, `NodeState`,
//! `ValueStore`, etc.).
//!
//! This is the **recommended path** for custom algorithms that can be expressed
//! using existing operations:
//!
//! - **Define an extension trait** on `Comp<R, f64>` in your own crate.
//! - **Implement using only public APIs** from `tflo_core::prelude::*`.
//! - **Use inside `.tflo(|t| { ... })`** just like any built-in method.
//! - **Return single values or tuples** — any tuple up to 8 elements
//!   implements `Compile<R>` and can be the return value of the `.tflo()` closure.
//!
//! ## Example
//!
//! ```rust
//! use tflo_core::prelude::*;
//!
//! // Step 1: Define an extension trait
//! pub trait MyExt<R: 'static> {
//!     fn spread_ratio(&self, other: &Comp<R, f64>) -> Comp<R, f64>;
//!     fn log_scaled(&self) -> Comp<R, f64>;
//! }
//!
//! // Step 2: Implement using only public APIs (closure ops)
//! impl<R: 'static> MyExt<R> for Comp<R, f64> {
//!     fn spread_ratio(&self, other: &Comp<R, f64>) -> Comp<R, f64> {
//!         (self - other) / other
//!     }
//!
//!     fn log_scaled(&self) -> Comp<R, f64> {
//!         self.map_f64(|x| if x > 0.0 { x.ln() } else { f64::NAN })
//!     }
//! }
//!
//! // Step 3: Use inside `.tflo()` like any built-in
//! fn example(ticks: Vec<(i64, f64)>) -> Vec<f64> {
//!     ticks.into_iter()
//!         .tflo(|t| {
//!             t.timestamp(|x| x.0);
//!             let price = t.prop(|x| x.1);
//!             price.log_scaled()
//!         })
//!         .collect()
//! }
//! ```
//!
//! ## Key points
//!
//! - **Composite ≠ Primitive**: Composite algorithms build graphs. They never
//!   add new runtime behavior. For truly new runtime nodes, see the `Node` enum
//!   and the functional graph primitives ([`map_f64`], [`scan_f64`], etc.).
//! - **No private API access**: Your extension trait should only use public
//!   [`Comp`] methods and public types from `tflo_core::prelude::*`.
//! - **Tuple outputs**: Return `(Comp<R, f64>, Comp<R, f64>)` or larger tuples
//!   from your composite methods. They implement `Compile<R>` and work as
//!   `.tflo()` return values.
//! - **Window genericity**: Use `impl Into<Window>` to accept both time-based
//!   ([`Duration`]) and count-based ([`usize`]) windows.
//! - **Chaining**: Composite methods compose freely with each other and with
//!   closure primitives. For example: `price.log_scaled().map_f64(|x| x * 2.0).spread_ratio(&baseline)`.
//!
//! See the full example at `tflo-examples/examples/custom-composite`
//! (../../tflo-examples/examples/custom-composite) for a complete, runnable demo.
//!
//! # Custom Functional Graph Primitives
//!
//! For algorithms that cannot be expressed cleanly as composites — custom
//! formulas, state machines, or time-decayed filters — `tflo-core` provides
//! closure-based functional graph primitives on `Comp<R, f64>` (see [`Comp`]):
//!
//! | Method | Signature | Purpose |
//! |---|---|---|
//! | [`map_f64`] | `\|x\| -> f64` | Stateless unary transform |
//! | [`map2_f64`] | `(&other, \|x, y\| -> f64)` | Stateless binary transform |
//! | [`filter_f64`] | `\|x\| -> bool` | Keep/drop based on predicate |
//! | [`filter_map_f64`] | `\|x\| -> Option<f64>` | Transform + optionally suppress |
//! | [`scan_f64`] | `(\|\| init, \|state, x\| -> f64)` | Stateful unary scan |
//! | [`scan2_f64`] | `(&other, \|\| init, \|state, x, y\| -> f64)` | Stateful binary scan |
//!
//! All methods accept optional `.named("...")` metadata for graph-plan
//! readability.
//!
//! ```rust
//! use tflo_core::prelude::*;
//!
//! #[derive(Clone)]
//! struct Tick { ts: i64, price: f64, volume: f64 }
//!
//! let ticks = vec![
//!     Tick { ts: 1000, price: 100.0, volume: 10.0 },
//!     Tick { ts: 2000, price: 101.0, volume: 12.0 },
//!     Tick { ts: 3000, price: 99.0, volume: 15.0 },
//! ];
//!
//! // Stateless unary transform
//! let results: Vec<f64> = ticks.clone().into_iter()
//!     .tflo(|t| {
//!         t.timestamp(|x| x.ts);
//!         let price = t.prop(|x| x.price);
//!         price.map_f64(|x| x.max(0.0))
//!     })
//!     .collect();
//!
//! // Stateful custom EMA with closure state
//! let results: Vec<f64> = ticks.into_iter()
//!     .tflo(|t| {
//!         t.timestamp(|x| x.ts);
//!         let price = t.prop(|x| x.price);
//!         price.scan_f64(
//!             || 0.0,
//!             |state, x| { *state = 0.9 * *state + 0.1 * x; *state },
//!         )
//!     })
//!     .collect();
//! ```
//!
//! ### When to use which approach
//!
//! | Approach | Best for |
//! |---|---|
//! | **Extension traits** (composite) | Algorithms expressible using existing [`Comp`] operations |
//! | **Functional primitives** (closures) | Custom formulas, state machines, nonlinear transforms |
//! | **Built-in core nodes** | Common high-value algorithms with first-class optimization |
//!
//! > **Note on closures**: [`map_f64`], [`map2_f64`], [`filter_f64`], [`filter_map_f64`] are
//! > per-record transforms. [`scan_f64`] and [`scan2_f64`] are per-record state machines —
//! > their state advances once per input record, not once per window. For rolling
//! > count-windowed or time-windowed behavior, compose with catalog operators from
//! > `tflo-ops` (the operator catalog crate).
//!
//! > **Note on naming**: Closure bodies are opaque to `tflo-core`; even though Rust compiles
//! > them, `tflo-core` cannot inspect or display the formula inside. Optional
//! > `.named("...")` metadata improves graph-plan and debug output. Names have no
//! > semantic effect — skip them for clear local formulas, use them when graph plans or
//! > diagnostics need human-readable node names.
//!
//! # Quick Start
//!
//! ```rust
//! use tflo_core::prelude::*;
//!
//! #[derive(Clone)]
//! struct Tick {
//!     ts: i64,
//!     price: f64,
//! }
//!
//! let ticks: Vec<Tick> = vec![
//!     Tick { ts: 1000, price: 100.0 },
//!     Tick { ts: 2000, price: 101.0 },
//!     Tick { ts: 3000, price: 99.0 },
//! ];
//!
//! // Apply a stateless transform using a closure primitive
//! let results: Vec<_> = ticks.into_iter()
//!     .tflo(|t| {
//!         t.timestamp(|x| x.ts);
//!         let price = t.prop(|x| x.price);
//!         price.map_f64(|x| x * 2.0)
//!     })
//!     .collect();
//! ```
//!
//! [`Comp`]: crate::comp::Comp
//! [`map_f64`]: crate::comp::Comp::map_f64
//! [`map2_f64`]: crate::comp::Comp::map2_f64
//! [`filter_f64`]: crate::comp::Comp::filter_f64
//! [`filter_map_f64`]: crate::comp::Comp::filter_map_f64
//! [`scan_f64`]: crate::comp::Comp::scan_f64
//! [`scan2_f64`]: crate::comp::Comp::scan2_f64
//! [`Duration`]: std::time::Duration

#![warn(missing_docs)]
#![warn(missing_debug_implementations)]
#![deny(unsafe_code)]
// Test code may freely `unwrap`/`expect`/`panic!` — the panic-freedom lints
// only police production code paths.
#![cfg_attr(
    test,
    allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::indexing_slicing,
        clippy::arithmetic_side_effects,
        clippy::let_underscore_must_use,
        clippy::map_err_ignore
    )
)]
// ── Phase 5 intent-allows for the numeric streaming engine ─────────────
// `tflo-core` is the engine: timestamps cross between `i64` and `usize`
// constantly, window arithmetic moves integer counts into `f64`, and
// the typed-absence model compares `f64` exactly against the warming
// sentinel. These lints fire intentionally here.
#![allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    clippy::cast_lossless,
    clippy::float_cmp,
    clippy::suboptimal_flops
)]

pub mod adapter;
pub mod builder;
pub mod combinators;
pub mod comp;
pub mod compile;
pub mod duration;
pub mod error;
pub mod event;
pub mod iter_ext;
pub mod keyed;
pub mod metrics;
pub mod operator;
pub use operator::{BivariateWindow, WindowPrimitive};
#[cfg(feature = "async")]
pub mod dedup;
pub mod pipeline;
pub mod scalar;
/// Behavioral contracts and guarantees provided by the computation graph
/// execution layer. **Recovered orphan** — this module was written for
/// the initial commit and never wired in; resurfaced here so the docs
/// are actually visible. See the file's `# Audit (post-Phase-1)` block
/// for sections that need refresh against current code.
pub mod semantics;
pub mod shard;
#[cfg(feature = "async")]
pub mod state;
/// Time point trait for generic time type abstraction.
pub mod timepoint;
pub mod timer;
pub mod validation;
pub mod window;

#[cfg(feature = "async")]
pub mod r#async;

/// Prelude for convenient imports
pub mod prelude {
    pub use crate::adapter::{
        Checkpoint, CheckpointId, CheckpointPolicy, Cursor, CursorStore, KeyedMetrics, NoopMetrics,
    };
    pub use crate::builder::TFlowBuilder;
    pub use crate::combinators::{
        GroupByKey, batch_by_time, dedupe_by_key, merge_by_timestamp, partition, rate_limit,
        window_join,
    };
    pub use crate::comp::Comp;
    pub use crate::compile::{
        Absent, CompiledGraph, Computed, ExtractOutput, GraphPlan, GraphStateSummary, NodeOutput,
        ValueStore, finite_or_warming,
    };
    pub use crate::duration::IntoDuration;
    pub use crate::error::{ComputeError, ComputeResult, TFloError, TFloResult};
    pub use crate::event::{
        EdgeSignal, EventMode, PulseEventMode, PulseMetadata, PulseSignal, Signal,
        ThresholdCrossEventMode, ZoneEventMode, ZoneSignal,
    };
    pub use crate::iter_ext::TFlowIteratorExt;
    pub use crate::keyed::{OutOfOrderPolicy, TFloKeyedIter};
    pub use crate::operator::{BoxedOperator, Operator, OperatorFactory, OperatorLoadError};
    pub use crate::pipeline::{
        Hybrid, HybridItem, KeyedTimestamped, PipelineContext, PipelineItem, Sequenced,
        SequencedItem, Timestamped, TimestampedItem,
    };
    pub use crate::scalar::Scalar;
    pub use crate::shard::{AssignmentEpoch, DropReason, LocalShard, ShardRouter};
    #[cfg(feature = "async")]
    pub use crate::state::{AsyncCursorStore, AsyncStateStore, CheckpointError, Checkpointer};
    pub use crate::timepoint::TimePoint;
    pub use crate::validation::ValidationOptions;
    pub use crate::validation::{
        require_finite, require_finite_opt, require_non_negative, require_not_infinite,
        require_not_nan, require_positive,
    };
    pub use crate::window::Window;
    pub use crate::window::{IntoSamples, WindowSpec, WindowSpecExt};

    #[cfg(feature = "async")]
    pub use crate::r#async::{
        TFloKeyedStream, TFloStream, TFloStreamExt, TFlowWithStream, from_iter,
    };
}
