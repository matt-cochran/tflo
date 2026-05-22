//! Semantics contracts for tflo-core.
//!
//! This module documents the behavioral contracts and guarantees provided
//! by tflo-core's computation graph execution.

/// # Semantics Contracts
///
/// This document defines the behavioral contracts and guarantees provided
/// by tflo-core's computation graph execution.
///
/// ## Warmup Behavior
///
/// **Contract**: Computation graphs require a "warmup period" before producing
/// valid outputs. During warmup, operators accumulate state but do not emit values.
///
/// **Details**:
/// - Each operator has a minimum number of records required before it can produce
///   valid output (e.g., SMA(20) requires 20 records).
/// - The graph's `min_warmup` is the maximum warmup requirement across all operators.
/// - During warmup, `step()` returns `None` and `step_with_status()` returns
///   `StepResult::WarmingUp { remaining }`.
/// - Warmup is per-operator: if you have `sma(20)` and `ema(10)`, the graph
///   will start producing outputs after 20 records (the maximum).
///
/// **Example**:
/// ```ignore
/// // SMA(5) requires 5 records before output
/// let mut graph = /* ... */;
/// for i in 0..5 {
///     assert!(graph.step(&record).is_none()); // Warmup
/// }
/// assert!(graph.step(&record).is_some()); // Ready
/// ```
///
/// ## NaN Policy
///
/// **Contract**: Missing or invalid inputs produce `NaN` values, which propagate
/// through computations according to IEEE 754 rules.
///
/// **Details**:
/// - If a property extractor returns `NaN`, that `NaN` propagates through
///   all dependent computations.
/// - Window operations (SMA, EMA, etc.) handle `NaN` by excluding it from
///   calculations (if the window has enough non-NaN values).
/// - If all values in a window are `NaN`, the result is `NaN`.
/// - Division by zero produces `NaN` (not an error, unless validation is enabled).
///
/// **Example**:
/// ```ignore
/// // If price is NaN, sma(price) will be NaN
/// let price = t.prop(|x| x.price); // Returns NaN if missing
/// let sma = price.sma(5.secs()); // Will be NaN if price is NaN
/// ```
///
/// ## Timestamp Requirements
///
/// **Contract**: Timestamps must be in milliseconds since epoch (Unix timestamp * 1000).
///
/// **Details**:
/// - Time-based windows use timestamps to determine which values fall within
///   the window duration.
/// - Timestamps should be monotonically increasing (sorted order) for correct
///   window eviction behavior.
/// - Out-of-order timestamps are handled according to `OutOfOrderPolicy` in
///   keyed execution, or can be validated via `validated()` with `assert_sorted`.
/// - If timestamps are not provided (for count-based windows), the graph uses
///   sequence numbers internally.
///
/// **Example**:
/// ```ignore
/// // Correct: milliseconds since epoch
/// t.timestamp(|x| x.ts_ms); // e.g., 1699000000000
///
/// // Also supported: seconds as i64
/// t.timestamp_secs(|x| x.ts_secs); // e.g., 1699000000
///
/// // Also supported: seconds as f64
/// t.timestamp_secs_f64(|x| x.ts_secs_f64); // e.g., 1699000000.0
/// ```
///
/// ## Out-of-Order Handling
///
/// **Contract**: Out-of-order records are handled according to the configured policy.
///
/// **Policies**:
/// - `OutOfOrderPolicy::Error`: Immediately return an error if a record arrives
///   out of order (strictest).
/// - `OutOfOrderPolicy::Drop`: Silently drop out-of-order records.
/// - `OutOfOrderPolicy::Buffer { max_lateness_ms }`: Buffer out-of-order records
///   up to the maximum lateness window, then drop if too late.
///
/// **Details**:
/// - Out-of-order detection compares the current record's timestamp to the
///   last processed timestamp for that key (in keyed execution) or globally
///   (in non-keyed execution with validation).
/// - Buffering is currently a placeholder - full implementation requires
///   a priority queue sorted by timestamp.
///
/// **Example**:
/// ```rust
/// // Keyed execution with error policy
/// stream.tflo_keyed(
///     |r| r.symbol.clone(),
///     OutOfOrderPolicy::Error, // Fail fast on out-of-order
///     |t| { /* ... */ }
/// )
/// ```
///
/// ## Window Semantics
///
/// **Contract**: Windows are time-based (duration) or count-based (N samples).
///
/// **Time-based windows**:
/// - Include all records with timestamps in `[current_ts - window_duration, current_ts]`.
/// - Eviction happens automatically as old records fall outside the window.
/// - Windows are **inclusive** of the current timestamp and **exclusive** of
///   records older than `current_ts - window_duration`.
///
/// **Count-based windows**:
/// - Include the last N records (sliding window).
/// - Eviction happens when the window exceeds N records (FIFO).
///
/// **Example**:
/// ```rust
/// // Time-based: last 5 minutes
/// price.sma(5.mins()); // Includes all records from [now - 5min, now]
///
/// // Count-based: last 20 records
/// price.sma(20usize); // Includes last 20 records
/// ```
///
/// ## Indicator Compatibility
///
/// **Contract**: Indicator implementations match their documented formulas, but
/// may differ from TradingView/TA-Lib in initialization and smoothing methods.
///
/// **Details**:
/// - Standard indicators (SMA, EMA, RSI, etc.) in `tflo-ta` use standard formulas
///   but may use different initialization methods (e.g., SMA-based RSI vs Wilder's RMA).
/// - For strict TradingView/TA-Lib compatibility, use `tflo-ta-strict` crate
///   (when available) which provides explicitly named variants (e.g., `rsi_wilder_n`).
/// - Each indicator's documentation should specify:
///   - Formula used
///   - Smoothing method (SMA, EMA, Wilder's RMA, etc.)
///   - Initialization behavior (seed values, warmup requirements)
///   - Edge case handling (zero division, all-NaN windows, etc.)
///
/// **Example**:
/// ```rust
/// // Standard RSI (may use SMA for gains/losses)
/// price.rsi(14usize);
///
/// // Strict RSI with Wilder's smoothing (when available)
/// // use tflo_ta_strict::prelude::*;
/// // price.rsi_wilder_n(14);
/// ```

