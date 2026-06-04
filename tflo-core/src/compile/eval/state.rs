//! State & snapshot surface for [`CompiledGraph`] — accessors, warmup tracking,
//! and `snapshot` / `restore`.

use crate::compile::{CompiledGraph, CompositionNodeKind, GraphPlan, GraphStateSummary};
use crate::pipeline::PipelineContext;

impl<R, O, C: PipelineContext> CompiledGraph<R, O, C> {
    /// Get the total number of nodes in the graph.
    #[must_use]
    pub const fn node_count(&self) -> usize {
        // SAFETY: both `Vec::len()` values are bounded by the graph's
        // node count, which is itself bounded by available memory.
        // Summing them cannot overflow `usize` in any realizable graph.
        #[allow(clippy::arithmetic_side_effects)]
        {
            self.nodes.len() + self.composition_nodes.len()
        }
    }

    /// Get a debug representation of the graph structure.
    ///
    /// Returns information about the graph's nodes, dependencies, and configuration
    /// without exposing internal state. Useful for debugging and observability.
    #[must_use]
    pub fn graph_plan(&self) -> GraphPlan {
        GraphPlan {
            node_count: self.node_count(),
            base_node_count: self.nodes.len(),
            composition_node_count: self.composition_nodes.len(),
            output_count: self.output_ids.len(),
            records_seen: self.records_seen,
            min_warmup: self.min_warmup,
            warmup_remaining: self.min_warmup.saturating_sub(self.records_seen),
            context_type: std::any::type_name::<C>().to_string(),
        }
    }

    /// Get runtime state summary for observability.
    ///
    /// Returns a summary of the graph's runtime state including warmup status,
    /// node counts, and other metrics useful for monitoring and debugging.
    #[must_use]
    pub const fn state_summary(&self) -> GraphStateSummary {
        GraphStateSummary {
            records_seen: self.records_seen,
            min_warmup: self.min_warmup,
            warmup_remaining: self.min_warmup.saturating_sub(self.records_seen),
            is_warmed_up: self.records_seen >= self.min_warmup,
            node_count: self.node_count(),
            output_count: self.output_ids.len(),
        }
    }

    /// Get the maximum node ID in the graph (for offset calculations).
    pub(crate) fn max_node_id(&self) -> usize {
        let base_max = self.nodes.iter().map(|n| n.id.0).max().unwrap_or(0);
        let comp_max = self
            .composition_nodes
            .iter()
            .map(|n| n.id.0)
            .max()
            .unwrap_or(0);
        base_max.max(comp_max)
    }

    /// Get the number of records processed.
    #[must_use]
    pub const fn records_seen(&self) -> usize {
        self.records_seen
    }

    /// Check if the graph is warmed up (has seen minimum required records).
    #[must_use]
    pub const fn is_warmed_up(&self) -> bool {
        self.records_seen >= self.min_warmup
    }

    /// Set the minimum warmup period.
    pub const fn set_min_warmup(&mut self, min: usize) {
        self.min_warmup = min;
    }

    /// Take a snapshot of the current computation state for checkpointing.
    ///
    /// Returns a `StateSnapshot` — the full per-node state encoded with
    /// `postcard` — that can be persisted and later passed to
    /// [`restore()`](Self::restore).
    ///
    /// # Errors
    ///
    /// Returns [`ComputeError::InvalidInput`](crate::error::ComputeError::InvalidInput)
    /// if the graph contains state that cannot be captured: a `scan`/`scan2`
    /// node, a `fold` composition node, or an
    /// [`Operator`](crate::operator::Operator) plugin node that does not
    /// override [`save`](crate::operator::Operator::save). The snapshot is
    /// all-or-nothing — it never writes a partial checkpoint.
    ///
    /// # Usage
    ///
    /// ```ignore
    /// let snapshot = graph.snapshot()?;
    /// // persist `snapshot` somewhere
    /// // ... later ...
    /// let mut restored = CompiledGraph::compile(ts_fn, nodes, output_ids);
    /// restored.restore(&snapshot)?;
    /// ```
    pub fn snapshot(&self) -> Result<crate::keyed::StateSnapshot, crate::error::ComputeError> {
        use crate::compile::snapshot::{GraphSnapshot, SnapshotError};
        use crate::error::ComputeError;
        use crate::keyed::{SnapshotMetadata, StateSnapshot};

        // `fold` composition nodes hold opaque accumulator state; `map` nodes
        // are stateless and fine.
        if self
            .composition_nodes
            .iter()
            .any(|e| matches!(e.kind, CompositionNodeKind::Fold { .. }))
        {
            return Err(ComputeError::InvalidInput {
                reason: "graph has a fold composition node, which cannot be checkpointed",
            });
        }

        let mut node_states = Vec::with_capacity(self.nodes.len());
        for (index, node) in self.nodes.iter().enumerate() {
            // `to_snapshot` returns a typed `SnapshotError::Unsupported`
            // with the offending node index + kind; we keep the call-site
            // shape compatible by collapsing to the existing
            // `ComputeError::InvalidInput` variant, but the typed error is
            // already surfaced in the `reason` static string for
            // observability via metrics.
            match node.state.to_snapshot(index, node.op.scan_codec()) {
                Ok(s) => node_states.push(s),
                Err(SnapshotError::Unsupported { kind, .. }) => {
                    let reason: &'static str = match kind {
                        "scan node" => "graph has a non-checkpointable scan node",
                        "scan2 node" => "graph has a non-checkpointable scan2 node",
                        "scan node with unexpected accumulator type"
                        | "scan2 node with unexpected accumulator type" => {
                            "scan accumulator type did not match its codec"
                        }
                        "plugin operator without save() override" => {
                            "graph has a plugin operator that does not implement save()"
                        }
                        _ => "graph has a non-checkpointable node",
                    };
                    return Err(ComputeError::InvalidInput { reason });
                }
                Err(_) => {
                    return Err(ComputeError::InvalidInput {
                        reason: "snapshot of node failed",
                    });
                }
            }
        }

        let graph = GraphSnapshot {
            records_seen: self.records_seen,
            min_warmup: self.min_warmup,
            node_count: self.nodes.len(),
            output_count: self.output_ids.len(),
            node_states,
        };
        let data = postcard::to_stdvec(&graph).map_err(|e| ComputeError::Decode {
            context: "snapshot encoding failed",
            source: e.to_string(),
        })?;

        let timestamp_ms: i64 = {
            #[cfg(not(target_arch = "wasm32"))]
            {
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_millis() as i64
            }
            #[cfg(target_arch = "wasm32")]
            {
                // SystemTime::now() is unreliable on wasm; use 0 as placeholder
                0
            }
        };

        Ok(StateSnapshot {
            data,
            metadata: SnapshotMetadata {
                key: None,
                timestamp_ms,
                version: 1,
                topology_fingerprint: self.topology_fingerprint,
            },
        })
    }

    /// Restore computation state from a previously taken snapshot.
    ///
    /// The snapshot must have been taken from an identically structured
    /// compiled graph (same node topology, same output IDs). Every node's
    /// state — window buffers, accumulators, detector state machines — is
    /// restored.
    ///
    /// # Fingerprint check
    ///
    /// If both the snapshot and this graph carry a topology fingerprint
    /// (`SnapshotMetadata::topology_fingerprint` and
    /// `CompiledGraph::topology_fingerprint`, set via
    /// [`with_topology_fingerprint`](Self::with_topology_fingerprint)),
    /// they must match exactly. A mismatch returns
    /// [`ComputeError::InvalidInput`](crate::error::ComputeError::InvalidInput)
    /// with `reason = "topology fingerprint mismatch"`. This is the
    /// Phase 1 poka-yoke for silent version skew across workers.
    ///
    /// # Errors
    ///
    /// Returns [`ComputeError::InvalidInput`](crate::error::ComputeError::InvalidInput)
    /// if the snapshot bytes are malformed, the graph structure does not
    /// match the snapshot (different node count, output count, or a node
    /// whose kind differs), or the topology fingerprints disagree.
    pub fn restore(
        &mut self,
        snapshot: &crate::keyed::StateSnapshot,
    ) -> Result<(), crate::error::ComputeError> {
        use crate::compile::snapshot::GraphSnapshot;
        use crate::error::ComputeError;

        // Fingerprint check FIRST — cheap and the most informative failure
        // mode. Only enforced when both sides carry one (back-compat).
        if let (Some(expected), Some(actual)) = (
            self.topology_fingerprint,
            snapshot.metadata.topology_fingerprint,
        ) {
            if expected != actual {
                return Err(ComputeError::InvalidInput {
                    reason: "topology fingerprint mismatch: snapshot was produced by \
                             a structurally different graph",
                });
            }
        }

        let graph: GraphSnapshot =
            postcard::from_bytes(&snapshot.data).map_err(|e| ComputeError::Decode {
                context: "snapshot data invalid: expected GraphSnapshot format",
                source: e.to_string(),
            })?;

        // Verify graph structure compatibility.
        if graph.node_count != self.nodes.len() || graph.node_states.len() != self.nodes.len() {
            return Err(ComputeError::InvalidInput {
                reason: "snapshot node count mismatch: graph topology has changed",
            });
        }
        if graph.output_count != self.output_ids.len() {
            return Err(ComputeError::InvalidInput {
                reason: "snapshot output count mismatch: graph topology has changed",
            });
        }

        for (index, (node, snap)) in self.nodes.iter_mut().zip(graph.node_states).enumerate() {
            // Disjoint field borrows: the codec lives on `op` (immutable),
            // the live accumulator on `state` (mutable).
            let crate::compile::CompiledNode {
                op, state: ns, ..
            } = node;
            let codec = op.scan_codec();
            snap.apply_to(ns, index, codec)
                .map_err(|e| ComputeError::Decode {
                    context: "snapshot node-state mismatch: graph topology has changed",
                    source: e.to_string(),
                })?;
        }

        self.records_seen = graph.records_seen;
        self.min_warmup = graph.min_warmup;

        // Clear the value store to reset cached evaluations.
        self.store.clear();

        Ok(())
    }
}
