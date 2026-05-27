//! Adapter-level integration tests for `tflo-connect-kafka`'s `rdkafka` backend.
//!
//! Each test spins up its own Confluent Kafka container via `testcontainers`
//! (the default Apache/Confluent variant re-exported by
//! `testcontainers-modules::kafka`) and exercises the *adapter contract*:
//! `RdKafkaConsumer`, `RdKafkaProducer`, and the `KafkaMessage` <-> rdkafka
//! wire-format translation. Domain/application behaviour is out of scope.
//!
//! Run with:
//!
//! ```text
//! cargo test -p tflo-connect-kafka --features integration-tests
//! ```
//!
//! Requires Docker on the host.

#![cfg(all(feature = "integration-tests", feature = "rdkafka-backend"))]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::OnceLock;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use rdkafka::admin::{AdminClient, AdminOptions, NewTopic, TopicReplication};
use rdkafka::client::DefaultClientContext;
use rdkafka::consumer::{Consumer, StreamConsumer};
use rdkafka::producer::{FutureProducer, FutureRecord};
use rdkafka::{ClientConfig, Offset, TopicPartitionList};
use testcontainers::core::ImageExt;
use testcontainers::runners::AsyncRunner;
use testcontainers::ContainerAsync;
use testcontainers_modules::kafka::{Kafka, KAFKA_PORT};
use tokio::sync::{mpsc::unbounded_channel, OwnedSemaphorePermit, Semaphore};

use tflo_connect_kafka::rdkafka_backend::{RdKafkaConsumer, RdKafkaProducer};
use tflo_connect_kafka::{
    KafkaConsumer, KafkaMessage, KafkaOffset, KafkaProducer, KafkaShardRouter, RebalanceEvent,
    TopicPartition,
};
use tflo_core::keyed::StateSnapshot;
use tflo_core::shard::ShardRouter;

// ── Test plumbing ─────────────────────────────────────────────────────

/// Counter for unique topic names — `testcontainers` guarantees each
/// container is isolated, but a per-test unique topic is cheap insurance.
static TOPIC_COUNTER: AtomicU64 = AtomicU64::new(0);

fn unique_topic(prefix: &str) -> String {
    let n = TOPIC_COUNTER.fetch_add(1, Ordering::Relaxed);
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("{prefix}-{ts}-{n}")
}

/// Global serialization gate. cargo test runs `#[test]` functions in
/// parallel by default; spinning up multiple Confluent CP-Kafka
/// containers concurrently has been observed to exhaust resources on
/// laptops (in particular Docker Desktop on WSL2) and trip the
/// `WaitContainer(StartupTimeout)` path. We let only one Kafka
/// container live at a time per test process — the permit is held by
/// the returned `KafkaGuard` for the test's whole scope. This keeps the
/// plain `cargo test -p tflo-connect-kafka --all-features` invocation
/// honest, without forcing callers to remember `--test-threads=1`.
static KAFKA_GATE: OnceLock<std::sync::Arc<Semaphore>> = OnceLock::new();

fn gate() -> std::sync::Arc<Semaphore> {
    KAFKA_GATE
        .get_or_init(|| std::sync::Arc::new(Semaphore::new(1)))
        .clone()
}

/// RAII handle returned by [`start_kafka`]: owns the running container
/// **and** the serialization permit. Both drop together at scope exit
/// so the next test can proceed.
#[allow(dead_code)] // fields exist for Drop side-effects only.
struct KafkaGuard {
    container: ContainerAsync<Kafka>,
    permit: OwnedSemaphorePermit,
}

/// Boot a Kafka container and return `(guard, bootstrap_servers)`.
/// The container is dropped when the guard goes out of scope.
///
/// Startup timeout is bumped to 5 minutes — the Confluent CP-Kafka image
/// boots Zookeeper + Kafka + runs a follow-up `kafka-configs` exec to
/// rewrite advertised listeners, and on contended hosts (especially WSL2
/// Docker Desktop) the default 60s can fall short.
async fn start_kafka() -> (KafkaGuard, String) {
    let permit = gate()
        .acquire_owned()
        .await
        .expect("KAFKA_GATE semaphore closed");
    let container = Kafka::default()
        .with_startup_timeout(Duration::from_secs(300))
        .start()
        .await
        .expect("kafka container start");
    let port = container
        .get_host_port_ipv4(KAFKA_PORT)
        .await
        .expect("kafka host port");
    let bootstrap = format!("127.0.0.1:{port}");
    (KafkaGuard { container, permit }, bootstrap)
}

fn producer(bootstrap: &str) -> FutureProducer {
    ClientConfig::new()
        .set("bootstrap.servers", bootstrap)
        .set("message.timeout.ms", "10000")
        .create()
        .expect("FutureProducer")
}

/// Build an `RdKafkaConsumer` against `bootstrap` with `group_id`, manual
/// commit, and earliest offset reset. The returned `_tx` keeps the
/// rebalance channel alive for the lifetime of the consumer.
fn consumer(
    bootstrap: &str,
    group_id: &str,
) -> (
    RdKafkaConsumer,
    tokio::sync::mpsc::UnboundedSender<RebalanceEvent>,
) {
    let stream: StreamConsumer = ClientConfig::new()
        .set("bootstrap.servers", bootstrap)
        .set("group.id", group_id)
        .set("session.timeout.ms", "6000")
        .set("enable.auto.commit", "false")
        .set("auto.offset.reset", "earliest")
        .create()
        .expect("StreamConsumer");
    let (tx, rx) = unbounded_channel::<RebalanceEvent>();
    (RdKafkaConsumer::new(stream, rx), tx)
}

fn admin(bootstrap: &str) -> AdminClient<DefaultClientContext> {
    ClientConfig::new()
        .set("bootstrap.servers", bootstrap)
        .create()
        .expect("AdminClient")
}

async fn create_topic(bootstrap: &str, name: &str, partitions: i32) {
    let admin = admin(bootstrap);
    let topic = NewTopic::new(name, partitions, TopicReplication::Fixed(1));
    let res = admin
        .create_topics(std::iter::once(&topic), &AdminOptions::new())
        .await
        .expect("create_topics call");
    for r in res {
        r.unwrap_or_else(|e| panic!("create_topics result error: {e:?}"));
    }
}

/// Poll the consumer with a deadline. Returns whatever the adapter
/// returned, or `None` if the deadline elapses with no message.
async fn poll_with_timeout(
    consumer: &RdKafkaConsumer,
    timeout: Duration,
) -> Option<Result<Option<KafkaMessage>, String>> {
    tokio::time::timeout(timeout, consumer.poll()).await.ok()
}

// ── Tests ─────────────────────────────────────────────────────────────

/// Produce a single message, consume it back, verify topic + payload
/// round-trip through the adapter intact.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn kafka_round_trip_payload() {
    let (_container, bootstrap) = start_kafka().await;
    let topic = unique_topic("test-topic");
    create_topic(&bootstrap, &topic, 1).await;

    let prod = RdKafkaProducer::new(producer(&bootstrap));
    let (cons, _rebalance_tx) = consumer(&bootstrap, &unique_topic("group"));

    cons.subscribe(&[topic.clone()])
        .await
        .expect("subscribe ok");

    prod.send(&topic, None, b"hello tflo", None)
        .await
        .expect("send ok");

    let msg = poll_with_timeout(&cons, Duration::from_secs(30))
        .await
        .expect("poll within deadline")
        .expect("poll ok")
        .expect("got a message");

    assert_eq!(msg.topic, topic);
    assert_eq!(msg.payload, Some(b"hello tflo".to_vec()));
}

/// KAFKA-002 contract: a `None` payload (tombstone marker) must round-trip
/// as `None` through the adapter — *not* `Some(vec![])`. Verifies the
/// typed distinction reaches the wire and back.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn kafka_tombstone_payload_is_none() {
    let (_container, bootstrap) = start_kafka().await;
    let topic = unique_topic("tombstones");
    create_topic(&bootstrap, &topic, 1).await;

    // Use rdkafka directly here: our `KafkaProducer` trait takes
    // `value: &[u8]` (no Option), so the tombstone send must go through
    // a `FutureRecord` whose payload was never set.
    let raw_producer = producer(&bootstrap);
    let (cons, _rebalance_tx) = consumer(&bootstrap, &unique_topic("group"));
    cons.subscribe(&[topic.clone()])
        .await
        .expect("subscribe ok");

    let record: FutureRecord<'_, [u8], [u8]> = FutureRecord::to(&topic).key(b"tombstone-key");
    raw_producer
        .send(record, Duration::from_secs(10))
        .await
        .expect("tombstone send ok");

    let msg = poll_with_timeout(&cons, Duration::from_secs(30))
        .await
        .expect("poll within deadline")
        .expect("poll ok")
        .expect("got a message");

    assert_eq!(msg.topic, topic);
    assert_eq!(
        msg.payload, None,
        "tombstone must surface as None, not Some(vec![])"
    );
    // Defence in depth: confirm the two representations are distinct.
    assert_ne!(msg.payload, Some(Vec::<u8>::new()));
}

/// KAFKA-001 contract: produce 5, consume 3, commit msg3's offset, drop
/// the consumer, open a new consumer with the same group_id, assert it
/// resumes at message 4. End-to-end test of `KafkaMessage::commit_offset`
/// against a real broker.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn kafka_offset_resume_after_commit() {
    let (_container, bootstrap) = start_kafka().await;
    let topic = unique_topic("resume");
    let group_id = unique_topic("group");
    create_topic(&bootstrap, &topic, 1).await;

    let prod = RdKafkaProducer::new(producer(&bootstrap));
    for i in 0..5_u8 {
        prod.send(&topic, None, format!("msg-{i}").as_bytes(), None)
            .await
            .expect("send ok");
    }

    // Phase 1: consume 3, commit the third's offset.
    let msg3_committable;
    {
        let (cons, _tx) = consumer(&bootstrap, &group_id);
        cons.subscribe(&[topic.clone()])
            .await
            .expect("subscribe ok");

        let mut last = None;
        for _ in 0..3 {
            let m = poll_with_timeout(&cons, Duration::from_secs(30))
                .await
                .expect("poll within deadline")
                .expect("poll ok")
                .expect("got a message");
            last = Some(m);
        }
        let msg3 = last.expect("read 3 messages");
        msg3_committable = msg3.commit_offset();

        let cursor = KafkaOffset::from_committable(topic.clone(), msg3.partition, msg3_committable);
        cons.commit_offset(&cursor).await.expect("commit ok");

        // Commit is queued async by the adapter — poll committed() until
        // the broker reports the expected value before dropping.
        let verifier: StreamConsumer = ClientConfig::new()
            .set("bootstrap.servers", &bootstrap)
            .set("group.id", &group_id)
            .set("enable.auto.commit", "false")
            .create()
            .expect("verifier consumer");
        let mut tpl = TopicPartitionList::new();
        tpl.add_partition(&topic, msg3.partition);
        let deadline = std::time::Instant::now() + Duration::from_secs(15);
        loop {
            let got = verifier
                .committed_offsets(tpl.clone(), Duration::from_secs(5))
                .expect("committed_offsets ok");
            let elem = got
                .find_partition(&topic, msg3.partition)
                .expect("partition entry");
            if matches!(elem.offset(), Offset::Offset(o) if o == msg3_committable.as_i64()) {
                break;
            }
            assert!(
                std::time::Instant::now() <= deadline,
                "broker did not observe committed offset {} within 15s; saw {:?}",
                msg3_committable.as_i64(),
                elem.offset()
            );
            tokio::time::sleep(Duration::from_millis(250)).await;
        }
    }

    // Phase 2: fresh consumer, same group → must resume at msg4.
    let (cons2, _tx2) = consumer(&bootstrap, &group_id);
    cons2
        .subscribe(&[topic.clone()])
        .await
        .expect("subscribe ok");

    let next = poll_with_timeout(&cons2, Duration::from_secs(30))
        .await
        .expect("poll within deadline")
        .expect("poll ok")
        .expect("got a message");
    assert_eq!(
        next.payload,
        Some(b"msg-3".to_vec()),
        "resume must deliver msg-3 (4th message, index 3) after committing offset of msg-2 (3rd, index 2)"
    );
    assert_eq!(next.offset, msg3_committable.as_i64());
}

/// `KafkaShardRouter::owns` must agree with the broker's partition
/// assignment for produced messages. Topic with 3 partitions; produce
/// messages with stable keys; assert each consumed message's
/// `(topic, partition)` is owned by the router after rebalance.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn kafka_shard_router_owned_partitions() {
    let (_container, bootstrap) = start_kafka().await;
    let topic = unique_topic("shards");
    create_topic(&bootstrap, &topic, 3).await;

    // Test-only AsyncStateStore: no-op, exists only so the router can be
    // constructed (KafkaShardRouter requires one by design).
    #[derive(Default)]
    struct NoopStore;
    #[async_trait::async_trait]
    impl tflo_core::state::AsyncStateStore for NoopStore {
        async fn save(&self, _k: &[u8], _s: &StateSnapshot) -> Result<(), String> {
            Ok(())
        }
        async fn load(&self, _k: &[u8]) -> Result<Option<StateSnapshot>, String> {
            Ok(None)
        }
        async fn list_keys(&self) -> Result<Vec<Vec<u8>>, String> {
            Ok(Vec::new())
        }
    }

    let router = KafkaShardRouter::new(std::sync::Arc::new(NoopStore));
    // Simulate the rebalance callback: a single consumer in the group
    // owns all 3 partitions.
    let all_parts: Vec<TopicPartition> = (0..3)
        .map(|p| TopicPartition {
            topic: topic.clone(),
            partition: p,
        })
        .collect();
    router
        .apply_rebalance(&RebalanceEvent::Assigned(all_parts.clone()))
        .expect("rebalance apply ok");

    // Produce messages with deterministic keys — each key hashes to some
    // partition; the broker decides which. We consume them back and
    // assert the router owns the partition the broker reported.
    let prod = RdKafkaProducer::new(producer(&bootstrap));
    let keys: [&[u8]; 6] = [b"a", b"b", b"c", b"d", b"e", b"f"];
    for k in keys {
        prod.send(&topic, Some(k), b"payload", None)
            .await
            .expect("send ok");
    }

    let (cons, _tx) = consumer(&bootstrap, &unique_topic("group"));
    cons.subscribe(&[topic.clone()])
        .await
        .expect("subscribe ok");

    for _ in 0..keys.len() {
        let m = poll_with_timeout(&cons, Duration::from_secs(30))
            .await
            .expect("poll within deadline")
            .expect("poll ok")
            .expect("got a message");
        let tp = TopicPartition {
            topic: m.topic.clone(),
            partition: m.partition,
        };
        assert!(
            router.owns(&tp),
            "router must own partition {} reported by broker for key {:?}",
            m.partition,
            m.key
        );
    }

    // Revoke one partition and confirm `owns()` now rejects it — the
    // ShardRouter trait's contract under rebalance.
    let revoked = TopicPartition {
        topic: topic.clone(),
        partition: 0,
    };
    router
        .apply_rebalance(&RebalanceEvent::Revoked(vec![revoked.clone()]))
        .expect("rebalance revoke ok");
    assert!(!router.owns(&revoked), "router must not own revoked partition");
}

/// Error-path contract: when the adapter is pointed at an unreachable
/// broker (port 1 is the IANA-reserved tcpmux port — nothing legitimate
/// listens there) and `poll()` is called, the call MUST return within the
/// configured rdkafka timeout budget with a typed `Err` whose message
/// contains a recognisable transport/broker/timeout token. It MUST NOT
/// hang past the budget and it MUST NOT panic.
///
/// This test does *not* start a Kafka container — the point is "no broker
/// available, surface that as an actionable error to the operator". It
/// therefore runs anywhere, fast (no Docker required), and guards the
/// negative path that the five happy-path tests above cannot reach.
///
/// Configured budgets:
///   - `socket.connection.setup.timeout.ms=2000` — bound TCP connect.
///   - `metadata.request.timeout.ms=3000` — bound metadata fetch.
///   - outer `tokio::time::timeout` of 10s — hard ceiling proving the
///     adapter doesn't hang past its declared budget.
///
/// Expected error message format from the adapter:
///   `consumer recv failed: <rdkafka KafkaError text>`
/// where the rdkafka text reliably contains one of:
///   "broker", "transport", "connect", or "timeout"
/// depending on whether ECONNREFUSED arrives before the metadata timeout.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn kafka_unreachable_broker_surfaces_typed_error() {
    // 127.0.0.1:1 — IANA-reserved tcpmux port; on Linux this usually
    // returns ECONNREFUSED immediately, but rdkafka may also surface the
    // metadata request timeout first. Either path satisfies the contract.
    let bootstrap = "127.0.0.1:1";

    // Build a StreamConsumer with aggressively low connect/metadata
    // timeouts so we don't sit on rdkafka's defaults (which are minutes).
    let stream: StreamConsumer = ClientConfig::new()
        .set("bootstrap.servers", bootstrap)
        .set("group.id", "kafka-unreachable-test")
        .set("enable.auto.commit", "false")
        .set("auto.offset.reset", "earliest")
        .set("socket.connection.setup.timeout.ms", "2000")
        .set("metadata.request.timeout.ms", "3000")
        // session.timeout.ms must be >= group.min.session.timeout.ms (6s
        // default) for create() to be willing to issue a JoinGroup later;
        // we never get that far, but keep it valid.
        .set("session.timeout.ms", "6000")
        .create()
        .expect("StreamConsumer create (config-only, no network)");
    let (_tx, rx) = unbounded_channel::<RebalanceEvent>();
    let cons = RdKafkaConsumer::new(stream, rx);

    // Subscribe before polling — librdkafka resolves the broker
    // asynchronously, and `poll()` -> `recv().await` is what observes the
    // failure. Subscribe itself only updates internal state and should
    // not fail synchronously.
    cons.subscribe(&["unreachable-topic".to_string()])
        .await
        .expect("subscribe is a local state update; should not require broker");

    // Hard ceiling: the adapter must surface an error within 10s. If
    // tokio's timeout fires, the adapter hung past its declared budget —
    // that is itself a failure of this contract.
    let outcome = tokio::time::timeout(Duration::from_secs(10), cons.poll()).await;

    let result = outcome.expect("adapter must surface broker-unreachable within 10s budget, not hang");

    // Must be the error path — getting `Ok(Some(_))` against 127.0.0.1:1
    // would mean something is listening there and the test environment
    // is broken; getting `Ok(None)` would mean the adapter swallowed the
    // error, equally bad.
    let err = result.expect_err("poll against unreachable broker must return Err, not Ok");

    // Adapter wraps rdkafka's KafkaError as:
    //   "consumer recv failed: {e}"
    // The {e} text from rdkafka for an unreachable bootstrap is one of:
    //   - "Message consumption error: BrokerTransportFailure (..)"
    //   - "Message consumption error: ... timed out"
    //   - "... Connection refused ..."
    // Any of {broker, transport, connect, timeout} (case-insensitive)
    // proves the adapter forwarded an actionable signal.
    assert!(
        err.starts_with("consumer recv failed:"),
        "adapter error must use the documented prefix; got: {err}"
    );
    let lower = err.to_lowercase();
    assert!(
        lower.contains("broker")
            || lower.contains("transport")
            || lower.contains("connect")
            || lower.contains("timeout")
            || lower.contains("timed out"),
        "error must surface a recognisable transport/broker token; got: {err}"
    );

    // ── Producer side: same contract. Build a FutureProducer pointing at
    // the same dead address with a tight `message.timeout.ms`, call
    // `send()` under a 10s outer ceiling, assert typed Err with the
    // documented "send failed:" prefix.
    let dead_producer = RdKafkaProducer::new(
        ClientConfig::new()
            .set("bootstrap.servers", bootstrap)
            .set("socket.connection.setup.timeout.ms", "2000")
            .set("metadata.request.timeout.ms", "3000")
            // message.timeout.ms is the queue + delivery deadline; this
            // is what FutureProducer::send().await observes.
            .set("message.timeout.ms", "3000")
            .create()
            .expect("FutureProducer create (config-only, no network)"),
    );

    let prod_outcome = tokio::time::timeout(
        Duration::from_secs(10),
        dead_producer.send("unreachable-topic", None, b"never-arrives", None),
    )
    .await;
    let prod_result =
        prod_outcome.expect("producer must surface broker-unreachable within 10s budget, not hang");
    let prod_err =
        prod_result.expect_err("producer send against unreachable broker must return Err, not Ok");
    assert!(
        prod_err.starts_with("send failed:"),
        "producer error must use the documented prefix; got: {prod_err}"
    );
    let prod_lower = prod_err.to_lowercase();
    assert!(
        prod_lower.contains("broker")
            || prod_lower.contains("transport")
            || prod_lower.contains("connect")
            || prod_lower.contains("timeout")
            || prod_lower.contains("timed out")
            || prod_lower.contains("queue"), // rdkafka may report "Local: Message timed out" or queue-full variants
        "producer error must surface a recognisable transport/broker token; got: {prod_err}"
    );
}

/// Type-enforced rule restated against a real broker: the offset committed
/// by `RdKafkaConsumer::commit_offset` for a message with offset `n` is
/// exactly `n + 1` on the broker side.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn kafka_commit_offset_is_plus_one() {
    let (_container, bootstrap) = start_kafka().await;
    let topic = unique_topic("commit-plus-one");
    let group_id = unique_topic("group");
    create_topic(&bootstrap, &topic, 1).await;

    let prod = RdKafkaProducer::new(producer(&bootstrap));
    prod.send(&topic, None, b"only-message", None)
        .await
        .expect("send ok");

    let (cons, _tx) = consumer(&bootstrap, &group_id);
    cons.subscribe(&[topic.clone()])
        .await
        .expect("subscribe ok");

    let msg = poll_with_timeout(&cons, Duration::from_secs(30))
        .await
        .expect("poll within deadline")
        .expect("poll ok")
        .expect("got a message");

    let committable = msg.commit_offset();
    assert_eq!(
        committable.as_i64(),
        msg.offset + 1,
        "the typed CommitableOffset must equal record offset + 1"
    );

    let cursor = KafkaOffset::from_committable(topic.clone(), msg.partition, committable);
    cons.commit_offset(&cursor).await.expect("commit ok");

    // Verify via a fresh sync consumer that the broker stored `offset + 1`.
    let verifier: StreamConsumer = ClientConfig::new()
        .set("bootstrap.servers", &bootstrap)
        .set("group.id", &group_id)
        .set("enable.auto.commit", "false")
        .create()
        .expect("verifier consumer");
    let mut tpl = TopicPartitionList::new();
    tpl.add_partition(&topic, msg.partition);

    let deadline = std::time::Instant::now() + Duration::from_secs(15);
    let observed = loop {
        let got = verifier
            .committed_offsets(tpl.clone(), Duration::from_secs(5))
            .expect("committed_offsets ok");
        let elem = got
            .find_partition(&topic, msg.partition)
            .expect("partition entry");
        if let Offset::Offset(o) = elem.offset() {
            break o;
        }
        assert!(
            std::time::Instant::now() <= deadline,
            "broker did not store a numeric committed offset within 15s"
        );
        tokio::time::sleep(Duration::from_millis(250)).await;
    };
    assert_eq!(
        observed,
        msg.offset + 1,
        "broker-side committed offset must be record offset + 1"
    );
}
