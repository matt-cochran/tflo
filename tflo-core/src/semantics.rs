//! Semantics contracts for `tflo-core`.
//!
//! This module documents the behavioral contracts and guarantees
//! provided by `tflo-core`'s computation graph execution. The
//! document is normative: where the engine code and this file
//! disagree, the file should be updated to match the code.
//!
//! # Warmup Behavior
//!
//! **Contract:** Computation graphs require a warmup period
//! before producing valid outputs. During warmup, operators
//! accumulate state but do not emit values.
//!
//! **Details:**
//!
//! - Each operator has a minimum number of records required before
//!   it can produce a valid output (e.g., SMA(20) requires 20
//!   records).
//! - The graph's `min_warmup` is the maximum warmup requirement
//!   across all operators in the topology.
//! - During warmup, `step()` returns `None` and
//!   `step_with_status()` returns `StepResult::WarmingUp { remaining }`.
//! - Warmup is per-graph: if you have `sma(20)` and `ema(10)`,
//!   the graph starts producing outputs after 20 records (the
//!   maximum).
//!
//! ```ignore
//! // SMA(5) requires 5 records before output
//! let mut graph = /* ... */;
//! for i in 0..5 {
//!     assert!(graph.step(&record).is_none()); // Warmup
//! }
//! assert!(graph.step(&record).is_some()); // Ready
//! ```
//!
//! # Typed Absence (no NaN sentinel)
//!
//! **Contract:** Internally, a node's per-record output is a
//! [`Computed`](crate::compile::Computed), which is a
//! `Result<f64, Absent>`. Missing values carry a typed reason; they
//! are never silently encoded as `NaN`.
//!
//! **Details:**
//!
//! - The internal representation is `Computed = Result<f64, Absent>`
//!   with these variants of [`Absent`](crate::compile::Absent):
//!   - `WarmingUp` — not enough data yet; will resolve.
//!   - `InvalidConfig` — parameter makes this node unable to ever
//!     produce a value (e.g. zero window). Never resolves; indicates
//!     a misconfigured graph.
//!   - `DivideByZero` — a division had a zero denominator.
//!   - `DomainError` — a math function received an out-of-domain
//!     argument (`sqrt`/`ln` of a negative, `ln(0)`, etc.).
//!   - `ZeroTimeDelta` — a rate or derivative could not be computed
//!     because two consecutive samples shared a timestamp.
//!   - `FilteredOut` — a filter predicate suppressed this value.
//!   - `UpstreamAbsent` — an input was absent; the absence
//!     propagated downstream. (The original reason is preserved when
//!     a node has a single absent input; `UpstreamAbsent` is used
//!     only when reasons would otherwise be ambiguous.)
//! - Arithmetic nodes propagate the first `Err` they see (`?` at no
//!   cost — `Absent` is `Copy` and pointer-free).
//! - Stateful nodes (windows, accumulators) skip their state update
//!   on an absent input rather than advancing with a substitute.
//! - For callers extracting `f64`, the typed reason flattens to
//!   `f64::NAN` at the `ExtractOutput` boundary. Callers can opt
//!   into the typed channel by extracting `Computed` directly.
//!
//! **Why typed absence and not NaN:** NaN-as-sentinel cannot
//! distinguish a warming window from a divide-by-zero from a
//! filtered record. Downstream code that needs to react differently
//! to each case branches on the typed `Absent` variant. Engines
//! that use null/NaN force the user to guess the reason from
//! context.
//!
//! # Timestamp Requirements
//!
//! **Contract:** Event-time timestamps are milliseconds since
//! epoch (Unix timestamp × 1000). Watermarks advance based on
//! input timestamps, never wall-clock.
//!
//! **Details:**
//!
//! - Time-based windows use timestamps to determine which values
//!   fall within the window duration.
//! - Timestamps should be monotonically increasing (sorted order)
//!   for correct window-eviction behavior. Out-of-order timestamps
//!   are handled per the configured
//!   [`OutOfOrderPolicy`](crate::keyed::OutOfOrderPolicy) (see
//!   below) or rejected via `validated()` with `assert_sorted`.
//! - Count-based windows ignore timestamps; the graph uses
//!   sequence numbers internally.
//!
//! ```ignore
//! // Correct: milliseconds since epoch
//! t.timestamp(|x| x.ts_ms); // e.g., 1699000000000
//!
//! // Also supported: seconds as i64
//! t.timestamp_secs(|x| x.ts_secs); // e.g., 1699000000
//!
//! // Also supported: seconds as f64
//! t.timestamp_secs_f64(|x| x.ts_secs_f64); // e.g., 1699000000.0
//! ```
//!
//! # Per-key Watermark Contract
//!
//! **Contract:** `tflo` advances event-time watermarks **per key**,
//! not globally across keys. Each key's processing frontier is
//! independent.
//!
//! **Details:**
//!
//! - In keyed execution, each `KeyedGraphState` tracks its own
//!   `max_ts_seen` and `last_ts`. A late-arriving event on key `A`
//!   does not delay the frontier for key `B`.
//! - Cross-key alignment (e.g., "emit the joint signal only after
//!   every sensor up to time T") is **not** provided by
//!   `tflo-core`. When that is required, host `tflo` inside an
//!   engine that provides a global watermark (Flink, Beam) — see
//!   `docs/interop-backlog.md`.
//! - The per-key design is intentional. It matches the embeddable +
//!   edge + WASM positioning, where coordinating a global
//!   watermark across processes would require additional
//!   infrastructure that `tflo-core` deliberately avoids.
//!
//! # Out-of-Order Handling
//!
//! **Contract:** Out-of-order records are handled according to the
//! configured [`OutOfOrderPolicy`](crate::keyed::OutOfOrderPolicy).
//!
//! **Policies:**
//!
//! - `OutOfOrderPolicy::Error` — return an error immediately if a
//!   record arrives out of order. Strictest; used when correctness
//!   demands explicit handling.
//! - `OutOfOrderPolicy::Drop` — silently drop out-of-order records.
//!   The drop is observable via callers that track input vs output
//!   counts; consider Phase 2c's opt-in late side-output channel
//!   when it lands.
//! - `OutOfOrderPolicy::Buffer { max_lateness_ms }` — buffer
//!   out-of-order records up to `max_lateness_ms` of lateness, then
//!   release them in timestamp order. Records older than the
//!   release watermark on arrival are dropped.
//!
//! **Buffer policy details:**
//!
//! - On every `step(record, ts, key)`:
//!   1. If `ts < last_released_ts` (i.e. older than the frontier
//!      that has already advanced past it), the record is dropped.
//!   2. Otherwise the record is inserted into a `pending` buffer
//!      kept sorted ascending by timestamp; equal timestamps
//!      preserve arrival order.
//!   3. `max_ts_seen` advances; the release watermark is
//!      `max_ts_seen - max_lateness_ms` (saturating).
//!   4. Every buffered record with `ts <= watermark` is released,
//!      in timestamp order, into the graph.
//! - At end-of-stream, `flush(key)` releases every remaining
//!   buffered record in timestamp order so records still inside
//!   the lateness window are not lost. The async stream adapter
//!   calls this automatically; iterator callers do too via the
//!   `TFloKeyedIter` flush flag.
//!
//! ```ignore
//! // Keyed execution with buffer policy: tolerate 5s of lateness
//! stream.tflo_keyed(
//!     |r| r.symbol.clone(),
//!     OutOfOrderPolicy::Buffer { max_lateness_ms: 5_000 },
//!     |t| { /* ... */ },
//! );
//! ```
//!
//! # Window Semantics
//!
//! **Contract:** Windows are time-based (duration) or count-based
//! (N samples), both sliding.
//!
//! **Time-based windows:**
//!
//! - Include all records with timestamps in
//!   `[current_ts - window_duration, current_ts]`.
//! - Eviction happens automatically as old records fall outside the
//!   window.
//! - Windows are **inclusive** of the current timestamp and
//!   **exclusive** of records older than
//!   `current_ts - window_duration`.
//!
//! **Count-based windows:**
//!
//! - Include the last N records (sliding window).
//! - Eviction is FIFO when the window exceeds N records.
//!
//! ```ignore
//! // Time-based: last 5 minutes
//! price.sma(5.mins()); // Includes records in [now - 5min, now]
//!
//! // Count-based: last 20 records
//! price.sma(20usize); // Includes last 20 records
//! ```
//!
//! Session and tumbling windows are planned for Phase 2 of the
//! closure plan and not yet implemented.
//!
//! # Indicator Compatibility
//!
//! **Contract:** Indicator implementations match their documented
//! formulas. They may differ from TradingView / TA-Lib in
//! initialization and smoothing methods; each operator's doc
//! comment specifies which.
//!
//! **Details:**
//!
//! - Standard indicators (SMA, EMA, RSI, MACD, etc.) live in
//!   [`tflo-ops`](https://docs.rs/tflo-ops) — the catalog crate.
//!   Domain plugins like `tflo-fintech` build on top of
//!   `tflo-ops` for finance-specific composites.
//! - Where multiple definitions exist (e.g., RSI with SMA-based vs
//!   Wilder's RMA smoothing), each variant is exposed under a
//!   distinct name. Wilder's RSI is
//!   `tflo_ops::ops::windows::WindowOps::rsi_wilder`; the
//!   `rsi` method uses SMA-based initialization.
//! - Each indicator's documentation specifies:
//!   - The formula used,
//!   - The smoothing method (SMA, EMA, Wilder's RMA, etc.),
//!   - Initialization behavior (seed values, warmup),
//!   - Edge case handling (zero division → `DivideByZero`,
//!     all-absent windows → `WarmingUp`, etc.).
