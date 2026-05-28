#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing, clippy::arithmetic_side_effects))]
//! MQTT adapter for tflo (Phase 3) — edge-friendly Source/Sink + a
//! bounded QoS-2 dedup cursor.
//!
//! # What's here
//!
//! - [`MqttCursor`] — small `Cursor` impl with a **bounded** QoS-2
//!   in-flight window. The bound is the poka-yoke against the most common
//!   edge-side memory leak ("dedup set grows forever").
//! - [`MqttPublish`] / [`MqttMessage`] — the message types crossing the
//!   trait boundary.
//! - [`MqttConsumer`] / [`MqttProducer`] — minimal async traits a client
//!   library must satisfy. Concrete `rumqttc` impls live behind the
//!   `rumqttc-backend` feature.
//! - No `ShardRouter` impl — MQTT terminates at the edge, single-process.
//!   The fan-out from edge to central Kafka is the user's pattern, not
//!   `tflo-connect-mqtt`'s concern.
//!
//! # Why `rumqttc`
//!
//! Pure Rust, no C dependencies — cross-compiles cleanly to ARM and WASM
//! gateways. That matches the Phase 3 "edge complement" use case.

#![warn(missing_docs)]
#![warn(missing_debug_implementations)]
#![deny(unsafe_code)]

use std::collections::VecDeque;
use tflo_core::adapter::Cursor;

#[cfg(feature = "rumqttc-backend")]
pub mod rumqttc_backend;

// ── Bounded set used by MqttCursor.qos2_inflight_window ────────────────

/// Fixed-capacity insertion-ordered set used for QoS-2 in-flight dedup.
///
/// The first inserted item is evicted to make room when the set is full.
/// This is intentionally O(N) on `contains` — the bound is small (default
/// 1024) and the access pattern is "check on every incoming packet."
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct BoundedSet {
    capacity: usize,
    items: VecDeque<u16>,
}

impl BoundedSet {
    /// Construct with a capacity. Panics is not used — a zero capacity
    /// degenerates to "never remember" rather than failing.
    #[must_use]
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity,
            items: VecDeque::with_capacity(capacity),
        }
    }

    /// Number of items currently held.
    #[must_use]
    pub fn len(&self) -> usize {
        self.items.len()
    }

    /// True when the set is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// Insert. Returns `true` when the value is newly added, `false` if
    /// it was already present. Evicts the oldest when over capacity.
    pub fn insert(&mut self, value: u16) -> bool {
        if self.contains(value) {
            return false;
        }
        if self.items.len() >= self.capacity && self.capacity > 0 {
            let _ = self.items.pop_front();
        }
        if self.capacity > 0 {
            self.items.push_back(value);
        }
        true
    }

    /// Membership check — O(N), bounded by `capacity`.
    #[must_use]
    pub fn contains(&self, value: u16) -> bool {
        self.items.iter().any(|v| *v == value)
    }

    /// The configured capacity.
    #[must_use]
    pub const fn capacity(&self) -> usize {
        self.capacity
    }
}

/// Maximum allowed QoS-2 in-flight window size. Pre-Phase-3 the dedup
/// set had no bound; this constant exists so callers can compare against
/// a documented ceiling.
pub const MAX_QOS2_WINDOW_SIZE: usize = 64 * 1024;

// ── MqttCursor ─────────────────────────────────────────────────────────

/// MQTT progress cursor — implements [`Cursor`].
///
/// Carries the last seen packet id, a bounded in-flight QoS-2 window for
/// dedup, and a small per-topic monotonic sequence map for ordering
/// hints. The QoS-2 window is **bounded** (see [`BoundedSet`]) —
/// pre-Phase-3 this was unbounded and a known edge-memory hazard.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MqttCursor {
    /// Most recent packet id observed across all topics.
    pub last_packet_id: u16,
    /// Bounded set of in-flight QoS-2 packet ids — the
    /// "have I seen this before" set.
    pub qos2_inflight_window: BoundedSet,
    /// Per-topic monotonic sequence counter — best-effort ordering aid.
    pub retained_topics: std::collections::HashMap<String, u64>,
}

impl MqttCursor {
    /// Construct with the given QoS-2 window size.
    ///
    /// # Errors
    ///
    /// Returns an error string when `qos2_window_size` exceeds
    /// [`MAX_QOS2_WINDOW_SIZE`] — the poka-yoke against accidental
    /// unbounded growth.
    pub fn new(qos2_window_size: usize) -> Result<Self, String> {
        if qos2_window_size > MAX_QOS2_WINDOW_SIZE {
            return Err(format!(
                "qos2_window_size {qos2_window_size} exceeds MAX_QOS2_WINDOW_SIZE \
                 ({MAX_QOS2_WINDOW_SIZE}) — refuse to accept an unbounded dedup window"
            ));
        }
        Ok(Self {
            last_packet_id: 0,
            qos2_inflight_window: BoundedSet::new(qos2_window_size),
            retained_topics: std::collections::HashMap::new(),
        })
    }

    /// Construct a cursor with the default 1024-packet QoS-2 window.
    #[must_use]
    pub fn with_default_window() -> Self {
        Self::new(1024).unwrap_or_else(|_| Self {
            last_packet_id: 0,
            qos2_inflight_window: BoundedSet::new(1024),
            retained_topics: std::collections::HashMap::new(),
        })
    }

    /// Mark a QoS-2 packet id as in-flight. Returns `true` if it was new
    /// (i.e. should be processed) and `false` if this is a redelivery
    /// (i.e. should be deduplicated).
    pub fn observe_qos2(&mut self, packet_id: u16) -> bool {
        self.last_packet_id = packet_id;
        self.qos2_inflight_window.insert(packet_id)
    }
}

impl Cursor for MqttCursor {
    fn to_bytes(&self) -> Vec<u8> {
        serde_json::to_vec(self).unwrap_or_default()
    }
    fn from_bytes(data: &[u8]) -> Result<Self, String> {
        serde_json::from_slice(data).map_err(|e| format!("Failed to deserialize MqttCursor: {e}"))
    }
    fn display(&self) -> String {
        format!(
            "MqttCursor(last_id={}, dedup={}/{}, topics={})",
            self.last_packet_id,
            self.qos2_inflight_window.len(),
            self.qos2_inflight_window.capacity(),
            self.retained_topics.len()
        )
    }
}

// ── MqttPublish / MqttMessage ──────────────────────────────────────────

/// `QoS` level passed across the trait boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Qos {
    /// At most once.
    AtMostOnce,
    /// At least once.
    AtLeastOnce,
    /// Exactly once.
    ExactlyOnce,
}

/// A message received from MQTT (consumer side).
#[derive(Debug, Clone)]
pub struct MqttMessage {
    /// Topic the message arrived on.
    pub topic: String,
    /// Payload bytes.
    pub payload: Vec<u8>,
    /// `QoS` level the message was delivered at.
    pub qos: Qos,
    /// Whether this is a retained message.
    pub retain: bool,
    /// Packet id, if the broker provided one (QoS≥1).
    pub packet_id: Option<u16>,
}

/// A message to publish (producer side).
#[derive(Debug, Clone)]
pub struct MqttPublish {
    /// Topic to publish to.
    pub topic: String,
    /// Payload bytes.
    pub payload: Vec<u8>,
    /// `QoS` level.
    pub qos: Qos,
    /// Retain flag.
    pub retain: bool,
}

// ── MqttConsumer / MqttProducer ────────────────────────────────────────

/// Minimal async MQTT consumer trait. A concrete `rumqttc` impl lives in
/// [`rumqttc_backend`] (feature `rumqttc-backend`).
#[cfg(feature = "async")]
#[async_trait::async_trait]
pub trait MqttConsumer: Send + Sync {
    /// Subscribe to a topic filter with the given QoS. Idempotent.
    ///
    /// # Errors
    ///
    /// Returns an error string when the underlying client rejects the
    /// subscription.
    async fn subscribe(&self, topic_filter: &str, qos: Qos) -> Result<(), String>;

    /// Poll for the next message; `None` means the consumer has been
    /// shut down or no more messages will arrive.
    ///
    /// # Errors
    ///
    /// Returns an error string on connection / protocol failure.
    async fn poll(&self) -> Result<Option<MqttMessage>, String>;
}

/// Minimal async MQTT producer trait.
#[cfg(feature = "async")]
#[async_trait::async_trait]
pub trait MqttProducer: Send + Sync {
    /// Publish a message; resolves once the local client has serialized
    /// the packet (or, for QoS>0, once the broker acks).
    ///
    /// # Errors
    ///
    /// Returns an error string on connection / serialization failure.
    async fn publish(&self, msg: &MqttPublish) -> Result<(), String>;
}

// ── Tests ──────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bounded_set_evicts_oldest() {
        let mut s = BoundedSet::new(3);
        assert!(s.insert(1));
        assert!(s.insert(2));
        assert!(s.insert(3));
        assert!(s.insert(4)); // 1 evicted
        assert!(!s.contains(1));
        assert!(s.contains(2));
        assert!(s.contains(3));
        assert!(s.contains(4));
    }

    #[test]
    fn bounded_set_dedups_existing() {
        let mut s = BoundedSet::new(3);
        assert!(s.insert(7));
        assert!(!s.insert(7)); // already present
        assert_eq!(s.len(), 1);
    }

    #[test]
    fn cursor_refuses_oversized_window() {
        let err = MqttCursor::new(MAX_QOS2_WINDOW_SIZE + 1).unwrap_err();
        assert!(err.contains("MAX_QOS2_WINDOW_SIZE"));
    }

    #[test]
    fn cursor_observe_qos2_dedupes() {
        let mut c = MqttCursor::new(4).expect("ok");
        assert!(c.observe_qos2(10)); // new
        assert!(!c.observe_qos2(10)); // redelivery
        assert_eq!(c.last_packet_id, 10);
    }

    #[test]
    fn cursor_round_trip_serde() {
        let mut c = MqttCursor::new(8).expect("ok");
        c.observe_qos2(1);
        c.observe_qos2(2);
        c.observe_qos2(3);
        c.retained_topics.insert("sensor/temp".into(), 42);
        let bytes = c.to_bytes();
        let back = MqttCursor::from_bytes(&bytes).expect("ok");
        assert_eq!(back.last_packet_id, 3);
        assert!(back.qos2_inflight_window.contains(1));
        assert!(back.qos2_inflight_window.contains(2));
        assert!(back.qos2_inflight_window.contains(3));
        assert_eq!(back.retained_topics.get("sensor/temp"), Some(&42));
    }

    #[test]
    fn cursor_display_is_informative() {
        let mut c = MqttCursor::new(8).expect("ok");
        c.observe_qos2(5);
        let d = c.display();
        assert!(d.contains("last_id=5"));
        assert!(d.contains("dedup=1/8"));
    }
}
