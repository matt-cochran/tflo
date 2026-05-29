//! Pluggable per-operator metrics surface.
//!
//! The engine emits coarse-grained metrics on the keyed-execution and
//! checkpoint paths via the [`Metrics`] trait. The default impl —
//! [`NoopMetrics`] — discards every value, so the trait carries zero
//! runtime cost when no impl is wired in. Users who want to ship
//! metrics to Prometheus / statsd / OpenTelemetry implement the trait
//! themselves; the engine takes no direct dependency on any concrete
//! metrics crate.
//!
//! The richer `tracing` integration lives behind the optional
//! `tracing` feature (`#[cfg(feature = "tracing")]` spans on the same
//! call sites). The two are independent: callers can wire metrics
//! without enabling `tracing`, and vice versa.

/// Pluggable metrics surface for the keyed-execution and checkpoint paths.
///
/// All methods are `&self` and have a no-op default impl. Concrete impls
/// should be cheap on the hot paths — e.g. atomic increments — so the
/// engine can call them on every keyed step without observable overhead.
pub trait Metrics: Send + Sync + 'static {
    /// Called once per keyed step. `key_bytes` is a best-effort
    /// rendering of the key for cardinality-controlled labels; do not
    /// rely on it being unique across keys for very large key spaces.
    fn on_keyed_step(&self, _key_bytes: &[u8]) {}

    /// Called once per event-time timer fire. `node_id` is the
    /// registering node's id.
    fn on_timer_fire(&self, _node_id: usize) {}

    /// Called once per successful checkpoint commit.
    fn on_checkpoint_commit(&self) {}

    /// Called once per failed checkpoint commit. `is_timeout` flags
    /// per-stage deadline exhaustion (the most common production
    /// failure mode).
    fn on_checkpoint_failure(&self, _is_timeout: bool) {}

    /// Called once per late record dropped by
    /// [`OutOfOrderPolicy::Buffer`](crate::keyed::OutOfOrderPolicy::Buffer)
    /// or [`OutOfOrderPolicy::Drop`](crate::keyed::OutOfOrderPolicy::Drop).
    fn on_late_record_dropped(&self) {}
}

/// No-op [`Metrics`] impl. Every method discards its arguments. Used
/// as the default when no concrete impl is wired in; the compiler
/// inlines the no-ops away so there is no runtime cost.
#[derive(Debug, Default, Clone, Copy)]
pub struct NoopMetrics;

impl Metrics for NoopMetrics {}

/// Boxed `dyn Metrics`. Allows mixed concrete impls behind a uniform
/// trait object — useful for runtime-configured production pipelines.
pub type BoxedMetrics = std::sync::Arc<dyn Metrics>;
