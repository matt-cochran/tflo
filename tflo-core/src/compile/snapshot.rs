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

/// Why a restored snapshot could not be applied to a live graph.
///
/// The *non-checkpointable node* case (a `scan`/`scan2` node, or a custom node
/// without `save()`) is detected on the snapshot side via
/// [`NodeState::to_snapshot`] returning `None`, so it never reaches this type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum SnapshotError {
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
}

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
    /// Returns `None` if the node is not checkpointable: a `scan`/`scan2`
    /// node, or a custom node whose `save()` returns `None`.
    pub(crate) fn to_snapshot(&self) -> Option<NodeStateSnapshot> {
        Some(match self {
            Self::Stateless => NodeStateSnapshot::Stateless,
            // A plugin operator is checkpointable only if it overrides `save()`.
            Self::Plugin(op) => return op.save().map(NodeStateSnapshot::Plugin),
            // `scan`/`scan2` hold opaque closure state — not checkpointable.
            Self::ScanState(_) | Self::Scan2State(_) => return None,
        })
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
