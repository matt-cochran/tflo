//! Serializable checkpoint of a compiled graph's runtime state.
//!
//! [`GraphSnapshot`] is the on-the-wire form of a graph's per-node state. It is
//! produced by [`CompiledGraph::snapshot`](super::CompiledGraph::snapshot) and
//! consumed by [`CompiledGraph::restore`](super::CompiledGraph::restore),
//! encoded with `postcard`.
//!
//! Not every graph is checkpointable: `scan`/`scan2` nodes hold opaque closure
//! state, `fold` composition nodes hold opaque accumulator state, and an
//! [`Operator`](crate::operator::Operator) plugin node is checkpointable only
//! when it overrides [`save`](crate::operator::Operator::save).
//! [`NodeState::to_snapshot`] returns `None` for any node it cannot capture,
//! and `snapshot()` turns that into an error rather than writing a partial
//! checkpoint.

use serde::{Deserialize, Serialize};

use super::NodeState;

/// Why a snapshot could not be produced from or applied to a graph.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SnapshotError {
    /// A restored node-state variant does not match the live graph's node at
    /// that position: the graph topology has changed since the snapshot.
    VariantMismatch {
        /// Position of the mismatched node.
        index: usize,
    },
    /// A custom node rejected its checkpoint bytes.
    Decode {
        /// Position of the offending node.
        index: usize,
    },
    /// A node in the graph is not snapshottable. Produced eagerly on the
    /// snapshot side rather than written as a partial checkpoint.
    ///
    /// Today this fires for `scan`/`scan2` nodes (opaque closure state) and
    /// for [`Operator`](crate::operator::Operator) plugins that do not
    /// override [`save`](crate::operator::Operator::save). Pre Phase 1 this
    /// was reported as a silent `None`; the typed variant is the
    /// hardening-pass fix.
    Unsupported {
        /// Position of the offending node.
        index: usize,
        /// A short name for the node kind, for diagnostics.
        kind: &'static str,
    },
}

impl std::fmt::Display for SnapshotError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::VariantMismatch { index } => write!(
                f,
                "snapshot variant does not match live node at index {index} \
                 (graph topology changed since snapshot)"
            ),
            Self::Decode { index } => {
                write!(f, "custom node at index {index} rejected its checkpoint bytes")
            }
            Self::Unsupported { index, kind } => write!(
                f,
                "node at index {index} ({kind}) does not support checkpointing"
            ),
        }
    }
}

impl std::error::Error for SnapshotError {}

/// Serializable mirror of a single node's [`NodeState`].
///
/// There is one variant per *checkpointable* `NodeState` variant; `scan` and
/// `scan2` have no mirror because their state is an opaque closure type.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) enum NodeStateSnapshot {
    /// Mirror of [`NodeState::Stateless`].
    Stateless,
    /// Opaque bytes from a checkpointable plugin operator's
    /// [`save`](crate::operator::Operator::save).
    Plugin(Vec<u8>),
}

/// Serializable checkpoint of an entire compiled graph.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct GraphSnapshot {
    /// Records processed at snapshot time.
    pub records_seen: usize,
    /// Configured minimum warmup.
    pub min_warmup: usize,
    /// Number of base nodes — checked against the restoring graph.
    pub node_count: usize,
    /// Number of output IDs — checked against the restoring graph.
    pub output_count: usize,
    /// Per-node state, in node order.
    pub node_states: Vec<NodeStateSnapshot>,
}

impl NodeState {
    /// Capture this node's state as a serializable [`NodeStateSnapshot`].
    ///
    /// Returns a typed [`SnapshotError::Unsupported`] when the node is not
    /// snapshottable (a `scan`/`scan2` node, or a plugin whose
    /// [`save`](crate::operator::Operator::save) returns `None`). `index`
    /// is the node's position in the graph and is used only for error
    /// reporting.
    pub(crate) fn to_snapshot(&self, index: usize) -> Result<NodeStateSnapshot, SnapshotError> {
        match self {
            Self::Stateless => Ok(NodeStateSnapshot::Stateless),
            // A plugin operator is checkpointable only if it overrides `save()`.
            Self::Plugin(op) => op.save().map(NodeStateSnapshot::Plugin).ok_or(
                SnapshotError::Unsupported {
                    index,
                    kind: "plugin operator without save() override",
                },
            ),
            // `scan`/`scan2` hold opaque closure state — not checkpointable.
            Self::ScanState(_) => Err(SnapshotError::Unsupported {
                index,
                kind: "scan node",
            }),
            Self::Scan2State(_) => Err(SnapshotError::Unsupported {
                index,
                kind: "scan2 node",
            }),
        }
    }
}

impl NodeStateSnapshot {
    /// Apply this restored snapshot onto a live [`NodeState`].
    ///
    /// `index` is the node's position in the graph, used only for error
    /// reporting. Returns [`SnapshotError::VariantMismatch`] if the snapshot's
    /// variant does not match the live node — i.e. the topology changed.
    pub(crate) fn apply_to(self, state: &mut NodeState, index: usize) -> Result<(), SnapshotError> {
        match (self, &mut *state) {
            (Self::Stateless, NodeState::Stateless) => {}
            (Self::Plugin(bytes), NodeState::Plugin(op)) => {
                op.load(&bytes)
                    .map_err(|_| SnapshotError::Decode { index })?;
            }
            // Snapshot variant did not line up with the live node.
            _ => return Err(SnapshotError::VariantMismatch { index }),
        }
        Ok(())
    }
}
