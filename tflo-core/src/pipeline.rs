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

use std::sync::atomic::{AtomicU64, Ordering};

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

/// Sequence-based pipeline context.
///
/// Carries a monotonically increasing sequence number as an `i64` ordering key.
/// Useful for count-based operations where time is not relevant.
///
/// # Use Cases
///
/// - Count-based windowing (SMA over N items)
/// - Index-based lookback
/// - Testing and debugging
///
/// # Thread Safety
///
/// The sequence counter is global and atomic, ensuring unique sequence numbers
/// across all graphs and threads.
///
/// # Example
///
/// ```rust
/// use tflo_core::pipeline::{Sequenced, PipelineContext};
///
/// let ctx1 = Sequenced::from_ordering_key(0);
/// let ctx2 = Sequenced::from_ordering_key(0);
/// // ctx1.0 and ctx2.0 will be different (monotonically increasing)
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Sequenced(pub i64);

/// Global sequence counter for `Sequenced` context.
static SEQUENCE_COUNTER: AtomicU64 = AtomicU64::new(0);

impl PipelineContext for Sequenced {
    #[inline]
    fn from_ordering_key(_key: i64) -> Self {
        Self(SEQUENCE_COUNTER.fetch_add(1, Ordering::Relaxed) as i64)
    }

    #[inline]
    fn ordering_key(&self) -> i64 {
        self.0
    }
}

impl Sequenced {
    /// Create a new sequenced context with explicit value.
    #[must_use]
    pub const fn new(seq: i64) -> Self {
        Self(seq)
    }

    /// Get the sequence number (ordering key).
    #[must_use]
    pub const fn sequence(&self) -> i64 {
        self.0
    }

    /// Reset the global sequence counter (for testing).
    #[cfg(test)]
    pub fn reset_counter() {
        SEQUENCE_COUNTER.store(0, Ordering::Relaxed);
    }
}

/// Hybrid pipeline context with both timestamp and sequence number.
///
/// Useful when you need both temporal ordering and item counting,
/// or when debugging pipelines. The ordering key is the timestamp.
///
/// # Ordering
///
/// The `ordering_key()` returns the timestamp, making this suitable
/// for time-based windowing while still tracking sequence numbers.
///
/// # Example
///
/// ```rust
/// use tflo_core::pipeline::{Hybrid, PipelineContext};
///
/// let ctx = Hybrid::from_ordering_key(1699000000000);
/// assert_eq!(ctx.timestamp(), 1699000000000);
/// // ctx.sequence() will be a unique sequence number
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub struct Hybrid {
    /// Timestamp in milliseconds since epoch (used as ordering key).
    pub ts: i64,
    /// Monotonically increasing sequence number.
    pub seq: u64,
}

/// Global sequence counter for `Hybrid` context.
static HYBRID_COUNTER: AtomicU64 = AtomicU64::new(0);

impl PipelineContext for Hybrid {
    #[inline]
    fn from_ordering_key(key: i64) -> Self {
        Self {
            ts: key,
            seq: HYBRID_COUNTER.fetch_add(1, Ordering::Relaxed),
        }
    }

    #[inline]
    fn ordering_key(&self) -> i64 {
        self.ts
    }
}

impl Hybrid {
    /// Create a new hybrid context.
    #[must_use]
    pub const fn new(ts: i64, seq: u64) -> Self {
        Self { ts, seq }
    }

    /// Get the timestamp (ordering key).
    #[must_use]
    pub const fn timestamp(&self) -> i64 {
        self.ts
    }

    /// Get the sequence number.
    #[must_use]
    pub const fn sequence(&self) -> u64 {
        self.seq
    }

    /// Reset the global sequence counter (for testing).
    #[cfg(test)]
    pub fn reset_counter() {
        HYBRID_COUNTER.store(0, Ordering::Relaxed);
    }
}

/// Keyed timestamped pipeline context.
///
/// Carries both a timestamp (for ordering/windowing) and a key (for attribution).
/// This is the context type used by keyed execution (`tflo_keyed`), ensuring that
/// outputs remain attributable to their key even after transformations, without
/// requiring cloning of the full record.
///
/// The ordering key is the timestamp, making this suitable for time-based windowing
/// while preserving key attribution throughout the pipeline.
///
/// # Use Cases
///
/// - Keyed time-series analysis (per-symbol, per-device, per-frequency-bin)
/// - Multi-tenant signal processing
/// - Partitioned stateful computations
///
/// # Example
///
/// ```rust
/// use tflo_core::pipeline::{KeyedTimestamped, PipelineContext};
///
/// let ctx = KeyedTimestamped::new(1699000000000, "AAPL");
/// assert_eq!(ctx.timestamp(), 1699000000000);
/// assert_eq!(ctx.key(), &"AAPL");
/// assert_eq!(ctx.ordering_key(), 1699000000000);
/// ```
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct KeyedTimestamped<K> {
    /// Timestamp in milliseconds since epoch (used as ordering key).
    pub ts: i64,
    /// The key value (symbol, `device_id`, etc.).
    pub key: K,
}

impl<K: Clone + Send + Sync + 'static + Default + std::hash::Hash + Eq> PipelineContext
    for KeyedTimestamped<K>
{
    #[inline]
    fn from_ordering_key(key: i64) -> Self {
        // This is a fallback - normally you'd construct with both ts and key
        // This exists to satisfy the trait, but keyed contexts should be created explicitly
        // via KeyedTimestamped::new() or by the keyed execution adaptors
        Self {
            ts: key,
            key: K::default(),
        }
    }

    #[inline]
    fn ordering_key(&self) -> i64 {
        self.ts
    }
}

impl<K> KeyedTimestamped<K> {
    /// Create a new keyed timestamped context.
    #[must_use]
    pub const fn new(ts: i64, key: K) -> Self {
        Self { ts, key }
    }

    /// Get the timestamp (ordering key).
    #[must_use]
    pub const fn timestamp(&self) -> i64 {
        self.ts
    }

    /// Get a reference to the key.
    #[must_use]
    pub const fn key(&self) -> &K {
        &self.key
    }

    /// Extract the key, consuming the context.
    #[must_use]
    pub fn into_key(self) -> K {
        self.key
    }
}

// ============================================================================
// PIPELINE ITEM
// ============================================================================

/// A pipeline item carrying context and value.
///
/// `PipelineItem<C, T>` is the fundamental unit of data flowing through
/// a pipeline. It pairs a computed value `T` with context `C` that carries
/// metadata like timestamps or sequence numbers.
///
/// # Monadic Operations
///
/// `PipelineItem` supports monadic operations that preserve context:
///
/// - `map`: Transform the value, preserve context
/// - `filter`: Conditionally keep value, preserve context
/// - `and_then`: Chain computations (flatMap)
///
/// # Example
///
/// ```rust
/// use tflo_core::pipeline::{PipelineItem, Timestamped};
///
/// let item = PipelineItem {
///     ctx: 1000_i64,
///     value: 42.0_f64,
/// };
///
/// // Map preserves timestamp
/// let doubled = item.map(|v| v * 2.0);
/// assert_eq!(doubled.ctx, 1000_i64);
/// assert_eq!(doubled.value, 84.0);
/// ```
#[derive(Clone, Debug, PartialEq)]
pub struct PipelineItem<C, T> {
    /// The pipeline context (timestamp, sequence, etc.)
    pub ctx: C,
    /// The computed value
    pub value: T,
}

impl<C, T> PipelineItem<C, T> {
    /// Create a new pipeline item.
    #[must_use]
    pub const fn new(ctx: C, value: T) -> Self {
        Self { ctx, value }
    }

    /// Extract just the value, discarding context.
    #[must_use]
    pub fn into_value(self) -> T {
        self.value
    }

    /// Get a reference to the value.
    #[must_use]
    pub const fn value(&self) -> &T {
        &self.value
    }

    /// Get a reference to the context.
    #[must_use]
    pub const fn context(&self) -> &C {
        &self.ctx
    }
}

impl<C: Clone, T> PipelineItem<C, T> {
    /// Transform the value, preserving context.
    ///
    /// This is the functor `map` operation - it applies a function to the
    /// value while keeping the context unchanged.
    ///
    /// # Example
    ///
    /// ```rust
    /// use tflo_core::pipeline::{PipelineItem, Timestamped};
    ///
    /// let item = PipelineItem::new(1000_i64, 10.0);  // Timestamped is i64
    /// let mapped = item.map(|x| x * 2.0);
    /// assert_eq!(mapped.value, 20.0);
    /// assert_eq!(mapped.ctx, 1000); // Context preserved
    /// ```
    #[must_use]
    pub fn map<U, F>(self, f: F) -> PipelineItem<C, U>
    where
        F: FnOnce(T) -> U,
    {
        PipelineItem {
            ctx: self.ctx,
            value: f(self.value),
        }
    }

    /// Filter the value based on a predicate, preserving context.
    ///
    /// Returns `Some(value)` if predicate is true, `None` otherwise.
    /// Context is always preserved.
    ///
    /// # Example
    ///
    /// ```rust
    /// use tflo_core::pipeline::{PipelineItem, Timestamped};
    ///
    /// let item = PipelineItem::new(1000_i64, 10.0);  // Timestamped is i64
    /// let filtered = item.filter(|&x| x > 5.0);
    /// assert_eq!(filtered.value, Some(10.0));
    /// ```
    #[must_use]
    pub fn filter<F>(self, predicate: F) -> PipelineItem<C, Option<T>>
    where
        F: FnOnce(&T) -> bool,
    {
        PipelineItem {
            ctx: self.ctx,
            value: if predicate(&self.value) {
                Some(self.value)
            } else {
                None
            },
        }
    }

    /// Chain computations that produce new pipeline items.
    ///
    /// This is the monadic `bind` (flatMap) operation. The function
    /// receives the value and must produce a new `PipelineItem`.
    ///
    /// # Note
    ///
    /// The new item's context comes from the function result, not
    /// from the original item. Use `map` if you want to preserve context.
    #[must_use]
    pub fn and_then<U, C2, F>(self, f: F) -> PipelineItem<C2, U>
    where
        F: FnOnce(T) -> PipelineItem<C2, U>,
    {
        f(self.value)
    }

    /// Apply a function that takes both context and value.
    #[must_use]
    pub fn map_with_context<U, F>(self, f: F) -> PipelineItem<C, U>
    where
        F: FnOnce(&C, T) -> U,
    {
        let value = f(&self.ctx, self.value);
        PipelineItem {
            ctx: self.ctx,
            value,
        }
    }
}

impl<C: Clone, T: Clone> PipelineItem<C, Option<T>> {
    /// Flatten an optional value, returning `None` if the value is `None`.
    ///
    /// Useful after `filter` to unwrap the option.
    #[must_use]
    pub fn flatten(self) -> Option<PipelineItem<C, T>> {
        self.value.map(|v| PipelineItem {
            ctx: self.ctx,
            value: v,
        })
    }
}

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

impl<C: PipelineContext> PipelineContext for PipelineItem<C, ()> {
    fn from_ordering_key(key: i64) -> Self {
        Self {
            ctx: C::from_ordering_key(key),
            value: (),
        }
    }

    fn ordering_key(&self) -> i64 {
        self.ctx.ordering_key()
    }
}

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
