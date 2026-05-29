#![cfg_attr(
    test,
    allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::indexing_slicing,
        clippy::arithmetic_side_effects,
        clippy::map_err_ignore
    )
)]
#![deny(clippy::print_stdout)]
// library code must not write to stdout
// Numeric streaming-engine intent-allows (see tflo-core for rationale).
#![allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    clippy::cast_lossless,
    clippy::float_cmp,
    clippy::suboptimal_flops
)]
//! `InfluxDB` sink for tflo — line-protocol formatter, batching wrapper,
//! and a pluggable HTTP backend.
//!
//! # What's here
//!
//! - [`LineProtocol`] — small builder producing well-formed `InfluxDB`
//!   line-protocol strings (the wire format both `InfluxDB` 1.x and 2.x
//!   speak). Pure, no dependencies — works in any process.
//! - [`Batcher`] — accumulates measurements and flushes them through a
//!   pluggable [`InfluxHttpClient`] either when a size threshold is hit
//!   or when an explicit `flush()` is called. Bounded buffer; rejects
//!   writes past the limit so an unresponsive backend cannot consume
//!   unbounded memory.
//! - [`InfluxHttpClient`] — minimal async trait an HTTP client must
//!   implement. The crate does **not** take a direct dependency on
//!   `reqwest` / `hyper` / `surf` — users plug in their preferred
//!   client. This mirrors the contracts-in-core / impls-in-separate-
//!   crates pattern used for Kafka and MQTT.

#![warn(missing_docs)]
#![warn(missing_debug_implementations)]
#![deny(unsafe_code)]

use std::collections::BTreeMap;

// ── Line-protocol builder ──────────────────────────────────────────────

/// A single measurement to be written via line-protocol.
///
/// Construct with [`new`](Self::new), add tags / fields, then call
/// [`format`](Self::format) (or pass to [`Batcher::push`]). Tags must
/// only contain identifier-friendly characters — escaping rules per
/// the `InfluxDB` line-protocol spec.
#[derive(Debug, Clone)]
pub struct LineProtocol {
    measurement: String,
    tags: BTreeMap<String, String>,
    fields: Vec<(String, FieldValue)>,
    timestamp_ns: Option<i64>,
}

/// A field value — one of `InfluxDB`'s supported primitive types.
#[derive(Debug, Clone, PartialEq)]
pub enum FieldValue {
    /// 64-bit float.
    Float(f64),
    /// 64-bit signed integer.
    Integer(i64),
    /// Unsigned 64-bit integer.
    UInteger(u64),
    /// UTF-8 string.
    String(String),
    /// Boolean.
    Boolean(bool),
}

impl LineProtocol {
    /// Begin a new measurement.
    #[must_use]
    pub fn new(measurement: impl Into<String>) -> Self {
        Self {
            measurement: measurement.into(),
            tags: BTreeMap::new(),
            fields: Vec::new(),
            timestamp_ns: None,
        }
    }

    /// Add a tag.
    #[must_use]
    pub fn tag(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        let _ = self.tags.insert(name.into(), value.into());
        self
    }

    /// Add a field.
    #[must_use]
    pub fn field(mut self, name: impl Into<String>, value: FieldValue) -> Self {
        self.fields.push((name.into(), value));
        self
    }

    /// Set the timestamp (nanoseconds since epoch).
    #[must_use]
    pub const fn timestamp_ns(mut self, ts_ns: i64) -> Self {
        self.timestamp_ns = Some(ts_ns);
        self
    }

    /// Convenience: set the timestamp in milliseconds since epoch.
    #[must_use]
    pub const fn timestamp_ms(self, ts_ms: i64) -> Self {
        self.timestamp_ns(ts_ms.saturating_mul(1_000_000))
    }

    /// Render to a line-protocol string.
    ///
    /// # Errors
    ///
    /// Returns an error string when no fields have been added —
    /// `InfluxDB` requires at least one field per measurement.
    pub fn format(&self) -> Result<String, String> {
        if self.fields.is_empty() {
            return Err("LineProtocol: at least one field is required".to_string());
        }
        let mut out = escape_identifier(&self.measurement);
        for (k, v) in &self.tags {
            out.push(',');
            out.push_str(&escape_tag_key(k));
            out.push('=');
            out.push_str(&escape_tag_value(v));
        }
        let mut first = true;
        for (k, v) in &self.fields {
            out.push(if first { ' ' } else { ',' });
            first = false;
            out.push_str(&escape_field_key(k));
            out.push('=');
            out.push_str(&format_field(v));
        }
        if let Some(ts) = self.timestamp_ns {
            out.push(' ');
            out.push_str(&ts.to_string());
        }
        Ok(out)
    }
}

// ── Line-protocol escape helpers ───────────────────────────────────────
//
// `InfluxDB` line-protocol escape rules (per the InfluxData spec — see
// <https://docs.influxdata.com/influxdb/v2/reference/syntax/line-protocol/#special-characters>):
//
//   measurement / identifier : escape `,` ` `
//   tag key, tag value,
//   field key                : escape `,` `=` ` `
//   field string value       : escape `"` and `\`  (and is wrapped in quotes)
//
// Backslash is intentionally NOT escaped for identifiers or tags: per
// the spec, a literal `\` in a tag value is preserved as-is unless it
// precedes a special character. Integration testing against real
// `InfluxDB` 2.7 (`tests/integration_influx.rs`) confirmed that
// escaping `\` to `\\` here causes the server to store the doubled
// backslash rather than unescape it.
//
// Newlines are wire-protocol unsafe (they're record separators) so they
// are rewritten to the two-character sequence `\n` to keep each record
// on a single physical line.

fn escape_identifier(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            ',' | ' ' => {
                out.push('\\');
                out.push(c);
            }
            '\n' => {
                out.push('\\');
                out.push('n');
            }
            _ => out.push(c),
        }
    }
    out
}

fn escape_tag_key(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            ',' | '=' | ' ' => {
                out.push('\\');
                out.push(c);
            }
            '\n' => {
                out.push('\\');
                out.push('n');
            }
            _ => out.push(c),
        }
    }
    out
}

fn escape_tag_value(s: &str) -> String {
    escape_tag_key(s)
}

/// Field keys follow the same rules as tag keys per the line-protocol
/// spec; kept as a distinct helper for readability and so future
/// divergence is a one-line change.
fn escape_field_key(s: &str) -> String {
    escape_tag_key(s)
}

/// Escape the contents of a quoted field-string value. Per the
/// line-protocol spec only `"` and `\` need escaping inside the quotes;
/// the backslash must be escaped first so we do not double-escape the
/// backslashes that escape the quotes themselves.
fn escape_field_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '\\' | '"' => {
                out.push('\\');
                out.push(c);
            }
            _ => out.push(c),
        }
    }
    out
}

fn format_field(v: &FieldValue) -> String {
    match v {
        FieldValue::Float(f) => format!("{f}"),
        FieldValue::Integer(i) => format!("{i}i"),
        FieldValue::UInteger(u) => format!("{u}u"),
        FieldValue::String(s) => {
            // Strings are wrapped in double quotes; backslashes and
            // internal quotes are escaped per the line-protocol spec.
            let escaped = escape_field_string(s);
            format!("\"{escaped}\"")
        }
        FieldValue::Boolean(b) => {
            if *b {
                "true".to_string()
            } else {
                "false".to_string()
            }
        }
    }
}

// ── HTTP client trait + Batcher ─────────────────────────────────────────

/// Minimal async HTTP client for writing line-protocol bodies to an
/// `InfluxDB`-compatible endpoint. Users supply a concrete impl
/// (`reqwest`, `hyper`, `surf`, etc.) — this crate keeps no opinion
/// about the HTTP library.
#[cfg(feature = "async")]
#[async_trait::async_trait]
pub trait InfluxHttpClient: Send + Sync {
    /// Write a batch of line-protocol records. Implementations should be
    /// idempotent under retry.
    ///
    /// # Errors
    ///
    /// Returns an error string when the HTTP write fails or the server
    /// returns a non-success status.
    async fn write(&self, body: &str) -> Result<(), String>;
}

#[cfg(feature = "async")]
#[async_trait::async_trait]
impl<T: InfluxHttpClient + ?Sized> InfluxHttpClient for std::sync::Arc<T> {
    async fn write(&self, body: &str) -> Result<(), String> {
        (**self).write(body).await
    }
}

/// Maximum allowed buffered byte size — bound against unbounded memory
/// growth when the backend wedges. Calls to [`Batcher::push`] beyond
/// this return an error.
pub const MAX_BUFFER_BYTES: usize = 16 * 1024 * 1024;

/// Bounded line-protocol accumulator with a pluggable flush target.
///
/// **Drop semantics**: callers MUST call [`flush`](Self::flush) and
/// `.await` it before drop for at-least-once delivery guarantees. The [`Drop`] impl
/// is best-effort only — it cannot await an HTTP write, so any
/// in-flight buffer is accounted to [`dropped_total`](Self::dropped_total)
/// and logged via `eprintln!`, then discarded.
///
/// **Max-age flushing**: by default the batcher only flushes on the
/// byte threshold or an explicit `flush()`. Slow streams that never
/// hit the threshold will stall indefinitely. Construct with
/// [`with_max_age`](Self::with_max_age) and drive
/// [`tick`](Self::tick) from a periodic task to bound the worst-case
/// flush latency.
#[cfg(feature = "async")]
pub struct Batcher<H: InfluxHttpClient> {
    client: H,
    flush_at_bytes: usize,
    max_buffer_bytes: usize,
    max_age: Option<std::time::Duration>,
    /// Buffered line-protocol body plus the instant of the first
    /// outstanding `push` since the last flush. `None` when the buffer
    /// is empty.
    buffered: tokio::sync::Mutex<(String, Option<std::time::Instant>)>,
    /// Total successful flushes. Observable. Held in an `Arc` so
    /// observers can keep a handle live past the batcher's own drop.
    pub flushes_total: std::sync::Arc<std::sync::atomic::AtomicU64>,
    /// Total dropped *bytes* — either rejected by [`push`](Self::push)
    /// for exceeding `max_buffer_bytes`, or abandoned by [`Drop`].
    /// Held in an `Arc` so observers can keep a handle live past the
    /// batcher's own drop.
    pub dropped_total: std::sync::Arc<std::sync::atomic::AtomicU64>,
}

#[cfg(feature = "async")]
impl<H: InfluxHttpClient> std::fmt::Debug for Batcher<H> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Batcher")
            .field("flush_at_bytes", &self.flush_at_bytes)
            .field("max_buffer_bytes", &self.max_buffer_bytes)
            .field("max_age", &self.max_age)
            .field(
                "flushes_total",
                &self
                    .flushes_total
                    .load(std::sync::atomic::Ordering::Relaxed),
            )
            .field(
                "dropped_total",
                &self
                    .dropped_total
                    .load(std::sync::atomic::Ordering::Relaxed),
            )
            .finish()
    }
}

#[cfg(feature = "async")]
impl<H: InfluxHttpClient> Batcher<H> {
    /// Construct with size-only flush triggers (no max-age).
    ///
    /// `flush_at_bytes` is the soft trigger — when the buffer crosses
    /// this size, the next `push` triggers a flush. `max_buffer_bytes`
    /// is the hard limit; pushes that would exceed it return an error
    /// and add the rejected byte count to `dropped_total`. Both bounds
    /// are silently clamped to [`MAX_BUFFER_BYTES`] for safety.
    ///
    /// Equivalent to `with_max_age(client, flush_at_bytes,
    /// max_buffer_bytes, None)`; provided for backwards compatibility.
    #[must_use]
    pub fn new(client: H, flush_at_bytes: usize, max_buffer_bytes: usize) -> Self {
        Self::with_max_age(client, flush_at_bytes, max_buffer_bytes, None)
    }

    /// Construct with an optional maximum buffer age. When `max_age` is
    /// `Some(d)`, a `push` whose buffer has been outstanding for at
    /// least `d` triggers a flush even if the byte threshold has not
    /// been hit. To bound flush latency on streams that never push
    /// after the deadline, drive [`tick`](Self::tick) from a periodic
    /// task — see that method's doc for the typical pattern.
    #[must_use]
    pub fn with_max_age(
        client: H,
        flush_at_bytes: usize,
        max_buffer_bytes: usize,
        max_age: Option<std::time::Duration>,
    ) -> Self {
        let flush = flush_at_bytes.min(MAX_BUFFER_BYTES);
        let max = max_buffer_bytes.min(MAX_BUFFER_BYTES);
        Self {
            client,
            flush_at_bytes: flush,
            max_buffer_bytes: max,
            max_age,
            buffered: tokio::sync::Mutex::new((String::new(), None)),
            flushes_total: std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0)),
            dropped_total: std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0)),
        }
    }

    /// Push one formatted line-protocol line. May trigger an HTTP flush
    /// if the buffer crosses `flush_at_bytes` or — when configured via
    /// [`with_max_age`](Self::with_max_age) — has been outstanding for
    /// longer than `max_age`. Always appends a trailing newline.
    ///
    /// # Errors
    ///
    /// Returns an error string when the push would exceed
    /// `max_buffer_bytes`, or when an auto-triggered flush fails.
    pub async fn push(&self, line: &str) -> Result<(), String> {
        let mut buf = self.buffered.lock().await;
        // SAFETY: `line.len()` is bounded by `MAX_BUFFER_BYTES` (16 MB) at
        // any realistic call site; `+ 1` for the newline cannot overflow
        // `usize` even on 32-bit. Same reasoning for `buf.0.len() + added`
        // — buffer size is capped at `max_buffer_bytes` (also <= 16 MB).
        #[allow(clippy::arithmetic_side_effects)]
        let added = line.len() + 1;
        #[allow(clippy::arithmetic_side_effects)]
        let would_be = buf.0.len() + added;
        if would_be > self.max_buffer_bytes {
            self.dropped_total
                .fetch_add(added as u64, std::sync::atomic::Ordering::Relaxed);
            return Err(format!(
                "Batcher: push would exceed max_buffer_bytes ({} bytes)",
                self.max_buffer_bytes
            ));
        }
        // Stamp the first-push time when transitioning from empty.
        if buf.0.is_empty() {
            buf.1 = Some(std::time::Instant::now());
        }
        buf.0.push_str(line);
        buf.0.push('\n');

        let size_trip = buf.0.len() >= self.flush_at_bytes;
        let age_trip = match (self.max_age, buf.1) {
            (Some(max), Some(stamp)) => stamp.elapsed() >= max,
            _ => false,
        };
        if size_trip || age_trip {
            let body = std::mem::take(&mut buf.0);
            buf.1 = None;
            drop(buf);
            self.client.write(&body).await?;
            self.flushes_total
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        }
        Ok(())
    }

    /// Flush any buffered data unconditionally.
    ///
    /// # Errors
    ///
    /// Returns an error string when the HTTP write fails.
    pub async fn flush(&self) -> Result<(), String> {
        let mut buf = self.buffered.lock().await;
        if buf.0.is_empty() {
            return Ok(());
        }
        let body = std::mem::take(&mut buf.0);
        buf.1 = None;
        drop(buf);
        self.client.write(&body).await?;
        self.flushes_total
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        Ok(())
    }

    /// Periodic tick — flushes the buffer when `max_age` has elapsed
    /// since the first outstanding `push`. No-op when `max_age` is
    /// `None` or the buffer is empty. Cheap to call frequently.
    ///
    /// Drive from a periodic task so slow streams do not stall:
    ///
    /// ```rust,ignore
    /// let batcher = std::sync::Arc::new(Batcher::with_max_age(
    ///     client, 64 * 1024, 1024 * 1024, Some(std::time::Duration::from_secs(1)),
    /// ));
    /// let b = batcher.clone();
    /// tokio::spawn(async move {
    ///     let mut iv = tokio::time::interval(std::time::Duration::from_millis(500));
    ///     loop {
    ///         iv.tick().await;
    ///         let _ = b.tick().await;
    ///     }
    /// });
    /// ```
    ///
    /// # Errors
    ///
    /// Returns an error string when the triggered HTTP write fails.
    pub async fn tick(&self) -> Result<(), String> {
        let Some(max) = self.max_age else {
            return Ok(());
        };
        let mut buf = self.buffered.lock().await;
        if buf.0.is_empty() {
            return Ok(());
        }
        let elapsed = buf.1.is_some_and(|s| s.elapsed() >= max);
        if !elapsed {
            return Ok(());
        }
        let body = std::mem::take(&mut buf.0);
        buf.1 = None;
        drop(buf);
        self.client.write(&body).await?;
        self.flushes_total
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        Ok(())
    }
}

/// Best-effort accounting on drop. Cannot await — so any in-flight
/// buffer is **lost**. Callers that need at-least-once delivery MUST
/// call [`Batcher::flush`].`await` before letting the batcher go out
/// of scope; this impl only ensures the loss is visible via
/// [`Batcher::dropped_total`] and a `stderr` log line.
#[cfg(feature = "async")]
impl<H: InfluxHttpClient> Drop for Batcher<H> {
    fn drop(&mut self) {
        // `try_lock` rather than `blocking_lock` — we must never block
        // the async runtime from `Drop`.
        if let Ok(buf) = self.buffered.try_lock() {
            let n = buf.0.len();
            if n > 0 {
                self.dropped_total
                    .fetch_add(n as u64, std::sync::atomic::Ordering::Relaxed);
                // SAFETY (print_stderr): operator-visible diagnostic for the
                // INFLUX-002 Drop-with-unflushed-data path. Tracing was
                // deliberately not pulled in as a dep; stderr is the
                // documented operator channel for this loss.
                #[allow(clippy::print_stderr)]
                {
                    eprintln!("[tflo-sink-influx] Batcher dropped with {n} unflushed bytes");
                }
            }
        } else {
            // Lock contention at drop-time means another task still
            // holds the buffer — exceptionally rare. Surface it; we
            // cannot account the loss precisely.
            #[allow(clippy::print_stderr)] // SAFETY: see comment above
            {
                eprintln!("[tflo-sink-influx] Batcher dropped while buffer mutex was held");
            }
        }
    }
}

// ── Tests ──────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_basic_measurement() {
        let line = LineProtocol::new("cpu")
            .tag("host", "web01")
            .field("usage", FieldValue::Float(0.84))
            .timestamp_ns(1_700_000_000_000_000_000)
            .format()
            .expect("ok");
        assert_eq!(line, "cpu,host=web01 usage=0.84 1700000000000000000");
    }

    #[test]
    fn format_escapes_spaces_in_measurement() {
        let line = LineProtocol::new("disk space")
            .field("free", FieldValue::Integer(42))
            .format()
            .expect("ok");
        assert_eq!(line, "disk\\ space free=42i");
    }

    #[test]
    fn format_requires_at_least_one_field() {
        let err = LineProtocol::new("m").format().unwrap_err();
        assert!(err.contains("at least one field"));
    }

    #[test]
    fn format_handles_all_field_types() {
        let line = LineProtocol::new("m")
            .field("f", FieldValue::Float(1.5))
            .field("i", FieldValue::Integer(-3))
            .field("u", FieldValue::UInteger(7))
            .field("s", FieldValue::String("hi".into()))
            .field("b", FieldValue::Boolean(true))
            .format()
            .expect("ok");
        assert!(line.contains("f=1.5"));
        assert!(line.contains("i=-3i"));
        assert!(line.contains("u=7u"));
        assert!(line.contains("s=\"hi\""));
        assert!(line.contains("b=true"));
    }

    #[test]
    fn format_quotes_strings_and_escapes_inner_quotes() {
        let line = LineProtocol::new("m")
            .field("msg", FieldValue::String(r#"hello "world""#.into()))
            .format()
            .expect("ok");
        assert!(line.contains(r#"msg="hello \"world\"""#));
    }

    #[cfg(feature = "async")]
    mod batcher {
        use super::*;
        use std::sync::Mutex;

        #[derive(Default)]
        struct CapturingClient {
            writes: Mutex<Vec<String>>,
            fail_next: std::sync::atomic::AtomicBool,
        }

        #[async_trait::async_trait]
        impl InfluxHttpClient for CapturingClient {
            async fn write(&self, body: &str) -> Result<(), String> {
                if self
                    .fail_next
                    .swap(false, std::sync::atomic::Ordering::Relaxed)
                {
                    return Err("forced".into());
                }
                self.writes
                    .lock()
                    .map_err(|_| "poison".to_string())?
                    .push(body.to_string());
                Ok(())
            }
        }

        #[tokio::test]
        async fn flush_threshold_triggers_write() {
            // Each pushed line is 7 bytes including the appended '\n';
            // threshold 14 trips on the second push.
            let b = Batcher::new(CapturingClient::default(), 14, 1024);
            b.push("a v=1i").await.expect("ok");
            assert_eq!(
                b.flushes_total.load(std::sync::atomic::Ordering::Relaxed),
                0,
                "first push should not flush"
            );
            b.push("b v=2i").await.expect("ok");
            assert_eq!(
                b.flushes_total.load(std::sync::atomic::Ordering::Relaxed),
                1,
                "second push crossed the threshold and should have flushed"
            );
        }

        #[tokio::test]
        async fn explicit_flush_writes_remaining() {
            let b = Batcher::new(CapturingClient::default(), 1024, 1024);
            b.push("m v=1i").await.expect("ok");
            b.flush().await.expect("ok");
            assert_eq!(
                b.flushes_total.load(std::sync::atomic::Ordering::Relaxed),
                1
            );
        }

        #[tokio::test]
        async fn push_rejects_when_over_max_buffer() {
            let b = Batcher::new(CapturingClient::default(), 1024, 32);
            // Fill the buffer.
            let big = "x".repeat(40);
            let err = b.push(&big).await.unwrap_err();
            assert!(err.contains("max_buffer_bytes"));
            // Counter measures bytes (line + newline = 41), not calls.
            assert_eq!(
                b.dropped_total.load(std::sync::atomic::Ordering::Relaxed),
                41
            );
        }

        #[tokio::test]
        async fn batcher_drop_records_loss() {
            let b = Batcher::new(CapturingClient::default(), 1024, 1024);
            // Clone the Arc so we keep an observer alive past drop.
            let dropped = std::sync::Arc::clone(&b.dropped_total);
            let pushed = "abc v=1i";
            b.push(pushed).await.expect("ok");
            let expected_bytes = (pushed.len() + 1) as u64; // +1 for '\n'
            drop(b);
            assert!(
                dropped.load(std::sync::atomic::Ordering::Relaxed) >= expected_bytes,
                "Drop should account at least the pushed body to dropped_total"
            );
        }

        #[tokio::test]
        async fn batcher_max_age_triggers_flush() {
            // Threshold is huge; only the max_age path should fire.
            let b = Batcher::with_max_age(
                CapturingClient::default(),
                1024 * 1024,
                1024 * 1024,
                Some(std::time::Duration::from_millis(50)),
            );
            b.push("m v=1i").await.expect("ok");
            assert_eq!(
                b.flushes_total.load(std::sync::atomic::Ordering::Relaxed),
                0,
                "first push should not flush yet"
            );
            tokio::time::sleep(std::time::Duration::from_millis(80)).await;
            // Second push observes elapsed > max_age and flushes.
            b.push("m v=2i").await.expect("ok");
            assert_eq!(
                b.flushes_total.load(std::sync::atomic::Ordering::Relaxed),
                1,
                "second push past max_age should have flushed"
            );
        }

        #[tokio::test]
        async fn batcher_tick_flushes_when_age_elapsed() {
            let b = Batcher::with_max_age(
                CapturingClient::default(),
                1024 * 1024,
                1024 * 1024,
                Some(std::time::Duration::from_millis(50)),
            );
            b.push("m v=1i").await.expect("ok");
            // Not yet elapsed.
            b.tick().await.expect("ok");
            assert_eq!(
                b.flushes_total.load(std::sync::atomic::Ordering::Relaxed),
                0,
                "tick before max_age should be a no-op"
            );
            tokio::time::sleep(std::time::Duration::from_millis(80)).await;
            b.tick().await.expect("ok");
            assert_eq!(
                b.flushes_total.load(std::sync::atomic::Ordering::Relaxed),
                1,
                "tick after max_age should flush"
            );
        }

        #[tokio::test]
        async fn batcher_tick_noop_when_no_max_age() {
            let b = Batcher::new(CapturingClient::default(), 1024 * 1024, 1024 * 1024);
            b.push("m v=1i").await.expect("ok");
            // No max_age configured — tick must never flush.
            for _ in 0..5 {
                b.tick().await.expect("ok");
            }
            assert_eq!(
                b.flushes_total.load(std::sync::atomic::Ordering::Relaxed),
                0,
                "tick must be a no-op when max_age is None"
            );
        }
    }

    #[test]
    fn escape_handles_newline_and_passes_backslash_through() {
        // measurement / identifier: `\` is NOT special per the
        // line-protocol spec — passes through unchanged. `\n` is rewritten
        // to `\n` (literal two chars) for wire-format safety since
        // newlines are record separators.
        assert_eq!(escape_identifier("a\\b"), "a\\b");
        assert_eq!(escape_identifier("a\nb"), "a\\nb");
        assert_eq!(escape_identifier("a b,c\\d\ne"), "a\\ b\\,c\\d\\ne");
        // tag key (and tag value / field key all share the rule): `,` `=` ` `
        assert_eq!(escape_tag_key("k\\v"), "k\\v");
        assert_eq!(escape_tag_key("k\nv"), "k\\nv");
        assert_eq!(escape_tag_key("k=v\\,x\n"), "k\\=v\\\\,x\\n");
        assert_eq!(escape_tag_value("v\\x\ny"), "v\\x\\ny");
        assert_eq!(escape_field_key("f\\k\n"), "f\\k\\n");
        // field string value: `"` and `\` ARE special per spec.
        assert_eq!(escape_field_string("a\\b"), "a\\\\b");
        assert_eq!(escape_field_string("he said \"hi\""), "he said \\\"hi\\\"");
        // backslash escaped first so we do not double-escape.
        assert_eq!(escape_field_string("\\\""), "\\\\\\\"");
    }

    #[test]
    fn format_field_string_escapes_backslash() {
        let line = LineProtocol::new("m")
            .field("path", FieldValue::String(r"C:\tmp\x".into()))
            .format()
            .expect("ok");
        // Each backslash escaped to `\\`; quotes wrap the whole thing.
        assert!(
            line.contains(r#"path="C:\\tmp\\x""#),
            "expected backslashes to be escaped, got: {line}"
        );
    }

    #[test]
    fn format_measurement_passes_backslash_through() {
        // `\` is not a special character in measurement names per the
        // line-protocol spec, so it should pass through to the wire
        // unchanged. (Integration testing against a real InfluxDB 2.7
        // confirmed escaping `\` here causes the server to store the
        // doubled backslash rather than unescape it.)
        let line = LineProtocol::new(r"weird\name")
            .field("v", FieldValue::Integer(1))
            .format()
            .expect("ok");
        assert!(
            line.starts_with(r"weird\name "),
            "expected backslash in measurement to pass through, got: {line}"
        );
    }
}
