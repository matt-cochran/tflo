//! Serializable checkpoint of a compiled graph's runtime state.
//!
//! [`GraphSnapshot`] is the on-the-wire form of a graph's per-node state. It is
//! produced by [`CompiledGraph::snapshot`](super::CompiledGraph::snapshot) and
//! consumed by [`CompiledGraph::restore`](super::CompiledGraph::restore),
//! encoded with `postcard` — a compact format that round-trips non-finite
//! `f64` values (which window and cumulative buffers legitimately hold),
//! unlike `serde_json` which maps them to `null`.
//!
//! Not every graph is checkpointable: `scan`/`scan2` nodes hold opaque closure
//! state, `fold` composition nodes hold opaque accumulator state, and a
//! [`CustomNode`](crate::custom_node::CustomNode) is checkpointable only when
//! it overrides [`save`](crate::custom_node::CustomNode::save).
//! [`NodeState::to_snapshot`] returns `None` for any node it cannot capture,
//! and `snapshot()` turns that into an error rather than writing a partial
//! checkpoint.

use serde::{Deserialize, Serialize};

use super::{NodeState, RsiWilderState};
use crate::primitives::{
    CorrelationCountWindow, CorrelationTimeWindow, CountEma, CountWindow, CrossDetector,
    CumulativeMax, CumulativeMin, CumulativeProduct, CumulativeSum, GlitchFilter,
    HysteresisCrossDetector, LagBuffer, MedianCountWindow, MedianTimeWindow, MomentsCountWindow,
    MomentsTimeWindow, PrevByTracker, PrevTracker, PulseWidthDetector, RsiCountWindow,
    RsiTimeWindow, RuntDetector, TimeEma, TimeWindow, WindowDetector, WmaCountWindow,
    WmaTimeWindow,
};

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
    /// Mirror of [`NodeState::TimeWindow`].
    TimeWindow(TimeWindow),
    /// Mirror of [`NodeState::CountWindow`].
    CountWindow(CountWindow),
    /// Mirror of [`NodeState::TimeEma`].
    TimeEma(TimeEma),
    /// Mirror of [`NodeState::CountEma`].
    CountEma(CountEma),
    /// Mirror of [`NodeState::Prev`].
    Prev(PrevTracker),
    /// Mirror of [`NodeState::PrevBy`].
    PrevBy(PrevByTracker<u64>),
    /// Mirror of [`NodeState::Lag`].
    Lag(LagBuffer),
    /// Mirror of [`NodeState::Cross`].
    Cross(CrossDetector),
    /// Mirror of [`NodeState::CrossHysteresis`].
    CrossHysteresis(HysteresisCrossDetector),
    /// Mirror of [`NodeState::GlitchFilterState`].
    GlitchFilter(GlitchFilter),
    /// Mirror of [`NodeState::RuntDetectorState`].
    RuntDetector(RuntDetector),
    /// Mirror of [`NodeState::PulseWidthState`].
    PulseWidth(PulseWidthDetector),
    /// Mirror of [`NodeState::WindowDetectorState`].
    WindowDetector(WindowDetector),
    /// Mirror of [`NodeState::Rate`].
    Rate {
        /// Previous sample timestamp.
        prev_ts: Option<i64>,
        /// Previous sample value.
        prev_value: Option<f64>,
    },
    /// Mirror of [`NodeState::Velocity`].
    Velocity {
        /// Previous sample timestamp.
        prev_ts: Option<i64>,
        /// Previous sample value.
        prev_value: Option<f64>,
    },
    /// Mirror of [`NodeState::Acceleration`] (the nested velocity tracker is
    /// flattened into the `vel_*` fields).
    Acceleration {
        /// Previous acceleration-sample timestamp.
        prev_ts: Option<i64>,
        /// Previous velocity value.
        prev_velocity: Option<f64>,
        /// Inner velocity tracker's previous timestamp.
        vel_prev_ts: Option<i64>,
        /// Inner velocity tracker's previous value.
        vel_prev_value: Option<f64>,
    },
    /// Mirror of [`NodeState::MedianTimeWindow`].
    MedianTimeWindow(MedianTimeWindow),
    /// Mirror of [`NodeState::MedianCountWindow`].
    MedianCountWindow(MedianCountWindow),
    /// Mirror of [`NodeState::CorrelationTimeWindow`].
    CorrelationTimeWindow(CorrelationTimeWindow),
    /// Mirror of [`NodeState::CorrelationCountWindow`].
    CorrelationCountWindow(CorrelationCountWindow),
    /// Mirror of [`NodeState::MomentsTimeWindow`].
    MomentsTimeWindow(MomentsTimeWindow),
    /// Mirror of [`NodeState::MomentsCountWindow`].
    MomentsCountWindow(MomentsCountWindow),
    /// Mirror of [`NodeState::WmaTimeWindow`].
    WmaTimeWindow(WmaTimeWindow),
    /// Mirror of [`NodeState::WmaCountWindow`].
    WmaCountWindow(WmaCountWindow),
    /// Mirror of [`NodeState::RsiTimeWindow`].
    RsiTimeWindow(RsiTimeWindow),
    /// Mirror of [`NodeState::RsiCountWindow`].
    RsiCountWindow(RsiCountWindow),
    /// Mirror of [`NodeState::RsiWilderState`].
    RsiWilder(RsiWilderState),
    /// Mirror of [`NodeState::CumSum`].
    CumSum(CumulativeSum),
    /// Mirror of [`NodeState::CumMax`].
    CumMax(CumulativeMax),
    /// Mirror of [`NodeState::CumMin`].
    CumMin(CumulativeMin),
    /// Mirror of [`NodeState::CumProd`].
    CumProd(CumulativeProduct),
    /// Mirror of [`NodeState::PctChange`].
    PctChange {
        /// Previous value.
        prev: Option<f64>,
    },
    /// Mirror of [`NodeState::LogReturn`].
    LogReturn {
        /// Previous value.
        prev: Option<f64>,
    },
    /// Opaque bytes from a checkpointable custom node's
    /// [`save`](crate::custom_node::CustomNode::save).
    Custom(Vec<u8>),
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
            NodeState::Stateless => NodeStateSnapshot::Stateless,
            NodeState::TimeWindow(w) => NodeStateSnapshot::TimeWindow(w.clone()),
            NodeState::CountWindow(w) => NodeStateSnapshot::CountWindow(w.clone()),
            NodeState::TimeEma(e) => NodeStateSnapshot::TimeEma(e.clone()),
            NodeState::CountEma(e) => NodeStateSnapshot::CountEma(e.clone()),
            NodeState::Prev(p) => NodeStateSnapshot::Prev(p.clone()),
            NodeState::PrevBy(p) => NodeStateSnapshot::PrevBy(p.clone()),
            NodeState::Lag(l) => NodeStateSnapshot::Lag(l.clone()),
            NodeState::Cross(c) => NodeStateSnapshot::Cross(c.clone()),
            NodeState::CrossHysteresis(h) => NodeStateSnapshot::CrossHysteresis(h.clone()),
            NodeState::GlitchFilterState(g) => NodeStateSnapshot::GlitchFilter(g.clone()),
            NodeState::RuntDetectorState(d) => NodeStateSnapshot::RuntDetector(d.clone()),
            NodeState::PulseWidthState(d) => NodeStateSnapshot::PulseWidth(d.clone()),
            NodeState::WindowDetectorState(d) => NodeStateSnapshot::WindowDetector(d.clone()),
            NodeState::Rate {
                prev_ts,
                prev_value,
            } => NodeStateSnapshot::Rate {
                prev_ts: *prev_ts,
                prev_value: *prev_value,
            },
            NodeState::Velocity {
                prev_ts,
                prev_value,
            } => NodeStateSnapshot::Velocity {
                prev_ts: *prev_ts,
                prev_value: *prev_value,
            },
            NodeState::Acceleration {
                prev_ts,
                prev_velocity,
                velocity_state,
            } => {
                let (vel_prev_ts, vel_prev_value) = match velocity_state.as_ref() {
                    NodeState::Velocity {
                        prev_ts,
                        prev_value,
                    } => (*prev_ts, *prev_value),
                    _ => (None, None),
                };
                NodeStateSnapshot::Acceleration {
                    prev_ts: *prev_ts,
                    prev_velocity: *prev_velocity,
                    vel_prev_ts,
                    vel_prev_value,
                }
            }
            NodeState::MedianTimeWindow(w) => NodeStateSnapshot::MedianTimeWindow(w.clone()),
            NodeState::MedianCountWindow(w) => NodeStateSnapshot::MedianCountWindow(w.clone()),
            NodeState::CorrelationTimeWindow(w) => {
                NodeStateSnapshot::CorrelationTimeWindow(w.clone())
            }
            NodeState::CorrelationCountWindow(w) => {
                NodeStateSnapshot::CorrelationCountWindow(w.clone())
            }
            NodeState::MomentsTimeWindow(w) => NodeStateSnapshot::MomentsTimeWindow(w.clone()),
            NodeState::MomentsCountWindow(w) => NodeStateSnapshot::MomentsCountWindow(w.clone()),
            NodeState::WmaTimeWindow(w) => NodeStateSnapshot::WmaTimeWindow(w.clone()),
            NodeState::WmaCountWindow(w) => NodeStateSnapshot::WmaCountWindow(w.clone()),
            NodeState::RsiTimeWindow(w) => NodeStateSnapshot::RsiTimeWindow(w.clone()),
            NodeState::RsiCountWindow(w) => NodeStateSnapshot::RsiCountWindow(w.clone()),
            NodeState::RsiWilderState(s) => NodeStateSnapshot::RsiWilder(s.clone()),
            NodeState::CumSum(c) => NodeStateSnapshot::CumSum(c.clone()),
            NodeState::CumMax(c) => NodeStateSnapshot::CumMax(c.clone()),
            NodeState::CumMin(c) => NodeStateSnapshot::CumMin(c.clone()),
            NodeState::CumProd(c) => NodeStateSnapshot::CumProd(c.clone()),
            NodeState::PctChange { prev } => NodeStateSnapshot::PctChange { prev: *prev },
            NodeState::LogReturn { prev } => NodeStateSnapshot::LogReturn { prev: *prev },
            // A custom node is checkpointable only if it overrides `save()`.
            NodeState::Custom(n) => return n.save().map(NodeStateSnapshot::Custom),
            // `scan`/`scan2` hold opaque closure state — not checkpointable.
            NodeState::ScanState(_) | NodeState::Scan2State(_) => return None,
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
            (NodeStateSnapshot::Stateless, NodeState::Stateless) => {}
            (NodeStateSnapshot::TimeWindow(v), NodeState::TimeWindow(slot)) => *slot = v,
            (NodeStateSnapshot::CountWindow(v), NodeState::CountWindow(slot)) => *slot = v,
            (NodeStateSnapshot::TimeEma(v), NodeState::TimeEma(slot)) => *slot = v,
            (NodeStateSnapshot::CountEma(v), NodeState::CountEma(slot)) => *slot = v,
            (NodeStateSnapshot::Prev(v), NodeState::Prev(slot)) => *slot = v,
            (NodeStateSnapshot::PrevBy(v), NodeState::PrevBy(slot)) => *slot = v,
            (NodeStateSnapshot::Lag(v), NodeState::Lag(slot)) => *slot = v,
            (NodeStateSnapshot::Cross(v), NodeState::Cross(slot)) => *slot = v,
            (NodeStateSnapshot::CrossHysteresis(v), NodeState::CrossHysteresis(slot)) => {
                *slot = v;
            }
            (NodeStateSnapshot::GlitchFilter(v), NodeState::GlitchFilterState(slot)) => {
                *slot = v;
            }
            (NodeStateSnapshot::RuntDetector(v), NodeState::RuntDetectorState(slot)) => {
                *slot = v;
            }
            (NodeStateSnapshot::PulseWidth(v), NodeState::PulseWidthState(slot)) => *slot = v,
            (NodeStateSnapshot::WindowDetector(v), NodeState::WindowDetectorState(slot)) => {
                *slot = v;
            }
            (
                NodeStateSnapshot::Rate {
                    prev_ts,
                    prev_value,
                },
                NodeState::Rate {
                    prev_ts: pt,
                    prev_value: pv,
                },
            ) => {
                *pt = prev_ts;
                *pv = prev_value;
            }
            (
                NodeStateSnapshot::Velocity {
                    prev_ts,
                    prev_value,
                },
                NodeState::Velocity {
                    prev_ts: pt,
                    prev_value: pv,
                },
            ) => {
                *pt = prev_ts;
                *pv = prev_value;
            }
            (
                NodeStateSnapshot::Acceleration {
                    prev_ts,
                    prev_velocity,
                    vel_prev_ts,
                    vel_prev_value,
                },
                NodeState::Acceleration {
                    prev_ts: pt,
                    prev_velocity: pv,
                    velocity_state,
                },
            ) => {
                *pt = prev_ts;
                *pv = prev_velocity;
                *velocity_state = Box::new(NodeState::Velocity {
                    prev_ts: vel_prev_ts,
                    prev_value: vel_prev_value,
                });
            }
            (NodeStateSnapshot::MedianTimeWindow(v), NodeState::MedianTimeWindow(slot)) => {
                *slot = v;
            }
            (NodeStateSnapshot::MedianCountWindow(v), NodeState::MedianCountWindow(slot)) => {
                *slot = v;
            }
            (
                NodeStateSnapshot::CorrelationTimeWindow(v),
                NodeState::CorrelationTimeWindow(slot),
            ) => *slot = v,
            (
                NodeStateSnapshot::CorrelationCountWindow(v),
                NodeState::CorrelationCountWindow(slot),
            ) => *slot = v,
            (NodeStateSnapshot::MomentsTimeWindow(v), NodeState::MomentsTimeWindow(slot)) => {
                *slot = v;
            }
            (NodeStateSnapshot::MomentsCountWindow(v), NodeState::MomentsCountWindow(slot)) => {
                *slot = v;
            }
            (NodeStateSnapshot::WmaTimeWindow(v), NodeState::WmaTimeWindow(slot)) => *slot = v,
            (NodeStateSnapshot::WmaCountWindow(v), NodeState::WmaCountWindow(slot)) => *slot = v,
            (NodeStateSnapshot::RsiTimeWindow(v), NodeState::RsiTimeWindow(slot)) => *slot = v,
            (NodeStateSnapshot::RsiCountWindow(v), NodeState::RsiCountWindow(slot)) => *slot = v,
            (NodeStateSnapshot::RsiWilder(v), NodeState::RsiWilderState(slot)) => *slot = v,
            (NodeStateSnapshot::CumSum(v), NodeState::CumSum(slot)) => *slot = v,
            (NodeStateSnapshot::CumMax(v), NodeState::CumMax(slot)) => *slot = v,
            (NodeStateSnapshot::CumMin(v), NodeState::CumMin(slot)) => *slot = v,
            (NodeStateSnapshot::CumProd(v), NodeState::CumProd(slot)) => *slot = v,
            (NodeStateSnapshot::PctChange { prev }, NodeState::PctChange { prev: slot }) => {
                *slot = prev;
            }
            (NodeStateSnapshot::LogReturn { prev }, NodeState::LogReturn { prev: slot }) => {
                *slot = prev;
            }
            (NodeStateSnapshot::Custom(bytes), NodeState::Custom(n)) => {
                n.load(&bytes)
                    .map_err(|_| SnapshotError::Decode { index })?;
            }
            // Snapshot variant did not line up with the live node.
            _ => return Err(SnapshotError::VariantMismatch { index }),
        }
        Ok(())
    }
}
