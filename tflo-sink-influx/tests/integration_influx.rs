//! Adapter-level integration tests for `tflo-sink-influx`.
//!
//! These tests stand up a real `InfluxDB` 2.x container via
//! `testcontainers`, drive the [`InfluxHttpClient`](tflo_sink_influx::InfluxHttpClient)
//! trait with a tiny `reqwest` impl, push lines through [`Batcher`](tflo_sink_influx::Batcher),
//! and then query the server back over HTTP to confirm the wire bytes
//! were *parsed* (not just re-parsed by our own code) as the line-protocol
//! spec promises.
//!
//! Why this exists as a separate file (and a separate feature):
//!
//! - The unit tests in `src/lib.rs` exercise the escape function — but
//!   only against *itself*. Audit finding INFLUX-001 was specifically
//!   about whether the wire bytes survive a real `InfluxDB` parser; the
//!   only way to prove that is to write the bytes and query them back.
//! - The container takes ~3 seconds to come up; gating with
//!   `--features integration-tests` keeps the default `cargo test` fast
//!   and CI-friendly on hosts without Docker.

#![cfg(all(feature = "integration-tests", feature = "async"))]
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::arithmetic_side_effects // test code: deadline math, counters, fixture sizing
)]

use std::sync::Arc;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use testcontainers::{
    GenericImage, ImageExt,
    core::{ContainerPort, WaitFor},
    runners::AsyncRunner,
};
use tflo_sink_influx::{Batcher, FieldValue, InfluxHttpClient, LineProtocol};

// ── Test constants ─────────────────────────────────────────────────────

const INFLUX_ORG: &str = "testorg";
const INFLUX_BUCKET: &str = "testbucket";
const INFLUX_TOKEN: &str = "testtoken";

/// Hard upper bound on any one test (container boot + write + read-back).
const TEST_TIMEOUT: Duration = Duration::from_secs(20);

// ── Container bring-up ─────────────────────────────────────────────────

/// Boot an `InfluxDB` 2.7 container with admin token + org + bucket
/// pre-provisioned, then wait until `/health` reports 200.
///
/// Returns `(container, base_url)`. The container handle must be kept
/// alive for the duration of the test; dropping it terminates the
/// container.
async fn start_influx() -> (testcontainers::ContainerAsync<GenericImage>, String) {
    // `influxdb:2.7` with `DOCKER_INFLUXDB_INIT_MODE=setup` performs a
    // one-shot setup on first boot. Any subsequent boot of the same
    // volume is a no-op; we don't mount a volume so every test gets a
    // fresh server.
    let image = GenericImage::new("influxdb", "2.7")
        .with_exposed_port(ContainerPort::Tcp(8086))
        .with_wait_for(WaitFor::message_on_stdout("Listening"))
        .with_env_var("DOCKER_INFLUXDB_INIT_MODE", "setup")
        .with_env_var("DOCKER_INFLUXDB_INIT_USERNAME", "admin")
        .with_env_var("DOCKER_INFLUXDB_INIT_PASSWORD", "adminpass")
        .with_env_var("DOCKER_INFLUXDB_INIT_ORG", INFLUX_ORG)
        .with_env_var("DOCKER_INFLUXDB_INIT_BUCKET", INFLUX_BUCKET)
        .with_env_var("DOCKER_INFLUXDB_INIT_ADMIN_TOKEN", INFLUX_TOKEN);

    let container = image
        .start()
        .await
        .expect("failed to start influxdb container");

    let host = container
        .get_host()
        .await
        .expect("container host")
        .to_string();
    let port = container
        .get_host_port_ipv4(8086)
        .await
        .expect("container port");
    let base_url = format!("http://{host}:{port}");

    // The `WaitFor::message_on_stdout` is necessary but not sufficient —
    // the setup-mode init runs *after* the listener is up, and writes
    // during init return 401. Poll `/health` until it reports a 200.
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(2))
        .build()
        .expect("reqwest client");
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        assert!(
            Instant::now() < deadline,
            "influxdb /health never reported ready"
        );
        match client.get(format!("{base_url}/health")).send().await {
            Ok(resp) if resp.status().is_success() => break,
            _ => tokio::time::sleep(Duration::from_millis(200)).await,
        }
    }

    (container, base_url)
}

// ── Reqwest-backed `InfluxHttpClient` ─────────────────────────────────

/// Minimal `reqwest`-backed impl of the crate's [`InfluxHttpClient`]
/// trait. Tests use this to drive [`Batcher`] against the real container.
struct ReqwestInfluxClient {
    client: reqwest::Client,
    write_url: String,
    token: String,
}

impl ReqwestInfluxClient {
    fn new(base_url: &str) -> Self {
        Self::with_overrides(base_url, INFLUX_TOKEN, INFLUX_BUCKET)
    }

    /// Build a client targeted at the same `org` as the default, but
    /// with an override-able token + bucket. Used by error-path tests
    /// that need to provoke an authentication failure or hit a bucket
    /// that does not exist.
    fn with_overrides(base_url: &str, token: &str, bucket: &str) -> Self {
        Self {
            client: reqwest::Client::builder()
                .timeout(Duration::from_secs(10))
                .build()
                .expect("reqwest client"),
            write_url: format!(
                "{base_url}/api/v2/write?org={INFLUX_ORG}&bucket={bucket}&precision=ns"
            ),
            token: token.to_string(),
        }
    }
}

#[async_trait]
impl InfluxHttpClient for ReqwestInfluxClient {
    async fn write(&self, body: &str) -> Result<(), String> {
        let resp = self
            .client
            .post(&self.write_url)
            .header("Authorization", format!("Token {}", self.token))
            .header("Content-Type", "text/plain; charset=utf-8")
            .body(body.to_string())
            .send()
            .await
            .map_err(|e| format!("write request failed: {e}"))?;
        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(format!("influx write {status}: {text}"));
        }
        Ok(())
    }
}

// ── Query helper ──────────────────────────────────────────────────────

/// Run a Flux query and return the raw CSV body.
///
/// Flux returns annotated CSV; we just substring-match on it. Sufficient
/// to assert a written value round-trips faithfully.
async fn query_flux(base_url: &str, flux: &str) -> String {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .expect("reqwest client");
    let url = format!("{base_url}/api/v2/query?org={INFLUX_ORG}");
    let resp = client
        .post(&url)
        .header("Authorization", format!("Token {INFLUX_TOKEN}"))
        .header("Accept", "application/csv")
        .header("Content-Type", "application/vnd.flux")
        .body(flux.to_string())
        .send()
        .await
        .expect("query request");
    assert!(
        resp.status().is_success(),
        "query failed with status {}",
        resp.status()
    );
    resp.text().await.expect("query body")
}

/// Poll a Flux query until it returns a non-empty data row (Flux CSV has
/// a header line for the table; we look for a `_result` row). Returns
/// the full body once a data row appears, or panics on timeout.
async fn query_flux_until_data(base_url: &str, flux: &str, timeout: Duration) -> String {
    let deadline = Instant::now() + timeout;
    loop {
        let body = query_flux(base_url, flux).await;
        // Annotated CSV: each data row starts with `,_result,` —
        // checking for the table-name token is the simplest "we have
        // at least one row" predicate that survives header changes.
        if body.lines().any(|l| l.contains(",_result,")) {
            return body;
        }
        assert!(
            Instant::now() < deadline,
            "no rows returned by Flux query within {timeout:?}; body was:\n{body}"
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

// ── Tests ──────────────────────────────────────────────────────────────

/// 1. Push a single measurement through Batcher::push + flush; query
///    it back and confirm the field value survives.
#[tokio::test(flavor = "multi_thread")]
async fn influx_round_trip_via_batcher() {
    tokio::time::timeout(TEST_TIMEOUT, async {
        let (_c, base) = start_influx().await;
        let client = Arc::new(ReqwestInfluxClient::new(&base));
        let batcher = Batcher::new(client, 1024 * 1024, 4 * 1024 * 1024);

        // Unique measurement name so other tests cannot collide on this
        // bucket if they ever share a container (they don't today, but
        // it costs us nothing to be safe).
        let measurement = "cpu_round_trip";
        let line = LineProtocol::new(measurement)
            .tag("host", "server01")
            .field("usage", FieldValue::Float(0.42))
            .format()
            .expect("format");
        batcher.push(&line).await.expect("push");
        batcher.flush().await.expect("flush");

        let flux = format!(
            r#"from(bucket: "{INFLUX_BUCKET}")
              |> range(start: -1m)
              |> filter(fn: (r) => r._measurement == "{measurement}")"#
        );
        let body = query_flux_until_data(&base, &flux, Duration::from_secs(8)).await;
        assert!(
            body.contains("server01"),
            "expected tag 'server01' in query body:\n{body}"
        );
        assert!(
            body.contains("0.42"),
            "expected field value '0.42' in query body:\n{body}"
        );
    })
    .await
    .expect("round_trip timed out");
}

/// 2. INFLUX-001 escape audit: write a tag value with embedded
///    backslashes via the production escape pipeline; query back; assert
///    the *server-parsed* value matches what was pushed. The unit tests
///    only re-parse with our own escape function — only a real Influx
///    parser confirms spec compliance.
#[tokio::test(flavor = "multi_thread")]
async fn influx_escape_backslash_in_tag_parses_correctly() {
    tokio::time::timeout(TEST_TIMEOUT, async {
        let (_c, base) = start_influx().await;
        let client = Arc::new(ReqwestInfluxClient::new(&base));
        let batcher = Batcher::new(client, 1024 * 1024, 4 * 1024 * 1024);

        let measurement = "files_backslash";
        let tag_value = r"C:\Users\foo";
        let line = LineProtocol::new(measurement)
            .tag("path", tag_value)
            .field("size", FieldValue::Integer(123))
            .format()
            .expect("format");
        batcher.push(&line).await.expect("push");
        batcher.flush().await.expect("flush");

        let flux = format!(
            r#"from(bucket: "{INFLUX_BUCKET}")
              |> range(start: -1m)
              |> filter(fn: (r) => r._measurement == "{measurement}")
              |> keep(columns: ["path", "_value"])"#
        );
        let body = query_flux_until_data(&base, &flux, Duration::from_secs(8)).await;
        // The server must round-trip the *unescaped* tag value back.
        // If our escape were wrong, the server would either reject the
        // write (HTTP 400) or store a garbled value.
        assert!(
            body.contains(tag_value),
            "expected tag value '{tag_value}' to round-trip; got:\n{body}"
        );
    })
    .await
    .expect("backslash test timed out");
}

/// 3. Tag value containing `,` and `=` — the other audit-listed escape
///    classes. Same shape as the backslash test.
#[tokio::test(flavor = "multi_thread")]
async fn influx_escape_comma_and_equals_in_tag() {
    tokio::time::timeout(TEST_TIMEOUT, async {
        let (_c, base) = start_influx().await;
        let client = Arc::new(ReqwestInfluxClient::new(&base));
        let batcher = Batcher::new(client, 1024 * 1024, 4 * 1024 * 1024);

        let measurement = "events_comma_eq";
        let tag_value = "a=1,b=2";
        let line = LineProtocol::new(measurement)
            .tag("expr", tag_value)
            .field("count", FieldValue::Integer(1))
            .format()
            .expect("format");
        batcher.push(&line).await.expect("push");
        batcher.flush().await.expect("flush");

        let flux = format!(
            r#"from(bucket: "{INFLUX_BUCKET}")
              |> range(start: -1m)
              |> filter(fn: (r) => r._measurement == "{measurement}")
              |> keep(columns: ["expr", "_value"])"#
        );
        let body = query_flux_until_data(&base, &flux, Duration::from_secs(8)).await;
        assert!(
            body.contains(tag_value),
            "expected tag value '{tag_value}' to round-trip; got:\n{body}"
        );
    })
    .await
    .expect("comma/equals test timed out");
}

/// 4. INFLUX-002 max-age: a single small line that never trips the byte
///    threshold should still be delivered after `max_age` elapses and a
///    `tick()` fires. Verifies the server actually received the row.
#[tokio::test(flavor = "multi_thread")]
async fn influx_batcher_max_age_triggers_flush_to_server() {
    tokio::time::timeout(TEST_TIMEOUT, async {
        let (_c, base) = start_influx().await;
        let client = Arc::new(ReqwestInfluxClient::new(&base));
        let batcher = Batcher::with_max_age(
            client,
            10_000_000,
            20_000_000,
            Some(Duration::from_millis(200)),
        );

        let measurement = "max_age_probe";
        let line = LineProtocol::new(measurement)
            .tag("source", "tick_test")
            .field("value", FieldValue::Integer(7))
            .format()
            .expect("format");
        batcher.push(&line).await.expect("push");

        // Wait past max_age, then drive the periodic tick by hand.
        tokio::time::sleep(Duration::from_millis(300)).await;
        batcher.tick().await.expect("tick");

        let flux = format!(
            r#"from(bucket: "{INFLUX_BUCKET}")
              |> range(start: -1m)
              |> filter(fn: (r) => r._measurement == "{measurement}")"#
        );
        let body = query_flux_until_data(&base, &flux, Duration::from_secs(8)).await;
        assert!(
            body.contains("tick_test"),
            "expected the max_age-flushed row to land on the server; got:\n{body}"
        );
    })
    .await
    .expect("max_age test timed out");
}

/// 5. Drop-loss accounting works with the production HTTP client wired
///    in (not just the in-memory mock). Pushes are buffered, the batcher
///    is dropped without an explicit flush, and we assert
///    `dropped_total >= bytes_pushed`. We do *not* query the server —
///    drop-time loss is documented behavior; this test only confirms the
///    accounting fires when the production client path is in play.
#[tokio::test(flavor = "multi_thread")]
async fn influx_drop_warning_path() {
    tokio::time::timeout(TEST_TIMEOUT, async {
        let (_c, base) = start_influx().await;
        let client = Arc::new(ReqwestInfluxClient::new(&base));
        // Large threshold so no auto-flush fires.
        let batcher = Batcher::new(client, 10_000_000, 20_000_000);
        let dropped = Arc::clone(&batcher.dropped_total);

        let mut pushed_bytes: u64 = 0;
        for i in 0..20 {
            let line = LineProtocol::new("drop_path")
                .tag("i", i.to_string())
                .field("v", FieldValue::Integer(i64::from(i)))
                .format()
                .expect("format");
            pushed_bytes += (line.len() + 1) as u64; // +1 for the newline Batcher appends
            batcher.push(&line).await.expect("push");
        }

        drop(batcher);
        let accounted = dropped.load(std::sync::atomic::Ordering::Relaxed);
        assert!(
            accounted >= pushed_bytes,
            "expected dropped_total >= pushed bytes ({accounted} >= {pushed_bytes})"
        );
    })
    .await
    .expect("drop_warning test timed out");
}

// ── Error-path tests ───────────────────────────────────────────────────
//
// The happy-path tests above prove the wire-format is spec-correct.
// These two tests cover the *other* half of the contract: when InfluxDB
// rejects a write, the failure must surface to the caller as a typed
// `Err(_)` from `Batcher::flush`, with enough payload (status code and
// server body) for an operator to diagnose what went wrong. A silent
// failure here would be a correctness bug — buffered records would be
// "delivered" from the batcher's accounting standpoint but in fact
// dropped on the floor.

/// Override the default token with a value the server cannot match. The
/// `/api/v2/write` endpoint must respond with `401 Unauthorized`, and
/// that status must propagate through `ReqwestInfluxClient::write` and
/// out of `Batcher::flush` as an `Err` whose message carries the 401.
#[tokio::test(flavor = "multi_thread")]
async fn influx_bad_token_surfaces_401() {
    tokio::time::timeout(Duration::from_secs(30), async {
        let (_c, base) = start_influx().await;
        // Same container, same bucket — only the token is wrong.
        let client = Arc::new(ReqwestInfluxClient::with_overrides(
            &base,
            "bogus-token",
            INFLUX_BUCKET,
        ));
        let batcher = Batcher::new(client, 1024 * 1024, 4 * 1024 * 1024);

        let line = LineProtocol::new("auth_probe")
            .tag("host", "server01")
            .field("v", FieldValue::Integer(1))
            .format()
            .expect("format");
        batcher.push(&line).await.expect("push");

        let err = batcher
            .flush()
            .await
            .expect_err("flush must fail when the token is invalid");
        // `ReqwestInfluxClient::write` formats failures as
        //   "influx write {status}: {body}"
        // where {status} is reqwest's `StatusCode` Display (e.g.
        // "401 Unauthorized") and {body} is the raw server JSON. We
        // assert on the 401 code itself (stable) and on a substring of
        // InfluxDB 2.x's documented unauthorized response body which
        // contains the word "unauthorized".
        assert!(
            err.contains("401"),
            "expected error to carry HTTP 401 status; got: {err}"
        );
        assert!(
            err.to_lowercase().contains("unauthorized"),
            "expected error to mention 'unauthorized'; got: {err}"
        );
    })
    .await
    .expect("bad_token test timed out");
}

/// Correct token, but write to a bucket that was never provisioned.
/// `/api/v2/write` returns 404 for an unknown bucket on InfluxDB 2.7;
/// the error must surface with enough information for an operator to
/// see *which* bucket was missing.
#[tokio::test(flavor = "multi_thread")]
async fn influx_missing_bucket_surfaces_error() {
    tokio::time::timeout(Duration::from_secs(30), async {
        let (_c, base) = start_influx().await;
        let missing = "does-not-exist";
        let client = Arc::new(ReqwestInfluxClient::with_overrides(
            &base,
            INFLUX_TOKEN,
            missing,
        ));
        let batcher = Batcher::new(client, 1024 * 1024, 4 * 1024 * 1024);

        let line = LineProtocol::new("bucket_probe")
            .tag("host", "server01")
            .field("v", FieldValue::Integer(1))
            .format()
            .expect("format");
        batcher.push(&line).await.expect("push");

        let err = batcher
            .flush()
            .await
            .expect_err("flush must fail when the bucket does not exist");
        // InfluxDB 2.7 returns 404 for an unknown bucket on the write
        // endpoint; the body includes the bucket name in a "bucket
        // <name> not found"-style message. We assert on the 404 (stable
        // status code) and on the bucket name appearing somewhere in
        // the propagated body so an operator can see what was missing.
        assert!(
            err.contains("404"),
            "expected error to carry HTTP 404 status; got: {err}"
        );
        assert!(
            err.contains(missing),
            "expected error to mention the missing bucket name '{missing}'; got: {err}"
        );
    })
    .await
    .expect("missing_bucket test timed out");
}
