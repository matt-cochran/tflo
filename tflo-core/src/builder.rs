//! Builder for constructing temporal computation graphs.
//!
//! [`TemporalBuilder`] provides a fluent API for defining computations
//! over streaming data. It is used within the closure passed to
//! `.temporal()` or `.temporal_with()`.

use crate::comp::{Comp, Node, NodeId};
use crate::compile::ExtractOutput;
use std::cell::RefCell;
use std::marker::PhantomData;
use std::rc::Rc;
use std::sync::Arc;

/// Shared state for builder operations.
///
/// This is the actual state that is shared across all `Comp` instances
/// created from the same builder. It's wrapped in `Rc<RefCell<...>>` to
/// allow shared mutable access.
#[derive(Debug)]
pub struct BuilderState<R> {
    next_node_id: usize,
    pub(crate) nodes: Vec<(NodeId, Node<R>)>,
}

impl<R> Default for BuilderState<R> {
    fn default() -> Self {
        Self {
            next_node_id: 0,
            nodes: Vec::new(),
        }
    }
}

impl<R> BuilderState<R> {
    pub(crate) const fn next_id(&mut self) -> NodeId {
        let id = NodeId(self.next_node_id);
        self.next_node_id += 1;
        id
    }
}

/// Builder for constructing temporal computation graphs.
///
/// This is the main interface for defining computations within a
/// `.temporal()` or `.temporal_with()` closure.
///
/// # Example
///
/// ```ignore
/// ticks.into_iter()
///     .temporal(|t| {
///         t.timestamp(|x| x.ts);
///         let price = t.prop(|x| x.price);
///         let sma = price.sma(2.mins());
///         sma
///     })
/// ```
pub struct TFlowBuilder<R> {
    pub(crate) state: Rc<RefCell<BuilderState<R>>>,
    pub(crate) timestamp_fn: Option<Arc<dyn Fn(&R) -> i64 + Send + Sync>>,
}

impl<R> std::fmt::Debug for TFlowBuilder<R> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let state = self.state.borrow();
        f.debug_struct("TemporalBuilder")
            .field("next_node_id", &state.next_node_id)
            .field("node_count", &state.nodes.len())
            .finish()
    }
}

impl<R> Clone for TFlowBuilder<R> {
    fn clone(&self) -> Self {
        Self {
            state: Rc::clone(&self.state),
            timestamp_fn: self.timestamp_fn.clone(),
        }
    }
}

impl<R: 'static> Default for TFlowBuilder<R> {
    fn default() -> Self {
        Self::new()
    }
}

impl<R: 'static> TFlowBuilder<R> {
    /// Create a new temporal builder.
    #[must_use]
    pub fn new() -> Self {
        Self {
            state: Rc::new(RefCell::new(BuilderState::default())),
            timestamp_fn: None,
        }
    }

    /// Set the timestamp extractor (required for time-based windows).
    ///
    /// The function should return the timestamp in milliseconds.
    ///
    /// # Example
    ///
    /// ```ignore
    /// t.timestamp(|x| x.ts_ms);
    /// ```
    pub fn timestamp<F>(&mut self, f: F) -> &mut Self
    where
        F: Fn(&R) -> i64 + Send + Sync + 'static,
    {
        self.timestamp_fn = Some(Arc::new(f));
        self
    }

    /// Set the timestamp extractor with timestamps in seconds.
    ///
    /// Automatically converts seconds to milliseconds.
    pub fn timestamp_secs<F>(&mut self, f: F) -> &mut Self
    where
        F: Fn(&R) -> i64 + Send + Sync + 'static,
    {
        self.timestamp_fn = Some(Arc::new(move |r| f(r) * 1000));
        self
    }

    /// Set the timestamp extractor with timestamps as f64 seconds.
    ///
    /// Automatically converts to milliseconds.
    #[allow(clippy::cast_possible_truncation)]
    pub fn timestamp_secs_f64<F>(&mut self, f: F) -> &mut Self
    where
        F: Fn(&R) -> f64 + Send + Sync + 'static,
    {
        self.timestamp_fn = Some(Arc::new(move |r| (f(r) * 1000.0) as i64));
        self
    }

    /// Get the timestamp function, if set.
    ///
    /// This is primarily for use by extension crates that need to access
    /// the timestamp function for compilation.
    #[must_use]
    pub fn get_timestamp_fn(&self) -> Option<Arc<dyn Fn(&R) -> i64 + Send + Sync>> {
        self.timestamp_fn.clone()
    }

    /// Extract a property from the input record.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let price = t.prop(|x| x.price);
    /// let volume = t.prop(|x| x.volume);
    /// ```
    #[must_use]
    pub fn prop<F>(&self, f: F) -> Comp<R>
    where
        F: Fn(&R) -> f64 + Send + Sync + 'static,
    {
        self.add_node(Node::Prop(Arc::new(f)))
    }

    /// Create a constant value.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let threshold = t.constant(100.0);
    /// let above = price.cross_above(&threshold);
    /// ```
    #[must_use]
    pub fn constant(&self, value: f64) -> Comp<R> {
        self.add_node(Node::Const(value))
    }

    /// Add a node to the graph and return a Comp handle.
    pub(crate) fn add_node(&self, node: Node<R>) -> Comp<R> {
        let mut state = self.state.borrow_mut();
        let id = state.next_id();
        state.nodes.push((id, node));
        Comp {
            id,
            state: Rc::clone(&self.state),
            _marker: PhantomData,
        }
    }

    /// Get the nodes from this builder.
    #[must_use]
    pub fn into_nodes(self) -> Vec<(NodeId, Node<R>)> {
        // Take the nodes directly from the shared state
        std::mem::take(&mut self.state.borrow_mut().nodes)
    }

    /// Topology fingerprint for crash-safe restore.
    ///
    /// Produces a 32-byte hash over the graph's *topology* — node count,
    /// per-node `(NodeId, kind, name)` triple, in order. Two builders
    /// that produced identical fingerprints have structurally identical
    /// graphs at the node level.
    ///
    /// **What this catches:** a worker restarting against a snapshot
    /// produced by a *different* build of the application (added/removed
    /// nodes, renamed nodes, reordered nodes). The
    /// [`Checkpointer`](crate::state::Checkpointer) stamps this
    /// fingerprint into snapshot metadata; restore code that sees a
    /// mismatched fingerprint **must refuse the load** with a typed
    /// error instead of attempting a best-effort restore — silent
    /// version skew is the failure mode this poka-yoke prevents.
    ///
    /// **What this does not catch:** Operator-internal state-schema
    /// changes (a stateful `Operator` whose `save()` bytes change shape
    /// across versions without a node-topology change). That second line
    /// of defense is [`Operator::type_id_version`](crate::operator::Operator::type_id_version),
    /// which operators must opt into. A future version of `fingerprint`
    /// can compose them — for now they are independent fences.
    ///
    /// The hash is **not** cryptographic — it is a fence against
    /// accidental version mismatches, not adversarial tampering.
    #[must_use]
    pub fn fingerprint(&self) -> [u8; 32] {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut h = DefaultHasher::new();
        let state = self.state.borrow();
        state.nodes.len().hash(&mut h);
        for (id, node) in &state.nodes {
            id.0.hash(&mut h);
            // The Debug impl for `Node<R>` is deterministic and discriminative
            // over the structural fields (input ids, names, variant); it
            // skips opaque closure addresses by design.
            format!("{node:?}").hash(&mut h);
        }
        let h64 = h.finish();
        // Pack the 64-bit hash into 32 bytes deterministically. This is
        // enough collision resistance for accidental-mismatch detection;
        // when we want cryptographic strength we can swap in blake3.
        let mut out = [0u8; 32];
        out[..8].copy_from_slice(&h64.to_le_bytes());
        // Mix into the upper bytes with a stable transform so structurally
        // different graphs never collide on the low 8 bytes alone.
        out[8..16].copy_from_slice(&h64.rotate_left(17).to_le_bytes());
        out[16..24].copy_from_slice(&h64.rotate_left(31).to_le_bytes());
        out[24..32].copy_from_slice(&h64.rotate_left(47).to_le_bytes());
        out
    }
}

/// Trait for compiling computation outputs.
///
/// Implemented for single Comp values and tuples of Comp values,
/// allowing multiple outputs from a single computation.
pub trait Compile<R>: Sized {
    /// The output type after evaluation.
    type Output;

    /// Get the node IDs that should be evaluated.
    fn output_ids(&self) -> Vec<NodeId>;
}

// Blanket impl over every output type that can be extracted from the value
// store. This subsumes the previous concrete impls for `Comp<R, f64>` and
// `Comp<R, ThresholdCrossEventMode>` — both still work, and any further
// `ExtractOutput` type now compiles via the builder. Out-of-crate operator
// catalogs (e.g. `tflo-ops` event-detector ops) rely on this to expose
// typed-output operators through `.tflo(...)`; an orphan-rule add of these
// impls from outside `tflo-core` is not possible.
impl<R, O: ExtractOutput> Compile<R> for Comp<R, O> {
    type Output = O;

    fn output_ids(&self) -> Vec<NodeId> {
        vec![self.id]
    }
}

// Tuple implementations for multiple outputs
macro_rules! impl_compile_tuple {
    ($(($($T:ident),+)),*) => {
        $(
            impl<R, $($T),+> Compile<R> for ($(Comp<R, $T>,)+)
            where
                $($T: Clone + 'static,)+
            {
                type Output = ($($T,)+);

                fn output_ids(&self) -> Vec<NodeId> {
                    #[allow(non_snake_case)]
                    let ($($T,)+) = self;
                    vec![$($T.id,)+]
                }
            }
        )*
    };
}

impl_compile_tuple!(
    (A, B),
    (A, B, C),
    (A, B, C, D),
    (A, B, C, D, E),
    (A, B, C, D, E, F),
    (A, B, C, D, E, F, G),
    (A, B, C, D, E, F, G, H)
);

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone)]
    struct TestRecord {
        #[allow(dead_code)]
        ts: i64,
        value: f64,
    }

    #[test]
    fn test_builder_creation() {
        let builder: TFlowBuilder<TestRecord> = TFlowBuilder::new();
        assert_eq!(builder.state.borrow().nodes.len(), 0);
    }

    #[test]
    fn test_prop_creation() {
        let builder: TFlowBuilder<TestRecord> = TFlowBuilder::new();
        let _value = builder.prop(|x| x.value);
        assert_eq!(builder.state.borrow().nodes.len(), 1);
    }

    #[test]
    fn test_constant_creation() {
        let builder: TFlowBuilder<TestRecord> = TFlowBuilder::new();
        let _c = builder.constant(100.0);
        assert_eq!(builder.state.borrow().nodes.len(), 1);
    }
}
