//! # Pipeline Architecture
//!
//! This module provides the core abstractions for type-safe, compositional
//! stream processing using a **monadic pipeline pattern**.
//!
//! ## Core Types
//!
//! - [`PipelineContext`]: Trait for context types that flow through pipelines
//! - [`PipelineItem`]: A value paired with its context (the "monad")
//! - [`Timestamped`]: Time-based context (milliseconds since epoch)
//! - [`Sequenced`]: Sequence-based context (monotonic counter)
//! - [`Hybrid`]: Both timestamp and sequence number
//!
//! ## Design Rationale
//!
//! Stream processing often requires metadata (timestamps, sequence numbers,
//! correlation IDs) to flow alongside computed values. The naive approach—
//! passing tuples everywhere—leads to boilerplate and coupling.
//!
//! This module uses a **monadic pattern** where:
//!
//! 1. **Context is implicit**: Operations like `map`, `filter`, and `fold`
//!    automatically preserve context without explicit threading
//! 2. **Composition is type-safe**: The context type `C` is a type parameter,
//!    preventing accidental mixing of incompatible pipelines
//! 3. **Extension is easy**: Custom contexts implement [`PipelineContext`]
//!
//! ### The Monad Pattern
//!
//! `PipelineItem<C, T>` is a monad with:
//! - **Unit**: `PipelineItem::new(ctx, value)`
//! - **Bind**: `and_then` (flatMap)
//! - **Functor**: `map` (fmap)
//!
//! This enables declarative pipeline composition:
//!
//! ```ignore
//! graph
//!     .map(|x| x * 2.0)           // Context preserved
//!     .filter(|x| x > threshold)  // Context preserved
//!     .fold(0.0, |acc, x| acc + x) // Context preserved
//! ```
//!
//! ## Prior Art
//!
//! This design draws from established patterns in functional programming:
//!
//! | Library | Language | Pattern |
//! |---------|----------|---------|
//! | **Conduit** | Haskell | Streaming with resource safety |
//! | **Pipes** | Haskell | Bidirectional streaming |
//! | **fs2** | Scala | Functional streams with effects |
//! | **tower** | Rust | Service middleware composition |
//! | **futures-rs** | Rust | Async stream combinators |
//! | **itertools** | Rust | Iterator adapters |
//!
//! ### Key Insights Borrowed
//!
//! - **From Conduit/Pipes**: Context flows through pipeline stages implicitly
//! - **From fs2**: Typed effects with lawful composition
//! - **From tower**: Layered transformation with preserved metadata
//! - **From futures-rs**: Zero-cost abstraction via static dispatch
//!
//! ## When to Use Each Context
//!
//! | Context | Use Case | `ordering_key()` |
//! |---------|----------|------------------|
//! | [`Timestamped`] (`i64`) | Time-windowed aggregations, event-time processing | Timestamp (ms) |
//! | [`Sequenced`] | Count-based windows, index-based lookback | Sequence number (`i64`) |
//! | [`Hybrid`] | Debugging, audit trails | Timestamp |
//! | Custom | Domain-specific metadata (session ID, etc.) | Your choice |
//!
//! ## Example: Multi-Stage Pipeline
//!
//! ```ignore
//! // Stage 1: Compute moving average
//! let stage1: CompiledGraph<Tick, f64, Timestamped> = builder
//!     .add_node(|b| b.sma(Duration::from_secs(300)))
//!     .build();
//!
//! // Stage 2: Generate signals from the moving average
//! // Note: Input is PipelineItem<Timestamped, f64>, giving access to timestamp
//! let stage2: CompiledGraph<PipelineItem<Timestamped, f64>, Signal, Timestamped> = ...;
//!
//! // Compose into single pipeline
//! let pipeline = stage1.pipe(stage2);
//!
//! for tick in ticks {
//!     if let Some(item) = pipeline.step(&tick) {
//!         // item.ctx is the original tick's timestamp
//!         // item.value is the Signal from stage 2
//!     }
//! }
//! ```
//!
//! ## Design
//!
//! The pipeline carries a [`PipelineContext`] (timestamp, key, or sequence
//! number) alongside each computed value, so downstream stages keep the
//! attribution of every event.

mod types;

use std::sync::atomic::AtomicU64;
pub use types::{Hybrid, KeyedTimestamped, PipelineItem, Sequenced};

// ============================================================================
// PIPELINE CONTEXT TRAIT
// ============================================================================

/// Trait for pipeline contexts that carry metadata through composition.
///
/// A pipeline context represents metadata that flows alongside computed values
/// through a processing pipeline. The ordering key (`i64`) is used for windowing
/// and buffering operations - it can represent timestamps, sequence numbers,
/// or any other ordering value.
///
/// # Required Methods
///
/// - `from_ordering_key`: Create context from an ordering key (timestamp, sequence, etc.)
/// - `ordering_key`: Get the ordering key for windowing operations
///
/// # Example: Custom Context
///
/// ```rust
/// use tflo_core::pipeline::PipelineContext;
///
/// #[derive(Clone, Debug)]
/// struct TradingContext {
///     ordering_key: i64,
///     session_id: u32,
/// }
///
/// impl PipelineContext for TradingContext {
///     fn from_ordering_key(key: i64) -> Self {
///         TradingContext {
///             ordering_key: key,
///             session_id: 1, // Would normally derive from key
///         }
///     }
///     
///     fn ordering_key(&self) -> i64 {
///         self.ordering_key
///     }
/// }
/// ```
pub trait PipelineContext: Clone + Send + Sync + 'static {
    /// Create a context from an ordering key.
    ///
    /// The ordering key is an `i64` that represents the ordering value for this record.
    /// It can be:
    /// - A timestamp (milliseconds since epoch) for time-based windows
    /// - A sequence number for count-based windows
    /// - Any other ordering value for custom windowing logic
    fn from_ordering_key(key: i64) -> Self;

    /// Get the ordering key for windowing and buffering operations.
    ///
    /// This key is used by time-based and count-based windows to determine
    /// which values fall within the window. The interpretation depends on context:
    /// - Time-based windows: milliseconds since epoch
    /// - Count-based windows: sequence number / ordinal
    fn ordering_key(&self) -> i64;
}

// ============================================================================
// BUILT-IN CONTEXT TYPES
// ============================================================================

/// Time-based pipeline context (type alias for `i64`).
///
/// This is simply an `i64` representing milliseconds since epoch. The ordering key
/// is the timestamp itself. This is the default context type for most temporal computations.
///
/// # Use Cases
///
/// - Time-windowed aggregations (SMA, EMA over duration)
/// - Event-time processing
/// - Joining streams by time proximity
///
/// # Example
///
/// ```rust
/// use tflo_core::pipeline::{Timestamped, PipelineContext};
///
/// let ctx: Timestamped = Timestamped::from_ordering_key(1699000000000); // Nov 2023
/// assert_eq!(ctx, 1699000000000);
/// assert_eq!(ctx.ordering_key(), 1699000000000);
/// ```
pub type Timestamped = i64;

impl PipelineContext for i64 {
    #[inline]
    fn from_ordering_key(key: i64) -> Self {
        key
    }

    #[inline]
    fn ordering_key(&self) -> i64 {
        *self
    }
}

/// Global sequence counter for `Sequenced` context.
pub(super) static SEQUENCE_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Global sequence counter for `Hybrid` context.
pub(super) static HYBRID_COUNTER: AtomicU64 = AtomicU64::new(0);

// ============================================================================
// PIPELINE ITEM
// ============================================================================

// ============================================================================
// TYPE ALIASES
// ============================================================================

/// A pipeline item with timestamp context.
pub type TimestampedItem<T> = PipelineItem<Timestamped, T>;

/// A pipeline item with sequence context.
pub type SequencedItem<T> = PipelineItem<Sequenced, T>;

/// A pipeline item with hybrid context.
pub type HybridItem<T> = PipelineItem<Hybrid, T>;

// ============================================================================
// EXTRACTORS FOR PIPELINE ITEMS
// ============================================================================

// ============================================================================
// TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_timestamped_context() {
        let ctx: Timestamped = Timestamped::from_ordering_key(1000);
        assert_eq!(ctx, 1000);
        assert_eq!(ctx.ordering_key(), 1000);
    }

    #[test]
    fn test_sequenced_context() {
        Sequenced::reset_counter();
        let ctx1 = Sequenced::from_ordering_key(0);
        let ctx2 = Sequenced::from_ordering_key(0);
        assert_eq!(ctx1.0, 0);
        assert_eq!(ctx2.0, 1);
        assert!(ctx1.0 < ctx2.0);
    }

    #[test]
    fn test_hybrid_context() {
        Hybrid::reset_counter();
        let ctx = Hybrid::from_ordering_key(5000);
        assert_eq!(ctx.ts, 5000);
        assert_eq!(ctx.ordering_key(), 5000);
    }

    #[test]
    fn test_pipeline_item_map() {
        let item = PipelineItem::new(1000_i64, 10.0_f64);
        let mapped = item.map(|x| x * 2.0);
        assert_eq!(mapped.value, 20.0);
        assert_eq!(mapped.ctx, 1000);
    }

    #[test]
    fn test_pipeline_item_filter_pass() {
        let item = PipelineItem::new(1000_i64, 10.0_f64);
        let filtered = item.filter(|&x| x > 5.0);
        assert_eq!(filtered.value, Some(10.0));
        assert_eq!(filtered.ctx, 1000);
    }

    #[test]
    fn test_pipeline_item_filter_reject() {
        let item = PipelineItem::new(1000_i64, 3.0_f64);
        let filtered = item.filter(|&x| x > 5.0);
        assert_eq!(filtered.value, None);
        assert_eq!(filtered.ctx, 1000);
    }

    #[test]
    fn test_pipeline_item_flatten() {
        let item = PipelineItem::new(1000_i64, Some(42.0_f64));
        let flattened = item.flatten();
        assert!(flattened.is_some());
        let inner = flattened.unwrap();
        assert_eq!(inner.value, 42.0);
        assert_eq!(inner.ctx, 1000);
    }

    #[test]
    fn test_pipeline_item_chain() {
        let item = PipelineItem::new(1000_i64, 10.0_f64);
        let result = item.map(|x| x + 5.0).map(|x| x * 2.0).filter(|&x| x > 20.0);
        assert_eq!(result.value, Some(30.0));
        assert_eq!(result.ctx, 1000);
    }

    #[test]
    fn test_map_with_context() {
        let item = PipelineItem::new(1000_i64, 10.0_f64);
        let result = item.map_with_context(|ctx, v| format!("{}@{}", v, *ctx));
        assert_eq!(result.value, "10@1000");
    }

    #[test]
    fn test_into_value() {
        let item = PipelineItem::new(1000_i64, 42.0_f64);
        let value = item.into_value();
        assert_eq!(value, 42.0);
    }
}
