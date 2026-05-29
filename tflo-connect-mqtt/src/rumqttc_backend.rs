//! Optional `rumqttc` backend — wires the trait surface in `lib.rs` to a
//! real `rumqttc::AsyncClient` + `EventLoop`. Gated behind the
//! `rumqttc-backend` feature so the crate stays minimal-deps for trait-
//! only consumers.
//!
//! This is a thin, single-thread driver suitable for an edge gateway
//! process. It is intentionally not a clustered MQTT runtime — fan-out
//! at scale terminates MQTT at the edge and republishes through a
//! cluster-capable message bus (Kafka, NATS).

use crate::{MqttConsumer, MqttMessage, MqttProducer, MqttPublish, Qos};
use rumqttc::{AsyncClient, Event, EventLoop, Incoming, QoS as RuQoS};
use tokio::sync::Mutex;

const fn to_rumqttc_qos(q: Qos) -> RuQoS {
    match q {
        Qos::AtMostOnce => RuQoS::AtMostOnce,
        Qos::AtLeastOnce => RuQoS::AtLeastOnce,
        Qos::ExactlyOnce => RuQoS::ExactlyOnce,
    }
}

const fn from_rumqttc_qos(q: RuQoS) -> Qos {
    match q {
        RuQoS::AtMostOnce => Qos::AtMostOnce,
        RuQoS::AtLeastOnce => Qos::AtLeastOnce,
        RuQoS::ExactlyOnce => Qos::ExactlyOnce,
    }
}

/// A [`MqttConsumer`] backed by an `rumqttc` `EventLoop`. Wrap an
/// already-constructed `AsyncClient` + `EventLoop` and call
/// [`subscribe`](MqttConsumer::subscribe) / [`poll`](MqttConsumer::poll)
/// against it.
pub struct RumqttcConsumer {
    client: AsyncClient,
    event_loop: Mutex<EventLoop>,
}

impl std::fmt::Debug for RumqttcConsumer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RumqttcConsumer").finish()
    }
}

impl RumqttcConsumer {
    /// Wrap an existing `(AsyncClient, EventLoop)` pair.
    #[must_use]
    pub fn new(client: AsyncClient, event_loop: EventLoop) -> Self {
        Self {
            client,
            event_loop: Mutex::new(event_loop),
        }
    }
}

#[async_trait::async_trait]
impl MqttConsumer for RumqttcConsumer {
    async fn subscribe(&self, topic_filter: &str, qos: Qos) -> Result<(), String> {
        self.client
            .subscribe(topic_filter, to_rumqttc_qos(qos))
            .await
            .map_err(|e| format!("subscribe failed: {e}"))
    }

    async fn poll(&self) -> Result<Option<MqttMessage>, String> {
        let mut el = self.event_loop.lock().await;
        loop {
            match el.poll().await {
                Ok(Event::Incoming(Incoming::Publish(p))) => {
                    return Ok(Some(MqttMessage {
                        topic: p.topic,
                        payload: p.payload.to_vec(),
                        qos: from_rumqttc_qos(p.qos),
                        retain: p.retain,
                        packet_id: Some(p.pkid),
                    }));
                }
                Ok(Event::Incoming(Incoming::Disconnect)) => return Ok(None),
                Ok(_) => {}
                Err(e) => return Err(format!("event loop error: {e}")),
            }
        }
    }
}

/// A [`MqttProducer`] backed by a `rumqttc::AsyncClient`.
#[derive(Clone)]
pub struct RumqttcProducer {
    client: AsyncClient,
}

impl std::fmt::Debug for RumqttcProducer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RumqttcProducer").finish()
    }
}

impl RumqttcProducer {
    /// Wrap an existing `AsyncClient`.
    #[must_use]
    pub const fn new(client: AsyncClient) -> Self {
        Self { client }
    }
}

#[async_trait::async_trait]
impl MqttProducer for RumqttcProducer {
    async fn publish(&self, msg: &MqttPublish) -> Result<(), String> {
        self.client
            .publish(
                &msg.topic,
                to_rumqttc_qos(msg.qos),
                msg.retain,
                msg.payload.clone(),
            )
            .await
            .map_err(|e| format!("publish failed: {e}"))
    }
}
