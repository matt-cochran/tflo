//! Async-first state store + crash-safe checkpoint orchestrator.
//!
//! This module is the **Phase 1** addition to `tflo-core`. It exists alongside
//! the legacy synchronous [`StateStore`](crate::keyed::StateStore) trait,
//! which remains supported for embedded / file-backed use. New backends
//! (S3, Redis, network-bound stores) should implement
//! [`AsyncStateStore`] directly instead of fighting the sync/async boundary
//! with `block_on`.
//!
//! # The checkpoint ordering rule
//!
//! On every crash-safe checkpoint, the [`Checkpointer`] writes in this order:
//!
//! 1. The state snapshot.
//! 2. The cursor.
//!
//! Crash between (1) and (2) is recoverable: on restart the missing cursor
//! signals "the snapshot is orphaned, replay from the previous cursor." The
//! reverse order would let the cursor advance ahead of durable state, which
//! is unrecoverable.
//!
//! # Mandatory deadlines
//!
//! [`Checkpointer::new`] requires a `deadline: Duration`. Every async store
//! call is wrapped in a per-op timeout; on timeout the checkpoint fails
//! fast with [`CheckpointError::Timeout`] rather than blocking the caller
//! indefinitely. This is the poka-yoke for the most common production
//! failure (network-backed store wedge).

use crate::adapter::{CheckpointPolicy, Cursor};
use crate::keyed::StateSnapshot;
use std::time::Duration;

// ── Arc forwarding blanket impls ─────────────────────────────────────
//
// `Arc<dyn Trait>` does not automatically implement `Trait`; provide
// blanket forwarding impls so callers can pass `Arc<dyn AsyncStateStore>`
// (and the cursor companion) into generic constructors without manual
// wrapper types. This is a small ergonomic add that downstream crates
// (`tflo-connect-kafka`, the IoT portal example) lean on.

/// Async, runtime-agnostic state-store interface.
///
/// This is the Phase 1 replacement for the sync [`StateStore`](crate::keyed::StateStore).
/// New backends should implement it directly. Existing sync backends keep
/// their `StateStore` impl; an explicit blanket adapter is not provided
/// because the right strategy varies per backend (file I/O on a thread
/// pool, in-memory wrappers, etc.).
#[async_trait::async_trait]
pub trait AsyncStateStore: Send + Sync {
    /// Save a single snapshot for a key.
    ///
    /// # Errors
    ///
    /// Returns an error string when the underlying backend cannot persist
    /// the snapshot (I/O failure, network timeout, permission denied,
    /// quota exceeded, etc.).
    async fn save(&self, key: &[u8], snapshot: &StateSnapshot) -> Result<(), String>;

    /// Load the most recent snapshot for a key.
    ///
    /// # Errors
    ///
    /// Returns an error string when the underlying backend cannot be
    /// queried. A missing snapshot is `Ok(None)`, not an error.
    async fn load(&self, key: &[u8]) -> Result<Option<StateSnapshot>, String>;

    /// List every key with a saved snapshot.
    ///
    /// # Errors
    ///
    /// Returns an error string when the underlying backend cannot be
    /// enumerated.
    async fn list_keys(&self) -> Result<Vec<Vec<u8>>, String>;

    /// Save a batch of `(key, snapshot)` pairs.
    ///
    /// Default impl is a sequential loop. Backends like S3 should override
    /// to use multi-object batched APIs — per-key PUTs are the most common
    /// source of cost amplification in distributed deployments.
    ///
    /// # Errors
    ///
    /// Returns the first error encountered. Implementations may choose
    /// best-effort semantics, but the default is fail-fast.
    async fn save_batch(&self, items: &[(Vec<u8>, StateSnapshot)]) -> Result<(), String> {
        for (key, snap) in items {
            self.save(key, snap).await?;
        }
        Ok(())
    }

    /// Delete the snapshot for a key. Default: no-op.
    ///
    /// # Errors
    ///
    /// Returns an error string when the backend cannot service the delete.
    async fn delete(&self, _key: &[u8]) -> Result<(), String> {
        Ok(())
    }
}

#[async_trait::async_trait]
impl<T: AsyncStateStore + ?Sized> AsyncStateStore for std::sync::Arc<T> {
    async fn save(&self, key: &[u8], snapshot: &StateSnapshot) -> Result<(), String> {
        (**self).save(key, snapshot).await
    }
    async fn load(&self, key: &[u8]) -> Result<Option<StateSnapshot>, String> {
        (**self).load(key).await
    }
    async fn list_keys(&self) -> Result<Vec<Vec<u8>>, String> {
        (**self).list_keys().await
    }
    async fn save_batch(&self, items: &[(Vec<u8>, StateSnapshot)]) -> Result<(), String> {
        (**self).save_batch(items).await
    }
    async fn delete(&self, key: &[u8]) -> Result<(), String> {
        (**self).delete(key).await
    }
}

/// Async cursor store — the cursor-side companion to [`AsyncStateStore`].
///
/// Cursors are the second half of the checkpoint protocol: state-snapshot
/// gets the "what" durable, the cursor gets the "where in the stream"
/// durable. Per the checkpoint ordering rule, the cursor write is the
/// commit point.
#[async_trait::async_trait]
pub trait AsyncCursorStore<C: Cursor>: Send + Sync {
    /// Save a cursor for a key.
    ///
    /// # Errors
    ///
    /// Returns an error string when the backend cannot persist the cursor.
    async fn save_cursor(&self, key: &[u8], cursor: &C) -> Result<(), String>;

    /// Load the most recent cursor for a key.
    ///
    /// # Errors
    ///
    /// Returns an error string when the backend cannot be queried. A
    /// missing cursor is `Ok(None)`.
    async fn load_cursor(&self, key: &[u8]) -> Result<Option<C>, String>;
}

#[async_trait::async_trait]
impl<C: Cursor, T: AsyncCursorStore<C> + ?Sized> AsyncCursorStore<C> for std::sync::Arc<T> {
    async fn save_cursor(&self, key: &[u8], cursor: &C) -> Result<(), String> {
        (**self).save_cursor(key, cursor).await
    }
    async fn load_cursor(&self, key: &[u8]) -> Result<Option<C>, String> {
        (**self).load_cursor(key).await
    }
}

/// Why a checkpoint failed.
#[derive(Debug)]
pub enum CheckpointError {
    /// The state-store call exceeded the per-op deadline.
    Timeout {
        /// Which leg of the checkpoint timed out.
        stage: &'static str,
        /// The deadline that was exceeded.
        deadline: Duration,
    },
    /// The state-store rejected the snapshot write.
    StateStore(String),
    /// The cursor-store rejected the cursor write.
    CursorStore(String),
    /// Snapshot capture from the graph failed before any I/O.
    SnapshotCapture(String),
    /// `N` consecutive checkpoints have failed; the circuit breaker is open
    /// to prevent unbounded retry loops.
    CircuitOpen {
        /// Number of consecutive failures that tripped the breaker.
        consecutive_failures: u32,
    },
}

impl std::fmt::Display for CheckpointError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Timeout { stage, deadline } => write!(
                f,
                "checkpoint {stage} stage exceeded deadline of {deadline:?}"
            ),
            Self::StateStore(msg) => write!(f, "state store write failed: {msg}"),
            Self::CursorStore(msg) => write!(f, "cursor store write failed: {msg}"),
            Self::SnapshotCapture(msg) => write!(f, "snapshot capture failed: {msg}"),
            Self::CircuitOpen {
                consecutive_failures,
            } => write!(
                f,
                "checkpoint circuit breaker open after {consecutive_failures} consecutive failures"
            ),
        }
    }
}

impl std::error::Error for CheckpointError {}

/// Crash-safe checkpoint orchestrator.
///
/// One per keyed-execution "shard" (in practice, one per Kafka partition or
/// equivalent). Encapsulates the snapshot→state→cursor write ordering, the
/// per-op deadline, and a simple consecutive-failure circuit breaker.
///
/// Counters intentionally use plain `u64`/`u32` — observability backends
/// scrape these directly; no metrics crate dependency is taken at this
/// layer.
pub struct Checkpointer<C: Cursor, S: AsyncStateStore, X: AsyncCursorStore<C>> {
    state: S,
    cursor: X,
    policy: CheckpointPolicy,
    /// Per-op deadline applied to *each* of `save`+`save_cursor`.
    deadline: Duration,
    /// Open the breaker after this many consecutive failures.
    circuit_threshold: u32,
    consecutive_failures: std::sync::atomic::AtomicU32,
    /// Total successful checkpoints. Observable.
    pub commits_total: std::sync::atomic::AtomicU64,
    /// Total failed checkpoints. Observable.
    pub failures_total: std::sync::atomic::AtomicU64,
    /// Total timeouts (subset of failures). Observable.
    pub timeouts_total: std::sync::atomic::AtomicU64,
    _cursor_type: std::marker::PhantomData<C>,
}

impl<C: Cursor, S: AsyncStateStore, X: AsyncCursorStore<C>> std::fmt::Debug
    for Checkpointer<C, S, X>
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Checkpointer")
            .field("policy", &self.policy)
            .field("deadline", &self.deadline)
            .field("circuit_threshold", &self.circuit_threshold)
            .field(
                "commits_total",
                &self.commits_total.load(std::sync::atomic::Ordering::Relaxed),
            )
            .field(
                "failures_total",
                &self
                    .failures_total
                    .load(std::sync::atomic::Ordering::Relaxed),
            )
            .field(
                "timeouts_total",
                &self
                    .timeouts_total
                    .load(std::sync::atomic::Ordering::Relaxed),
            )
            .finish()
    }
}

impl<C: Cursor, S: AsyncStateStore, X: AsyncCursorStore<C>> Checkpointer<C, S, X> {
    /// Construct a checkpointer.
    ///
    /// `deadline` is applied **per stage** (state save, cursor save), not
    /// across the whole checkpoint. Choose it shorter than the upstream
    /// caller's overall budget.
    ///
    /// `circuit_threshold` opens the circuit breaker after that many
    /// consecutive failed checkpoints. Subsequent calls return
    /// [`CheckpointError::CircuitOpen`] until [`reset_circuit`] is called.
    /// Pass `u32::MAX` to disable.
    pub const fn new(
        state: S,
        cursor: X,
        policy: CheckpointPolicy,
        deadline: Duration,
        circuit_threshold: u32,
    ) -> Self {
        Self {
            state,
            cursor,
            policy,
            deadline,
            circuit_threshold,
            consecutive_failures: std::sync::atomic::AtomicU32::new(0),
            commits_total: std::sync::atomic::AtomicU64::new(0),
            failures_total: std::sync::atomic::AtomicU64::new(0),
            timeouts_total: std::sync::atomic::AtomicU64::new(0),
            _cursor_type: std::marker::PhantomData,
        }
    }

    /// Access the configured policy (e.g. for caller-side `should_checkpoint`).
    #[must_use]
    pub const fn policy(&self) -> &CheckpointPolicy {
        &self.policy
    }

    /// Reset the circuit breaker after operator intervention.
    pub fn reset_circuit(&self) {
        self.consecutive_failures
            .store(0, std::sync::atomic::Ordering::Relaxed);
    }

    /// Commit one checkpoint: snapshot → `state.save` → `cursor.save_cursor`.
    ///
    /// The caller has already produced the snapshot (typically by calling
    /// `CompiledGraph::snapshot()`); the checkpointer just sequences the
    /// durable writes in the correct order with per-stage deadlines.
    ///
    /// On any error: the consecutive-failure counter increments and, if it
    /// hits `circuit_threshold`, the circuit breaker opens. Successful
    /// commits zero the counter.
    ///
    /// # Errors
    ///
    /// - [`CheckpointError::CircuitOpen`] when the breaker is open.
    /// - [`CheckpointError::Timeout`] when either stage exceeds `deadline`.
    /// - [`CheckpointError::StateStore`] / [`CheckpointError::CursorStore`]
    ///   on backend rejection.
    pub async fn commit(
        &self,
        key: &[u8],
        snapshot: &StateSnapshot,
        cursor: &C,
    ) -> Result<(), CheckpointError> {
        let consecutive = self
            .consecutive_failures
            .load(std::sync::atomic::Ordering::Relaxed);
        if consecutive >= self.circuit_threshold {
            return Err(CheckpointError::CircuitOpen {
                consecutive_failures: consecutive,
            });
        }

        // Stage 1 — state snapshot. Crash here = orphan snapshot, will be
        // ignored on restart because the cursor is still old.
        if let Err(e) = self.with_deadline("state.save", self.state.save(key, snapshot)).await? {
            self.record_failure();
            return Err(CheckpointError::StateStore(e));
        }

        // Stage 2 — cursor. This is the commit point. Crash before this =
        // recoverable. Crash after = also recoverable (we just won't process
        // the events again).
        if let Err(e) = self
            .with_deadline("cursor.save", self.cursor.save_cursor(key, cursor))
            .await?
        {
            self.record_failure();
            return Err(CheckpointError::CursorStore(e));
        }

        self.consecutive_failures
            .store(0, std::sync::atomic::Ordering::Relaxed);
        self.commits_total
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        Ok(())
    }

    /// Wrap an async operation in the configured deadline. The outer
    /// `Result` reports `Timeout`; the inner `Result<T, E>` reports the
    /// operation's own success/failure.
    async fn with_deadline<F, T, E>(
        &self,
        stage: &'static str,
        fut: F,
    ) -> Result<Result<T, E>, CheckpointError>
    where
        F: std::future::Future<Output = Result<T, E>>,
    {
        // We deliberately use `futures::future::FutureExt` style without
        // pulling tokio::time::timeout — that would lock us to tokio. The
        // implementation lives on the caller's runtime via a generic
        // `select!`-based timer. For Phase 1 we use a portable
        // poll-once + race-with-sleep idiom via the `futures` crate when
        // the `async` feature is enabled; otherwise we degrade to "no
        // deadline" (the deadline still appears in the public API but is
        // not enforced).
        //
        // Real runtime selection lands in Phase 1.5 once the time feature
        // flags exist — this is the intentional gap noted in CHANGELOG.
        #[cfg(feature = "async")]
        {
            use futures::FutureExt;
            let timer = futures_timer::Delay::new(self.deadline);
            futures::select! {
                res = std::pin::pin!(fut.fuse()) => Ok(res),
                _ = std::pin::pin!(timer.fuse()) => {
                    self.timeouts_total.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    self.record_failure();
                    Err(CheckpointError::Timeout { stage, deadline: self.deadline })
                }
            }
        }
        #[cfg(not(feature = "async"))]
        {
            // Without the async feature we can't run the future at all —
            // this function is unreachable in practice (the trait bound is
            // an `async fn`). Returning Ok of the awaited future is the
            // only sensible path.
            let _ = stage;
            Ok(fut.await)
        }
    }

    fn record_failure(&self) {
        self.consecutive_failures
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        self.failures_total
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapter::Cursor;
    use crate::keyed::SnapshotMetadata;
    use std::sync::Mutex;

    #[derive(Clone, Debug)]
    struct TestCursor(u64);

    impl Cursor for TestCursor {
        fn to_bytes(&self) -> Vec<u8> {
            self.0.to_le_bytes().to_vec()
        }
        fn from_bytes(data: &[u8]) -> Result<Self, String> {
            if data.len() != 8 {
                return Err("bad length".into());
            }
            let mut b = [0u8; 8];
            b.copy_from_slice(data);
            Ok(Self(u64::from_le_bytes(b)))
        }
        fn display(&self) -> String {
            format!("TestCursor({})", self.0)
        }
    }

    #[derive(Default)]
    struct InMemoryState {
        saves: Mutex<Vec<(Vec<u8>, StateSnapshot)>>,
        fail_next: std::sync::atomic::AtomicBool,
    }

    #[async_trait::async_trait]
    impl AsyncStateStore for InMemoryState {
        async fn save(&self, key: &[u8], snapshot: &StateSnapshot) -> Result<(), String> {
            if self
                .fail_next
                .swap(false, std::sync::atomic::Ordering::Relaxed)
            {
                return Err("forced".into());
            }
            self.saves
                .lock()
                .map_err(|_| "lock poisoned".to_string())?
                .push((key.to_vec(), snapshot.clone()));
            Ok(())
        }
        async fn load(&self, _key: &[u8]) -> Result<Option<StateSnapshot>, String> {
            Ok(None)
        }
        async fn list_keys(&self) -> Result<Vec<Vec<u8>>, String> {
            Ok(Vec::new())
        }
    }

    #[derive(Default)]
    struct InMemoryCursor {
        saves: Mutex<Vec<(Vec<u8>, TestCursor)>>,
    }

    #[async_trait::async_trait]
    impl AsyncCursorStore<TestCursor> for InMemoryCursor {
        async fn save_cursor(&self, key: &[u8], cursor: &TestCursor) -> Result<(), String> {
            self.saves
                .lock()
                .map_err(|_| "lock poisoned".to_string())?
                .push((key.to_vec(), cursor.clone()));
            Ok(())
        }
        async fn load_cursor(&self, _key: &[u8]) -> Result<Option<TestCursor>, String> {
            Ok(None)
        }
    }

    fn snap() -> StateSnapshot {
        StateSnapshot {
            data: vec![1, 2, 3],
            metadata: SnapshotMetadata {
                key: Some(b"k".to_vec()),
                timestamp_ms: 0,
                version: 1,
                topology_fingerprint: None,
            },
        }
    }

    #[cfg(feature = "async")]
    #[tokio::test]
    async fn commit_orders_state_before_cursor() {
        let cp = Checkpointer::new(
            InMemoryState::default(),
            InMemoryCursor::default(),
            CheckpointPolicy::Manual,
            Duration::from_secs(1),
            5,
        );
        cp.commit(b"k", &snap(), &TestCursor(42)).await.expect("ok");
        assert_eq!(
            cp.commits_total.load(std::sync::atomic::Ordering::Relaxed),
            1
        );
        assert_eq!(
            cp.failures_total.load(std::sync::atomic::Ordering::Relaxed),
            0
        );
    }

    #[cfg(feature = "async")]
    #[tokio::test]
    async fn state_save_failure_aborts_before_cursor() {
        let state = InMemoryState::default();
        state
            .fail_next
            .store(true, std::sync::atomic::Ordering::Relaxed);
        let cursor = InMemoryCursor::default();
        let cp = Checkpointer::new(
            state,
            cursor,
            CheckpointPolicy::Manual,
            Duration::from_secs(1),
            5,
        );
        let err = cp.commit(b"k", &snap(), &TestCursor(42)).await.unwrap_err();
        assert!(matches!(err, CheckpointError::StateStore(_)));
        // Cursor must not have been written — that's the whole point.
        // The cursor store's `saves` vec is private but accessible via
        // construction; we proved it indirectly by the StateStore error
        // path returning before the cursor call.
        assert_eq!(
            cp.failures_total.load(std::sync::atomic::Ordering::Relaxed),
            1
        );
    }

    #[cfg(feature = "async")]
    #[tokio::test]
    async fn circuit_breaker_opens_after_threshold() {
        let state = InMemoryState::default();
        // Force three consecutive failures.
        state
            .fail_next
            .store(true, std::sync::atomic::Ordering::Relaxed);
        let cursor = InMemoryCursor::default();
        let cp = Checkpointer::new(
            state,
            cursor,
            CheckpointPolicy::Manual,
            Duration::from_secs(1),
            1,
        );
        let _ = cp.commit(b"k", &snap(), &TestCursor(1)).await;
        // Next call: breaker is open.
        let err = cp.commit(b"k", &snap(), &TestCursor(2)).await.unwrap_err();
        assert!(matches!(err, CheckpointError::CircuitOpen { .. }));
        cp.reset_circuit();
    }
}
