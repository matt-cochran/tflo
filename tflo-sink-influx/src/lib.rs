#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]
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
            out.push_str(&escape_tag_key(k));
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

fn escape_identifier(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            ',' | ' ' => {
                out.push('\\');
                out.push(c);
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
            _ => out.push(c),
        }
    }
    out
}

fn escape_tag_value(s: &str) -> String {
    escape_tag_key(s)
}

fn format_field(v: &FieldValue) -> String {
    match v {
        FieldValue::Float(f) => format!("{f}"),
        FieldValue::Integer(i) => format!("{i}i"),
        FieldValue::UInteger(u) => format!("{u}u"),
        FieldValue::String(s) => {
            // Strings are wrapped in double quotes; internal quotes are
            // escaped with a backslash per the line-protocol spec.
            let escaped = s.replace('"', "\\\"");
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
#[cfg(feature = "async")]
pub struct Batcher<H: InfluxHttpClient> {
    client: H,
    flush_at_bytes: usize,
    max_buffer_bytes: usize,
    buffered: tokio::sync::Mutex<String>,
    /// Total successful flushes. Observable.
    pub flushes_total: std::sync::atomic::AtomicU64,
    /// Total dropped writes (push exceeded `max_buffer_bytes`).
    pub dropped_total: std::sync::atomic::AtomicU64,
}

#[cfg(feature = "async")]
impl<H: InfluxHttpClient> std::fmt::Debug for Batcher<H> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Batcher")
            .field("flush_at_bytes", &self.flush_at_bytes)
            .field("max_buffer_bytes", &self.max_buffer_bytes)
            .field(
                "flushes_total",
                &self.flushes_total.load(std::sync::atomic::Ordering::Relaxed),
            )
            .field(
                "dropped_total",
                &self.dropped_total.load(std::sync::atomic::Ordering::Relaxed),
            )
            .finish()
    }
}

#[cfg(feature = "async")]
impl<H: InfluxHttpClient> Batcher<H> {
    /// Construct.
    ///
    /// `flush_at_bytes` is the soft trigger — when the buffer crosses
    /// this size, the next `push` triggers a flush. `max_buffer_bytes`
    /// is the hard limit; pushes that would exceed it return an error
    /// and increment the `dropped_total` counter. Both bounds are
    /// silently clamped to [`MAX_BUFFER_BYTES`] for safety.
    #[must_use]
    pub fn new(client: H, flush_at_bytes: usize, max_buffer_bytes: usize) -> Self {
        let flush = flush_at_bytes.min(MAX_BUFFER_BYTES);
        let max = max_buffer_bytes.min(MAX_BUFFER_BYTES);
        Self {
            client,
            flush_at_bytes: flush,
            max_buffer_bytes: max,
            buffered: tokio::sync::Mutex::new(String::new()),
            flushes_total: std::sync::atomic::AtomicU64::new(0),
            dropped_total: std::sync::atomic::AtomicU64::new(0),
        }
    }

    /// Push one formatted line-protocol line. May trigger an HTTP flush
    /// if the buffer crosses `flush_at_bytes`. Always appends a trailing
    /// newline.
    ///
    /// # Errors
    ///
    /// Returns an error string when the push would exceed
    /// `max_buffer_bytes`, or when an auto-triggered flush fails.
    pub async fn push(&self, line: &str) -> Result<(), String> {
        let mut buf = self.buffered.lock().await;
        if buf.len() + line.len() + 1 > self.max_buffer_bytes {
            self.dropped_total
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            return Err(format!(
                "Batcher: push would exceed max_buffer_bytes ({} bytes)",
                self.max_buffer_bytes
            ));
        }
        buf.push_str(line);
        buf.push('\n');
        if buf.len() >= self.flush_at_bytes {
            let body = std::mem::take(&mut *buf);
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
        if buf.is_empty() {
            return Ok(());
        }
        let body = std::mem::take(&mut *buf);
        drop(buf);
        self.client.write(&body).await?;
        self.flushes_total
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        Ok(())
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
        assert_eq!(
            line,
            "cpu,host=web01 usage=0.84 1700000000000000000"
        );
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
            assert_eq!(
                b.dropped_total.load(std::sync::atomic::Ordering::Relaxed),
                1
            );
        }
    }
}
