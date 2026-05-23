//! Optional `rdkafka` backend — wires the trait surface in `lib.rs` to a
//! real `rdkafka::StreamConsumer` + `FutureProducer`.
//!
//! Gated behind the `rdkafka-backend` feature so the crate stays buildable
//! on hosts without librdkafka system deps.

use crate::{KafkaConsumer, KafkaMessage, KafkaOffset, KafkaProducer, RebalanceEvent, TopicPartition};
use rdkafka::{
    consumer::{Consumer, StreamConsumer},
    producer::{FutureProducer, FutureRecord},
    Message, TopicPartitionList,
};
use std::time::Duration;

/// A thin wrapper around [`rdkafka::consumer::StreamConsumer`] satisfying
/// the [`KafkaConsumer`] trait.
pub struct RdKafkaConsumer {
    inner: StreamConsumer,
    rebalance_rx: tokio::sync::Mutex<tokio::sync::mpsc::UnboundedReceiver<RebalanceEvent>>,
}

impl std::fmt::Debug for RdKafkaConsumer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RdKafkaConsumer").finish()
    }
}

impl RdKafkaConsumer {
    /// Wrap an existing `StreamConsumer`. The caller is expected to have
    /// already configured the consumer (`group.id`, `bootstrap.servers`,
    /// etc.) and is responsible for installing a rebalance callback that
    /// forwards events to `rebalance_tx`.
    #[must_use]
    pub fn new(
        consumer: StreamConsumer,
        rebalance_rx: tokio::sync::mpsc::UnboundedReceiver<RebalanceEvent>,
    ) -> Self {
        Self {
            inner: consumer,
            rebalance_rx: tokio::sync::Mutex::new(rebalance_rx),
        }
    }
}

#[async_trait::async_trait]
impl KafkaConsumer for RdKafkaConsumer {
    async fn subscribe(&self, topics: &[String]) -> Result<(), String> {
        let refs: Vec<&str> = topics.iter().map(String::as_str).collect();
        self.inner
            .subscribe(&refs)
            .map_err(|e| format!("subscribe failed: {e}"))
    }

    async fn poll(&self) -> Result<Option<KafkaMessage>, String> {
        match self.inner.recv().await {
            Ok(msg) => {
                let payload = msg.payload().unwrap_or_default().to_vec();
                let key = msg.key().map(<[u8]>::to_vec);
                Ok(Some(KafkaMessage {
                    topic: msg.topic().to_string(),
                    partition: msg.partition(),
                    offset: msg.offset(),
                    key,
                    value: payload,
                    timestamp_ms: msg.timestamp().to_millis(),
                }))
            }
            Err(e) => Err(format!("consumer recv failed: {e}")),
        }
    }

    async fn poll_rebalance(&self) -> Result<Option<RebalanceEvent>, String> {
        let mut rx = self.rebalance_rx.lock().await;
        // try_recv equivalent — return None if nothing pending.
        match rx.try_recv() {
            Ok(ev) => Ok(Some(ev)),
            Err(tokio::sync::mpsc::error::TryRecvError::Empty) => Ok(None),
            Err(tokio::sync::mpsc::error::TryRecvError::Disconnected) => Ok(None),
        }
    }

    async fn commit_offset(&self, offset: &KafkaOffset) -> Result<(), String> {
        let mut tpl = TopicPartitionList::new();
        tpl.add_partition_offset(
            &offset.topic,
            offset.partition,
            rdkafka::Offset::Offset(offset.offset),
        )
        .map_err(|e| format!("invalid offset: {e}"))?;
        self.inner
            .commit(&tpl, rdkafka::consumer::CommitMode::Async)
            .map_err(|e| format!("commit failed: {e}"))
    }
}

/// A thin wrapper around [`rdkafka::producer::FutureProducer`] satisfying
/// the [`KafkaProducer`] trait.
#[derive(Clone)]
pub struct RdKafkaProducer {
    inner: FutureProducer,
}

impl std::fmt::Debug for RdKafkaProducer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RdKafkaProducer").finish()
    }
}

impl RdKafkaProducer {
    /// Wrap an existing `FutureProducer`.
    #[must_use]
    pub const fn new(producer: FutureProducer) -> Self {
        Self { inner: producer }
    }
}

#[async_trait::async_trait]
impl KafkaProducer for RdKafkaProducer {
    async fn send(
        &self,
        topic: &str,
        key: Option<&[u8]>,
        value: &[u8],
        timestamp_ms: Option<i64>,
    ) -> Result<(), String> {
        let mut record = FutureRecord::to(topic).payload(value);
        if let Some(k) = key {
            record = record.key(k);
        }
        if let Some(ts) = timestamp_ms {
            record = record.timestamp(ts);
        }
        match self.inner.send(record, Duration::from_secs(30)).await {
            Ok(_) => Ok(()),
            Err((e, _)) => Err(format!("send failed: {e}")),
        }
    }
}

/// Convert an `rdkafka` `TopicPartitionList` into our `TopicPartition`
/// vector — useful inside a rebalance callback.
#[must_use]
pub fn tpl_to_topic_partitions(tpl: &TopicPartitionList) -> Vec<TopicPartition> {
    tpl.elements()
        .iter()
        .map(|e| TopicPartition {
            topic: e.topic().to_string(),
            partition: e.partition(),
        })
        .collect()
}
