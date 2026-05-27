#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
//! End-to-end `IoT` portal reference deployment — Phase 6 of the
//! production roadmap.
//!
//! # Topology
//!
//! ```text
//! [N simulated sensors]
//!         │  (MQTT publish — QoS-1)
//!         ▼
//! [edge gateway: tflo-core + tflo-connect-mqtt]
//!         │  conditioning: SMA + threshold cross + hysteresis debounce
//!         │  (republish detected lifecycle events to Kafka)
//!         ▼
//! [central worker: tflo-core + tflo-connect-kafka + KafkaShardRouter
//!  + tflo-state-files (AsyncStateStore) + tflo-sink-influx
//!  + tflo-arrow (parquet archive)]
//!         │  trend detection + checkpointed state per partition
//!         ▼
//! [InfluxDB line-protocol writes]   [Parquet archive]
//! ```
//!
//! Real production would substitute:
//! - `MockMqttBroker` → a real broker (`mosquitto`) via
//!   `tflo-connect-mqtt`'s `rumqttc-backend` feature.
//! - `MockKafkaCluster` → a real broker (`redpanda`/`kafka`) via
//!   `tflo-connect-kafka`'s `rdkafka-backend` feature.
//! - `MockInfluxClient` → `reqwest`/`hyper` against an `influxdb` HTTP
//!   API endpoint.
//!
//! For this example we wire mock implementations so the example runs in
//! CI without external services, while exercising the full contracts:
//! `Cursor`, `AsyncStateStore`, `ShardRouter`, `Checkpointer`, `BoundedSet`
//! QoS-2 dedup, and the schema/builder fingerprint poka-yoke.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use tflo_arrow::schema_fingerprint;
use tflo_connect_kafka::{
    KafkaMessage, KafkaOffset, KafkaShardRouter, RebalanceEvent, TopicPartition,
};
use tflo_connect_mqtt::{MqttCursor, MqttMessage, MqttPublish, Qos};
use tflo_core::adapter::{CheckpointPolicy, Cursor};
use tflo_core::keyed::{SnapshotMetadata, StateSnapshot};
use tflo_core::shard::ShardRouter;
use tflo_core::state::{AsyncCursorStore, AsyncStateStore, Checkpointer};
use tflo_sink_influx::{Batcher, FieldValue, InfluxHttpClient, LineProtocol};
use tflo_state_files::FileStateStore;

// ── Mock MQTT broker ──────────────────────────────────────────────────

#[derive(Default)]
struct MockMqttBroker {
    subscribed: Mutex<Vec<String>>,
    published: Mutex<Vec<MqttPublish>>,
    queue: Mutex<Vec<MqttMessage>>,
}

impl MockMqttBroker {
    fn inject(&self, msg: MqttMessage) {
        self.queue.lock().unwrap().push(msg);
    }

    fn published_messages(&self) -> Vec<MqttPublish> {
        self.published.lock().unwrap().clone()
    }
}

#[async_trait]
impl tflo_connect_mqtt::MqttConsumer for MockMqttBroker {
    async fn subscribe(&self, topic_filter: &str, _qos: Qos) -> Result<(), String> {
        self.subscribed.lock().unwrap().push(topic_filter.into());
        Ok(())
    }

    async fn poll(&self) -> Result<Option<MqttMessage>, String> {
        Ok(self.queue.lock().unwrap().pop())
    }
}

#[async_trait]
impl tflo_connect_mqtt::MqttProducer for MockMqttBroker {
    async fn publish(&self, msg: &MqttPublish) -> Result<(), String> {
        self.published.lock().unwrap().push(msg.clone());
        Ok(())
    }
}

// ── Mock Kafka cluster ────────────────────────────────────────────────

#[derive(Default)]
struct MockKafkaCluster {
    messages: Mutex<Vec<KafkaMessage>>,
    rebalance_queue: Mutex<Vec<RebalanceEvent>>,
    committed_offsets: Mutex<HashMap<(String, i32), i64>>,
}

impl MockKafkaCluster {
    fn enqueue(&self, msg: KafkaMessage) {
        self.messages.lock().unwrap().push(msg);
    }

    #[allow(dead_code)] // available for future scenarios; not exercised in this short demo
    fn schedule_rebalance(&self, ev: RebalanceEvent) {
        self.rebalance_queue.lock().unwrap().push(ev);
    }

    fn committed(&self) -> HashMap<(String, i32), i64> {
        self.committed_offsets.lock().unwrap().clone()
    }
}

#[async_trait]
impl tflo_connect_kafka::KafkaConsumer for MockKafkaCluster {
    async fn subscribe(&self, _topics: &[String]) -> Result<(), String> {
        Ok(())
    }
    async fn poll(&self) -> Result<Option<KafkaMessage>, String> {
        Ok(self.messages.lock().unwrap().pop())
    }
    async fn poll_rebalance(&self) -> Result<Option<RebalanceEvent>, String> {
        Ok(self.rebalance_queue.lock().unwrap().pop())
    }
    async fn commit_offset(&self, offset: &KafkaOffset) -> Result<(), String> {
        self.committed_offsets
            .lock()
            .unwrap()
            .insert((offset.topic.clone(), offset.partition), offset.offset);
        Ok(())
    }
}

// ── Mock Influx HTTP client ───────────────────────────────────────────

#[derive(Default)]
struct MockInfluxClient {
    writes: Mutex<Vec<String>>,
}

impl MockInfluxClient {
    fn written(&self) -> Vec<String> {
        self.writes.lock().unwrap().clone()
    }
}

#[async_trait]
impl InfluxHttpClient for MockInfluxClient {
    async fn write(&self, body: &str) -> Result<(), String> {
        self.writes.lock().unwrap().push(body.to_string());
        Ok(())
    }
}

// ── Detection logic ───────────────────────────────────────────────────

/// One simulated sensor reading.
#[derive(Clone, Debug)]
struct Reading {
    device_id: String,
    ts_ms: i64,
    value: f64,
}

/// One detected lifecycle event published downstream.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
struct LifecycleEvent {
    device_id: String,
    ts_ms: i64,
    smoothed: f64,
    /// "above" | "below" | "stable"
    state: String,
}

fn detect_threshold(
    history: &mut HashMap<String, (f64, &'static str)>,
    reading: &Reading,
    threshold: f64,
    hysteresis: f64,
) -> Option<LifecycleEvent> {
    let entry = history
        .entry(reading.device_id.clone())
        .or_insert((reading.value, "stable"));
    // Simple exponential moving average — the engine has nicer
    // operators (sma/ema in tflo-ops); here we keep the example
    // self-contained.
    entry.0 = 0.9 * entry.0 + 0.1 * reading.value;
    let smoothed = entry.0;

    let new_state = match entry.1 {
        "above" if smoothed < threshold - hysteresis => "below",
        "below" if smoothed > threshold + hysteresis => "above",
        "stable" if smoothed > threshold + hysteresis => "above",
        "stable" if smoothed < threshold - hysteresis => "below",
        other => other,
    };

    if new_state != entry.1 {
        entry.1 = new_state;
        Some(LifecycleEvent {
            device_id: reading.device_id.clone(),
            ts_ms: reading.ts_ms,
            smoothed,
            state: new_state.to_string(),
        })
    } else {
        None
    }
}

// ── Main ──────────────────────────────────────────────────────────────

#[tokio::main]
async fn main() -> Result<(), String> {
    println!("=== tflo iot-portal reference deployment ===\n");

    // ── 0. Set up wiring ──────────────────────────────────────────────
    let mqtt_broker = Arc::new(MockMqttBroker::default());
    let kafka_cluster = Arc::new(MockKafkaCluster::default());
    let influx_client = Arc::new(MockInfluxClient::default());

    // State store: file-backed, but used through the AsyncStateStore
    // path (Phase 1 contract).
    let state_dir = std::env::temp_dir().join(format!(
        "tflo-iot-portal-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    let state_store = Arc::new(
        FileStateStore::new(&state_dir).map_err(|e| format!("FileStateStore: {e}"))?,
    );

    // KafkaShardRouter — required AsyncStateStore param is the
    // Phase 1 / Phase 2 poka-yoke.
    let router: KafkaShardRouter<FileStateStore> =
        KafkaShardRouter::new(Arc::clone(&state_store));

    // Cursor store for Kafka offsets.
    let cursor_store = Arc::new(tflo_connect_kafka::InMemoryCursorStore::new());

    // Checkpointer — the orchestrator that owns snapshot-before-cursor
    // write ordering.
    let checkpointer = Checkpointer::new(
        Arc::clone(&state_store) as Arc<dyn AsyncStateStore>,
        Arc::clone(&cursor_store) as Arc<dyn AsyncCursorStore<KafkaOffset>>,
        CheckpointPolicy::EveryNRecords { n: 5 },
        Duration::from_secs(2),
        3, // open the circuit after 3 consecutive failures
    );

    // Influx batcher.
    let batcher = Batcher::new(
        Arc::clone(&influx_client) as Arc<dyn InfluxHttpClient>,
        4096,
        1024 * 1024,
    );

    // Detector state (one EMA + threshold-state per device).
    let mut history: HashMap<String, (f64, &'static str)> = HashMap::new();

    // ── 1. Simulate sensor traffic into MQTT ──────────────────────────
    println!("[edge] simulating 30 sensor readings across 3 devices...");
    let readings: Vec<Reading> = (0..30)
        .map(|i| {
            let device_id = format!("dev-{:02}", i % 3);
            let ts_ms = 1_700_000_000_000 + i as i64 * 100;
            // Drift a sine wave with a low-frequency trend so threshold
            // crossings actually happen.
            let phase = (i as f64) * 0.3;
            let value = 50.0 + 30.0 * phase.sin() + (i as f64) * 0.5;
            Reading {
                device_id,
                ts_ms,
                value,
            }
        })
        .collect();

    // Publish each reading as an MQTT message.
    let mut packet_id: u16 = 0;
    for r in &readings {
        packet_id = packet_id.wrapping_add(1);
        let payload = serde_json::to_vec(&serde_json::json!({
            "device_id": &r.device_id,
            "ts_ms": r.ts_ms,
            "value": r.value,
        }))
        .map_err(|e| e.to_string())?;
        mqtt_broker.inject(MqttMessage {
            topic: format!("sensors/{}/readings", r.device_id),
            payload,
            qos: Qos::ExactlyOnce,
            retain: false,
            packet_id: Some(packet_id),
        });
    }
    let mut cursor = MqttCursor::new(1024).expect("ok");

    // ── 2. Edge gateway: MQTT consume + conditioning + Kafka publish ──
    println!("[edge] consuming MQTT, conditioning, republishing to Kafka...\n");
    use tflo_connect_mqtt::MqttConsumer;
    use tflo_connect_mqtt::MqttProducer;

    let mut edge_dedup_drops = 0u64;
    let mut edge_detected = 0u64;
    while let Some(msg) = mqtt_broker.poll().await? {
        if let Some(pid) = msg.packet_id {
            if !cursor.observe_qos2(pid) {
                edge_dedup_drops += 1;
                continue;
            }
        }
        let reading: Reading = {
            let v: serde_json::Value =
                serde_json::from_slice(&msg.payload).map_err(|e| e.to_string())?;
            Reading {
                device_id: v["device_id"].as_str().unwrap_or("?").to_string(),
                ts_ms: v["ts_ms"].as_i64().unwrap_or(0),
                value: v["value"].as_f64().unwrap_or(0.0),
            }
        };
        if let Some(ev) = detect_threshold(&mut history, &reading, 60.0, 2.0) {
            edge_detected += 1;
            // Republish to Kafka.
            let payload = serde_json::to_vec(&ev).map_err(|e| e.to_string())?;
            mqtt_broker
                .publish(&MqttPublish {
                    topic: format!("events/{}/lifecycle", ev.device_id),
                    payload: payload.clone(),
                    qos: Qos::AtLeastOnce,
                    retain: false,
                })
                .await?;
            kafka_cluster.enqueue(KafkaMessage {
                topic: "lifecycle-events".into(),
                partition: (ev.device_id.bytes().last().unwrap_or(0) as i32) % 3,
                offset: edge_detected as i64,
                key: Some(ev.device_id.as_bytes().to_vec()),
                payload: Some(payload),
                timestamp_ms: Some(ev.ts_ms),
            });
        }
    }

    println!(
        "[edge] mqtt cursor: {} (dedup-drops={}, detected={})\n",
        cursor.display(),
        edge_dedup_drops,
        edge_detected
    );

    // ── 3. Central worker: rebalance, consume, checkpoint, Influx ────
    println!("[central] applying rebalance: assign partitions 0,1,2...");
    router
        .apply_rebalance(&RebalanceEvent::Assigned(vec![
            TopicPartition {
                topic: "lifecycle-events".into(),
                partition: 0,
            },
            TopicPartition {
                topic: "lifecycle-events".into(),
                partition: 1,
            },
            TopicPartition {
                topic: "lifecycle-events".into(),
                partition: 2,
            },
        ]))
        .map_err(|e| e.to_string())?;
    println!(
        "[central] router epoch is now {}, owns 3 partitions",
        ShardRouter::<TopicPartition>::assignment_epoch(&router)
    );

    use tflo_connect_kafka::KafkaConsumer;
    let mut consumed = 0u64;
    let mut stale_drops = 0u64;
    while let Some(msg) = kafka_cluster.poll().await? {
        let tp = TopicPartition {
            topic: msg.topic.clone(),
            partition: msg.partition,
        };
        // Ownership + epoch fence.
        if !router.owns(&tp) {
            stale_drops += 1;
            router.record_stale_event();
            continue;
        }
        consumed += 1;

        // Write to Influx via line-protocol.
        let payload = msg.payload.as_deref().ok_or_else(|| "tombstone message".to_string())?;
        let event: LifecycleEvent = serde_json::from_slice(payload).map_err(|e| e.to_string())?;
        let line = LineProtocol::new("lifecycle")
            .tag("device_id", &event.device_id)
            .tag("state", &event.state)
            .field("smoothed", FieldValue::Float(event.smoothed))
            .timestamp_ms(event.ts_ms)
            .format()?;
        batcher.push(&line).await?;

        // Per-N-records checkpoint via the Checkpointer (snapshot then
        // cursor, ordered).
        if checkpointer
            .policy()
            .should_checkpoint(consumed as usize, 0)
        {
            // For the example, snapshot bytes are placeholder; in
            // production a CompiledGraph::snapshot() would feed here.
            let snap = StateSnapshot {
                data: format!("snapshot@{consumed}").into_bytes(),
                metadata: SnapshotMetadata {
                    key: Some(tp.topic.as_bytes().to_vec()),
                    timestamp_ms: event.ts_ms,
                    version: 1,
                    topology_fingerprint: Some([0xab; 32]),
                },
            };
            let off = KafkaOffset::from_committable(
                msg.topic.clone(),
                msg.partition,
                msg.commit_offset(),
            );
            checkpointer
                .commit(tp.topic.as_bytes(), &snap, &off)
                .await
                .map_err(|e| format!("checkpoint: {e}"))?;
            kafka_cluster.commit_offset(&off).await?;
        }
    }

    // Flush whatever's left in the Influx batcher.
    batcher.flush().await?;

    // ── 4. Mid-run rebalance simulation (poka-yoke demonstration) ────
    println!(
        "\n[central] consumed={consumed} events, stale-drops={stale_drops}, \
         checkpoints commits={}",
        checkpointer
            .commits_total
            .load(std::sync::atomic::Ordering::Relaxed)
    );

    // Simulate a rebalance revoking partition 1 — the epoch must bump.
    let epoch_before = ShardRouter::<TopicPartition>::assignment_epoch(&router);
    router
        .apply_rebalance(&RebalanceEvent::Revoked(vec![TopicPartition {
            topic: "lifecycle-events".into(),
            partition: 1,
        }]))
        .map_err(|e| e.to_string())?;
    let epoch_after = ShardRouter::<TopicPartition>::assignment_epoch(&router);
    assert!(
        epoch_after > epoch_before,
        "epoch must bump on rebalance"
    );
    println!(
        "[central] rebalance: epoch {epoch_before} -> {epoch_after} (partition 1 revoked)"
    );

    // ── 5. Demonstrate schema fingerprint for Parquet backfill ───────
    use arrow::array::{Float64Array, Int64Array, StringArray};
    use arrow::record_batch::RecordBatch;
    use arrow_schema::{DataType, Field, Schema};

    let schema = Arc::new(Schema::new(vec![
        Field::new("ts_ms", DataType::Int64, false),
        Field::new("device_id", DataType::Utf8, false),
        Field::new("smoothed", DataType::Float64, false),
    ]));
    let fp = schema_fingerprint(&schema);
    println!(
        "\n[archive] arrow schema fingerprint = {:02x}{:02x}...{:02x}{:02x}",
        fp[0], fp[1], fp[30], fp[31]
    );

    // Build an in-memory RecordBatch of detections.
    let ts_arr = Int64Array::from(vec![1_700_000_000_000, 1_700_000_000_100]);
    let dev_arr = StringArray::from(vec!["dev-00", "dev-01"]);
    let val_arr = Float64Array::from(vec![61.2, 58.4]);
    let batch = RecordBatch::try_new(
        schema,
        vec![Arc::new(ts_arr), Arc::new(dev_arr), Arc::new(val_arr)],
    )
    .map_err(|e| e.to_string())?;
    println!(
        "[archive] built RecordBatch: {} rows × {} columns",
        batch.num_rows(),
        batch.num_columns()
    );

    // Report what flowed where.
    println!("\n=== summary ===");
    println!(
        "MQTT messages published downstream:    {}",
        mqtt_broker.published_messages().len()
    );
    println!("Kafka offsets committed (by partition):");
    for ((topic, partition), offset) in kafka_cluster.committed() {
        println!("  {topic}/{partition} -> {offset}");
    }
    println!(
        "Influx line-protocol batches written:  {}",
        influx_client.written().len()
    );
    println!(
        "Total Influx bytes written:            {}",
        influx_client
            .written()
            .iter()
            .map(String::len)
            .sum::<usize>()
    );

    // Cleanup.
    let _ = std::fs::remove_dir_all(&state_dir);
    println!("\niot-portal reference deployment: done");
    Ok(())
}
