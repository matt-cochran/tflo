//! Adapter-level integration tests for the `rumqttc` backend.
//!
//! These tests spin up an `eclipse-mosquitto:2` container via
//! `testcontainers` and exercise the public adapter surface:
//!
//! - [`RumqttcProducer`] / [`RumqttcConsumer`]
//! - The [`MqttCursor`] / [`BoundedSet`] QoS-2 dedup pathway
//! - `QoS` conversion round-trip (`to_rumqttc_qos` / `from_rumqttc_qos`)
//!
//! Gated behind the `integration-tests` feature so the default
//! `cargo test` run stays hermetic (no Docker requirement).
//!
//! Run with:
//!
//! ```text
//! cargo test -p tflo-connect-mqtt --features integration-tests
//! ```

#![cfg(all(feature = "integration-tests", feature = "rumqttc-backend"))]
// Test files are separate compilation units that don't inherit crate-level
// allows from `lib.rs`. Tests use `unwrap` / `expect` / `panic` and inline
// helpers; that's idiomatic and these lints would otherwise drown the file.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::missing_panics_doc,
    clippy::missing_errors_doc,
    clippy::items_after_statements,
    clippy::let_underscore_must_use,
    clippy::map_err_ignore,
    missing_docs
)]

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use rumqttc::{AsyncClient, Event, EventLoop, Incoming, LastWill, MqttOptions, QoS as RuQoS};
use testcontainers::core::{ContainerPort, WaitFor};
use testcontainers::runners::AsyncRunner;
use testcontainers::{ContainerAsync, GenericImage, ImageExt};
use tflo_connect_mqtt::rumqttc_backend::{RumqttcConsumer, RumqttcProducer};
use tflo_connect_mqtt::{MqttConsumer, MqttCursor, MqttProducer, MqttPublish, Qos};
use tokio::time::timeout;

// ── Test infrastructure ───────────────────────────────────────────────

/// Per-test deadline. Keeps a hung broker / dropped event loop from
/// pinning the suite indefinitely.
const TEST_TIMEOUT: Duration = Duration::from_secs(15);

/// Process-local monotonic counter — combined with the wall clock and
/// the test name, gives unique topics/client-ids across concurrent runs
/// without dragging in `uuid`.
static UNIQ: AtomicU64 = AtomicU64::new(0);

fn uniq(tag: &str) -> String {
    let n = UNIQ.fetch_add(1, Ordering::Relaxed);
    let t = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("{tag}-{t}-{n}")
}

/// Bring up a mosquitto 2.x container exposing 1883 with anonymous
/// listener enabled (the image ships with `/mosquitto-no-auth.conf`
/// for exactly this purpose).
async fn start_mosquitto() -> (ContainerAsync<GenericImage>, u16) {
    let container = GenericImage::new("eclipse-mosquitto", "2")
        .with_exposed_port(ContainerPort::Tcp(1883))
        // mosquitto logs "mosquitto version 2.x.y running" once the
        // listener is up — that's our readiness signal.
        .with_wait_for(WaitFor::message_on_stderr("running"))
        .with_cmd(vec!["mosquitto", "-c", "/mosquitto-no-auth.conf"])
        .start()
        .await
        .expect("mosquitto container failed to start — is Docker running?");

    let port = container
        .get_host_port_ipv4(1883)
        .await
        .expect("failed to map host port for mosquitto 1883");
    (container, port)
}

/// Build a raw rumqttc client + event loop for tests that need to drive
/// the loop directly (subscriber-only / observer clients).
fn make_client(client_id: &str, port: u16) -> (AsyncClient, EventLoop) {
    let mut opts = MqttOptions::new(client_id, "127.0.0.1", port);
    opts.set_keep_alive(Duration::from_secs(5));
    AsyncClient::new(opts, 32)
}

/// Drain the event loop until a `SubAck` arrives or the deadline elapses.
/// Necessary because `AsyncClient::subscribe().await` only queues the
/// `SUBSCRIBE` packet — the network round-trip happens when the
/// `EventLoop` is polled, and the broker won't route subsequent
/// publishes to this subscriber until `SUBACK` is observed.
async fn await_suback(el: &mut EventLoop, deadline: Duration) -> Result<(), String> {
    let fut = async {
        loop {
            match el.poll().await {
                Ok(Event::Incoming(Incoming::SubAck(_))) => return Ok(()),
                Ok(_) => {}
                Err(e) => return Err(format!("event-loop error before SubAck: {e}")),
            }
        }
    };
    match timeout(deadline, fut).await {
        Ok(r) => r,
        Err(_) => Err("timed out waiting for SubAck".into()),
    }
}

/// Drain the event loop until a Publish arrives or the deadline elapses.
/// Discards `ConnAck` / `SubAck` / `PingResp` / etc.
async fn await_publish(
    el: &mut EventLoop,
    deadline: Duration,
) -> Result<rumqttc::Publish, String> {
    let fut = async {
        loop {
            match el.poll().await {
                Ok(Event::Incoming(Incoming::Publish(p))) => return Ok(p),
                Ok(_) => {}
                Err(e) => return Err(format!("event loop error: {e}")),
            }
        }
    };
    timeout(deadline, fut)
        .await
        .map_err(|_| "timed out waiting for publish".to_string())?
}

// ── 1. Round-trip publish/subscribe ──────────────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn mqtt_publish_subscribe_round_trip() {
    let (_container, port) = start_mosquitto().await;

    let topic = uniq("test/round-trip");
    let producer_id = uniq("producer");
    let consumer_id = uniq("consumer");

    // Consumer side — wrap an AsyncClient + EventLoop in RumqttcConsumer.
    let (cclient, cel) = make_client(&consumer_id, port);
    let consumer = RumqttcConsumer::new(cclient, cel);
    consumer
        .subscribe(&topic, Qos::AtLeastOnce)
        .await
        .expect("subscribe failed");

    // Give the SUBSCRIBE / SUBACK a moment to round-trip by polling once
    // inside a short deadline. The poll loop drops non-Publish events,
    // so we run a brief background drain.
    //
    // We achieve this by issuing the publish AFTER subscribing; the
    // adapter's `poll()` is itself the drain.

    // Producer side — same broker, different client id.
    let (pclient, mut pel) = make_client(&producer_id, port);
    let producer = RumqttcProducer::new(pclient);

    // Drive the producer's event loop in the background so the publish
    // ack flows. rumqttc's `AsyncClient::publish` returns once the
    // packet is queued; the broker round-trip happens on the EventLoop.
    let pel_task = tokio::spawn(async move {
        // Drain for a bounded window — enough to land the CONNECT /
        // CONNACK / PUBLISH / PUBACK exchange.
        let _ = timeout(Duration::from_secs(5), async {
            loop {
                if pel.poll().await.is_err() {
                    break;
                }
            }
        })
        .await;
    });

    // Give the consumer subscription a beat to land before publishing.
    // We do this by polling the consumer in a background task that
    // races with the publish.
    let consume_handle = tokio::spawn(async move {
        timeout(TEST_TIMEOUT, consumer.poll())
            .await
            .map_err(|_| "consumer timed out".to_string())?
    });

    // Tiny pause to let SUBACK land. 200ms is generous for localhost.
    tokio::time::sleep(Duration::from_millis(300)).await;

    let msg = MqttPublish {
        topic: topic.clone(),
        payload: b"hello".to_vec(),
        qos: Qos::AtLeastOnce,
        retain: false,
    };
    producer.publish(&msg).await.expect("publish failed");

    let received = consume_handle
        .await
        .expect("consumer task panicked")
        .expect("consumer poll error");
    let received = received.expect("consumer returned None (disconnect)");

    assert_eq!(received.topic, topic, "topic mismatch");
    assert_eq!(received.payload, b"hello", "payload mismatch");
    assert_eq!(received.qos, Qos::AtLeastOnce, "qos mismatch");
    assert!(!received.retain, "retain flag mismatch");

    pel_task.abort();
}

// ── 2. QoS-2 dedup through MqttCursor / BoundedSet ───────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn mqtt_qos2_dedup_via_bounded_set() {
    let (_container, port) = start_mosquitto().await;

    let topic = uniq("test/qos2-dedup");
    let producer_id = uniq("qos2-producer");
    let consumer_id = uniq("qos2-consumer");

    // Subscriber driven directly so we can collect every Publish
    // packet (including any retransmits the broker emits).
    let (sclient, mut sel) = make_client(&consumer_id, port);
    sclient
        .subscribe(&topic, RuQoS::ExactlyOnce)
        .await
        .expect("subscribe");
    // The SUBSCRIBE packet only goes on the wire when the event loop is
    // polled — drain until SubAck before publishing, otherwise the broker
    // has no record of the subscription when the publishes arrive.
    await_suback(&mut sel, Duration::from_secs(8))
        .await
        .expect("subscriber SubAck");

    // Producer.
    let (pclient, mut pel) = make_client(&producer_id, port);
    let pel_task = tokio::spawn(async move {
        let _ = timeout(Duration::from_secs(8), async {
            loop {
                if pel.poll().await.is_err() {
                    break;
                }
            }
        })
        .await;
    });

    // Publish the *same* payload twice at QoS 2. The broker, being
    // spec-compliant, will assign different packet IDs and dedupe its
    // own retransmits. To exercise the cursor's BoundedSet path we
    // instead simulate a redelivery by replaying the same packet_id
    // through the cursor directly — this is the unit-of-dedup the
    // adapter consults on the receiving side.
    let p1 = MqttPublish {
        topic: topic.clone(),
        payload: b"qos2-payload".to_vec(),
        qos: Qos::ExactlyOnce,
        retain: false,
    };
    pclient
        .publish(&p1.topic, RuQoS::ExactlyOnce, false, p1.payload.clone())
        .await
        .expect("publish 1");
    pclient
        .publish(&p1.topic, RuQoS::ExactlyOnce, false, p1.payload.clone())
        .await
        .expect("publish 2");

    // Collect the first Publish — the broker is contractually required
    // to deliver each at least once, so we expect two distinct
    // arrivals with two distinct packet ids.
    let first = await_publish(&mut sel, Duration::from_secs(8))
        .await
        .expect("first publish");
    let second = await_publish(&mut sel, Duration::from_secs(8))
        .await
        .expect("second publish");

    assert_eq!(first.qos, RuQoS::ExactlyOnce, "first qos");
    assert_eq!(second.qos, RuQoS::ExactlyOnce, "second qos");
    assert_eq!(first.payload, p1.payload);
    assert_eq!(second.payload, p1.payload);

    // Drive the BoundedSet dedup contract by feeding *the same*
    // packet id twice (simulating a broker retransmit of an
    // unacknowledged PUBLISH). First insertion is "new" → true;
    // second is "redelivery" → false.
    let mut cursor = MqttCursor::new(64).expect("cursor");
    let replay_id: u16 = first.pkid;
    assert!(
        cursor.observe_qos2(replay_id),
        "first observation of a new packet id must be new"
    );
    assert!(
        !cursor.observe_qos2(replay_id),
        "redelivery of the same packet id MUST be deduped by the cursor"
    );
    assert_eq!(cursor.last_packet_id, replay_id);
    assert_eq!(
        cursor.qos2_inflight_window.len(),
        1,
        "dedup window must hold exactly one entry after a redelivery"
    );

    // Sanity: the bounded window evicts under pressure — same contract,
    // but exercised end-to-end with adapter-issued packet ids.
    let mut tight = MqttCursor::new(2).expect("cursor");
    assert!(tight.observe_qos2(first.pkid));
    assert!(tight.observe_qos2(second.pkid));
    let synthetic_id: u16 = first.pkid.wrapping_add(second.pkid).wrapping_add(1);
    assert!(tight.observe_qos2(synthetic_id), "third id is new");
    // First id should have been evicted now.
    assert!(
        !tight.qos2_inflight_window.contains(first.pkid),
        "BoundedSet must evict the oldest under capacity pressure"
    );

    pel_task.abort();
}

// ── 3. QoS-level round-trip ──────────────────────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn mqtt_qos_levels() {
    let (_container, port) = start_mosquitto().await;

    let topic = uniq("test/qos-levels");
    let producer_id = uniq("qos-producer");
    let consumer_id = uniq("qos-consumer");

    let (cclient, cel) = make_client(&consumer_id, port);
    let consumer = RumqttcConsumer::new(cclient, cel);
    // Subscribing at QoS 2 means the broker may downgrade to the
    // publisher's QoS but won't upgrade above 2 — preserving per-publish
    // QoS for the assertion below.
    consumer
        .subscribe(&topic, Qos::ExactlyOnce)
        .await
        .expect("subscribe");
    // `RumqttcConsumer::subscribe` only queues the SUBSCRIBE packet —
    // the SUBACK round-trip happens inside the consumer's event loop.
    // Drive it briefly so the broker has the subscription before we
    // start publishing; otherwise publishes vanish into the broker
    // without ever reaching this client. `poll()` will return Pending
    // (no Publish yet); the timeout breaks us out once SUBACK has been
    // processed.
    let _ = timeout(Duration::from_millis(500), consumer.poll()).await;

    let (pclient, mut pel) = make_client(&producer_id, port);
    let producer = RumqttcProducer::new(pclient);
    let pel_task = tokio::spawn(async move {
        let _ = timeout(Duration::from_secs(10), async {
            loop {
                if pel.poll().await.is_err() {
                    break;
                }
            }
        })
        .await;
    });

    let qos_levels = [Qos::AtMostOnce, Qos::AtLeastOnce, Qos::ExactlyOnce];
    for q in qos_levels {
        producer
            .publish(&MqttPublish {
                topic: topic.clone(),
                payload: format!("qos-{q:?}").into_bytes(),
                qos: q,
                retain: false,
            })
            .await
            .expect("publish");
    }

    // Drain three messages from the adapter. Order: MQTT does not
    // guarantee global ordering across QoS levels, but a single
    // mosquitto broker delivers in publish order to one subscriber.
    let mut received = Vec::with_capacity(3);
    for _ in 0..3 {
        let msg = timeout(TEST_TIMEOUT, consumer.poll())
            .await
            .expect("consumer timeout")
            .expect("consumer error")
            .expect("consumer disconnect");
        received.push(msg);
    }

    assert_eq!(received.len(), 3);
    // Each delivered QoS should match the publisher's QoS — this
    // exercises both `to_rumqttc_qos` (producer path) and
    // `from_rumqttc_qos` (consumer path).
    for (msg, expected_qos) in received.iter().zip(qos_levels.iter()) {
        assert_eq!(
            msg.qos, *expected_qos,
            "QoS round-trip failed: sent {expected_qos:?} got {:?}",
            msg.qos
        );
        assert!(msg.payload.starts_with(b"qos-"), "payload prefix");
    }

    pel_task.abort();
}

// ── 4. Last-Will-and-Testament on abrupt disconnect ──────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn mqtt_will_message_published_on_disconnect() {
    let (_container, port) = start_mosquitto().await;

    let will_topic = uniq("test/will");
    let a_id = uniq("will-a");
    let b_id = uniq("will-b");

    // Client B — subscriber, no will.
    let (b_client, mut b_el) = make_client(&b_id, port);
    b_client
        .subscribe(&will_topic, RuQoS::AtLeastOnce)
        .await
        .expect("subscribe");

    // Drain B's loop until SUBACK lands.
    let _ = timeout(Duration::from_secs(2), async {
        loop {
            match b_el.poll().await {
                Ok(Event::Incoming(Incoming::SubAck(_))) => break,
                Ok(_) => {}
                Err(_) => break,
            }
        }
    })
    .await;

    // Client A — has a LWT. We need to (a) get CONNACK then (b) drop
    // the event loop abruptly so the broker fires the will.
    let mut a_opts = MqttOptions::new(&a_id, "127.0.0.1", port);
    a_opts.set_keep_alive(Duration::from_secs(2));
    a_opts.set_last_will(LastWill::new(
        &will_topic,
        b"offline".to_vec(),
        RuQoS::AtLeastOnce,
        false,
    ));
    let (_a_client, mut a_el) = AsyncClient::new(a_opts, 32);

    // Run A's event loop until CONNACK, then drop it. The broker will
    // observe the connection going away without a DISCONNECT packet
    // and publish the will to subscribers of `will_topic`.
    let connected = timeout(Duration::from_secs(5), async {
        loop {
            match a_el.poll().await {
                Ok(Event::Incoming(Incoming::ConnAck(_))) => break true,
                Ok(_) => {}
                Err(_) => break false,
            }
        }
    })
    .await
    .expect("A connect timed out");
    assert!(connected, "client A failed to connect");

    // Abrupt disconnect — drop the EventLoop without sending DISCONNECT.
    drop(a_el);

    // B drains until the will arrives. Broker's keep-alive grace may
    // delay this up to ~1.5 * keep_alive (configured at 2s above).
    let will = await_publish(&mut b_el, Duration::from_secs(10))
        .await
        .expect("did not receive will message");
    assert_eq!(will.topic, will_topic, "will topic mismatch");
    assert_eq!(will.payload, b"offline".as_ref(), "will payload mismatch");
    assert_eq!(will.qos, RuQoS::AtLeastOnce, "will qos mismatch");
}

// ── 5. Unreachable broker surfaces a typed error (no hang) ───────────
//
// Failure-mode coverage: the adapter MUST surface broker-unreachable as
// `Result::Err` from `poll()` rather than hang or block the caller
// indefinitely. We do NOT start a mosquitto container — instead the
// consumer is pointed at a port that is guaranteed not to have a broker
// on it. The first event-loop poll attempts a TCP connect; on a closed
// port the OS returns `ECONNREFUSED` immediately, which `rumqttc` lifts
// to `ConnectionError::Io(_)`, which our adapter formats as a `String`
// error beginning with `"event loop error: I/O:"`.
//
// Note on the PRODUCER side: `rumqttc::AsyncClient::publish` only
// enqueues a `Request::Publish` onto the in-process channel; the
// network round-trip happens inside the `EventLoop`, which the adapter
// owns separately. With no one polling the event loop the publish
// call returns `Ok(())` — there is no broker-unreachable error to
// surface at the producer's `publish()` boundary. The connection
// failure is observable only via the consumer-side `poll()` contract,
// which is what this test asserts. Producer-side coverage would need a
// new method on the adapter that drives its own event loop; that is
// out of scope here.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn mqtt_unreachable_broker_surfaces_typed_error() {
    // Port 1 (TCP MUX, reserved) is effectively never bound on a
    // developer machine; `connect()` returns `ECONNREFUSED` instantly.
    // We also keep `keep_alive` short so any retry path bounds out
    // quickly. No mosquitto container is started — this test runs on
    // any host, with or without Docker.
    let port: u16 = 1;
    let client_id = uniq("unreachable-consumer");

    let (cclient, cel) = make_client(&client_id, port);
    let consumer = RumqttcConsumer::new(cclient, cel);

    // Subscribe is a local channel send to the event loop — it will
    // succeed even though the broker is unreachable. The connection
    // error materialises on the first `poll()`.
    consumer
        .subscribe("unreachable/topic", Qos::AtLeastOnce)
        .await
        .expect("subscribe (channel send) should not fail");

    // The contract: `poll()` returns `Err(...)` *promptly* — not hangs.
    // 10s is generous; ECONNREFUSED on localhost is typically <1ms.
    let polled = timeout(Duration::from_secs(10), consumer.poll()).await;

    let result = polled.expect(
        "RumqttcConsumer::poll() did not return within 10s against \
         an unreachable broker — adapter is hanging on connection failure",
    );

    // Adapter error type: `Result<Option<MqttMessage>, String>`.
    // The error string is `format!("event loop error: {e}")` where `e`
    // is a `rumqttc::ConnectionError`. For an unreachable TCP port the
    // variant is `ConnectionError::Io(_)` whose `Display` impl is
    // `"I/O: <inner os error>"`. We pin both the adapter prefix and
    // the underlying variant to catch silent regressions in either.
    let err = result.expect_err(
        "RumqttcConsumer::poll() returned Ok against an unreachable \
         broker — expected Err surfacing the connection failure",
    );
    assert!(
        err.starts_with("event loop error:"),
        "expected adapter-formatted error prefix, got: {err:?}"
    );
    assert!(
        err.contains("I/O:") || err.to_lowercase().contains("refused"),
        "expected ConnectionError::Io (I/O / refused) variant, got: {err:?}"
    );
}
