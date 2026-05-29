//! Pipeline context types. Moved out of `pipeline/mod.rs` by `StructureOS`
//! `move` action; the `PipelineContext` trait + the static counters
//! (`SEQUENCE_COUNTER`, `HYBRID_COUNTER`) remain in the parent module.

use super::{HYBRID_COUNTER, PipelineContext, SEQUENCE_COUNTER};
use std::sync::atomic::Ordering;

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
