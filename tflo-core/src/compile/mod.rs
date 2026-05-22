//! Compilation of computation graphs into executable form.
//!
//! This module handles compiling the declarative computation graph
//! into stateful executors that can process streaming data.
//!
//! # Type-Safe Composition
//!
//! `CompiledGraph<R, O>` provides type-safe composition operators:
//!
//! - [`zip`](CompiledGraph::zip): Combine two graphs into `(A, B)`
//! - [`map`](CompiledGraph::map): Transform output type
//! - [`reduce`](CompiledGraph::reduce): Collapse tuple to single value
//! - [`fold`](CompiledGraph::fold): Stateful accumulation
//! - [`filter`](CompiledGraph::filter): Conditional output
//!
//! # Architecture
//!
//! Computed node outputs are held in a [`ValueStore`] as a typed [`NodeOutput`]
//! (`f64` inline, everything else boxed). The external API stays fully
//! type-safe through generics and the [`ExtractOutput`] trait.

mod absent;
mod eval;
mod extract;
mod inspect;
mod node;
mod pipeline;
mod snapshot;
#[cfg(test)]
mod tests;
mod value;

use crate::comp::NodeId;
use crate::operator::BoxedOperator;
use crate::pipeline::{PipelineContext, Timestamped};
pub use absent::{Absent, Computed, finite_or_warming};
pub use extract::ExtractOutput;
pub use inspect::{GraphPlan, GraphStateSummary};
pub use node::{CompiledNode, CompositionNodeEntry, offset_node_ids};
pub use pipeline::{PipelinedGraph, StepResult};
use std::any::Any;
use std::collections::HashMap;
use std::marker::PhantomData;
use std::sync::{Arc, Mutex};
pub use value::NodeOutput;

// ============================================================================
// VALUE STORE - Type-erased storage for computed values
// ============================================================================

/// Storage for computed node outputs, keyed by [`NodeId`].
///
/// Values are held as a typed [`NodeOutput`] — `f64` inline, everything else boxed.
#[derive(Default)]
pub struct ValueStore {
    pub(crate) values: HashMap<NodeId, NodeOutput>,
}

// ============================================================================
// EXTRACT OUTPUT - Trait for type-safe extraction
// ============================================================================

/// Macro to implement ExtractOutput for simple cloneable types.
/// All these types use the same extraction pattern: get_cloned from first ID.
macro_rules! impl_extract_output {
    ($($t:ty),+ $(,)?) => {
        $(
            impl ExtractOutput for $t {
                fn extract(store: &ValueStore, ids: &[NodeId]) -> Option<Self> {
                    store.get_cloned(ids.first()?)
                }
            }
        )+
    };
}

// Primitive types other than `f64` (`f64` has a bespoke impl below). These are
// only ever produced by `map`/`fold` composition nodes, so plain `get_cloned`
// on the boxed value is correct.
impl_extract_output!(
    f32, bool, i8, i16, i32, i64, i128, u8, u16, u32, u64, u128, usize, isize, String
);

/// `f64` extraction flattens the typed-absence model back to the historical
/// NaN sentinel: a present `Ok` yields the value, any [`Absent`] reason yields
/// `f64::NAN`. This keeps `O = f64` callers fully back-compatible. Callers who
/// want the typed reason should use `O = Computed` instead.
impl ExtractOutput for f64 {
    fn extract(store: &ValueStore, ids: &[NodeId]) -> Option<Self> {
        match store.values.get(ids.first()?)? {
            NodeOutput::Computed(c) => Some(c.unwrap_or(f64::NAN)),
            NodeOutput::Other(b) => b.downcast_ref::<f64>().copied(),
        }
    }

    fn as_f64(&self) -> Option<f64> {
        Some(*self)
    }
}

/// `Computed` extraction is the opt-in path that preserves the typed
/// [`Absent`] reason a node produced for an absent record.
impl ExtractOutput for Computed {
    fn extract(store: &ValueStore, ids: &[NodeId]) -> Option<Self> {
        match store.values.get(ids.first()?)? {
            NodeOutput::Computed(c) => Some(*c),
            NodeOutput::Other(b) => b.downcast_ref::<f64>().copied().map(Ok),
        }
    }
}

/// Blanket impl for Option<T> - handles both filtered and direct values
impl<T: ExtractOutput + Clone + 'static> ExtractOutput for Option<T> {
    fn extract(store: &ValueStore, ids: &[NodeId]) -> Option<Self> {
        let id = ids.first()?;
        // First try Option<T> (from filter/filter_map that store Option directly)
        if let Some(opt) = store.get_cloned::<Option<T>>(id) {
            return Some(opt);
        }
        // Fall back to T's own extraction (for optional outputs, missing = None).
        // Using `T::extract` rather than a raw `get_cloned` keeps the typed
        // absence model intact — e.g. `T = f64` flattens `Absent` to NaN.
        Some(T::extract(store, ids))
    }

    fn output_id_count() -> usize {
        T::output_id_count()
    }
}

/// All composition operations preserve the context type.
pub struct CompiledGraph<R, O, C: PipelineContext = Timestamped> {
    pub(crate) timestamp_fn: Arc<dyn Fn(&R) -> i64 + Send + Sync>,
    pub(crate) nodes: Vec<CompiledNode<R>>,
    /// Post-compilation nodes (map, fold, filter operations)
    pub(crate) composition_nodes: Vec<CompositionNodeEntry>,
    pub(crate) output_ids: Vec<NodeId>,
    pub(crate) store: ValueStore,
    pub(crate) records_seen: usize,
    pub(crate) min_warmup: usize,
    pub(crate) _phantom: PhantomData<(O, C)>,
}

/// A post-compilation composition node (`map` / `fold`).
pub(crate) enum CompositionNodeKind {
    Map {
        mapper: Arc<dyn Fn(&ValueStore) -> Option<NodeOutput> + Send + Sync>,
    },
    Fold {
        state: Arc<Mutex<Box<dyn Any + Send + Sync>>>,
        folder: Arc<
            dyn Fn(&ValueStore, &Mutex<Box<dyn Any + Send + Sync>>) -> Option<NodeOutput>
                + Send
                + Sync,
        >,
    },
}

/// State for a compiled node.
pub(crate) enum NodeState {
    /// No state needed.
    Stateless,
    /// State for scan_f64.
    ScanState(Box<dyn Any + Send + Sync>),
    /// State for scan2_f64.
    Scan2State(Box<dyn Any + Send + Sync>),
    /// State for a plugin node.
    Plugin(BoxedOperator),
}

/// The operation to perform for a node.
pub(crate) enum NodeOp<R> {
    /// Extract a property from the input record.
    Prop(Arc<dyn Fn(&R) -> f64 + Send + Sync>),
    /// Constant value.
    Const(f64),
    // Custom functional operators
    MapF64(NodeId, Arc<dyn Fn(f64) -> f64 + Send + Sync>),
    Map2F64(NodeId, NodeId, Arc<dyn Fn(f64, f64) -> f64 + Send + Sync>),
    FilterF64(NodeId, Arc<dyn Fn(f64) -> bool + Send + Sync>),
    FilterMapF64(NodeId, Arc<dyn Fn(f64) -> Option<f64> + Send + Sync>),
    ScanF64(
        NodeId,
        Arc<dyn Fn() -> Box<dyn Any + Send + Sync> + Send + Sync>,
        Arc<dyn Fn(&mut Box<dyn Any + Send + Sync>, f64) -> Computed + Send + Sync>,
    ),
    Scan2F64(
        NodeId,
        NodeId,
        Arc<dyn Fn() -> Box<dyn Any + Send + Sync> + Send + Sync>,
        Arc<dyn Fn(&mut Box<dyn Any + Send + Sync>, f64, f64) -> Computed + Send + Sync>,
    ),
    /// Plugin node: resolves `inputs` and delegates to an [`Operator`](crate::operator::Operator)
    /// held in `NodeState`.
    Plugin {
        /// Input node IDs, in declaration order.
        inputs: Vec<NodeId>,
    },
}

// =============================================================================
// COMPOSITION OPERATORS
// ============================================================================

impl<R, O, C> CompiledGraph<R, O, C>
where
    O: ExtractOutput,
    C: PipelineContext,
{
    /// Combine this graph with another, producing a tuple output.
    ///
    /// This is the primary composition operator for combining independent
    /// computations. Both graphs must have the same context type.
    ///
    /// All node IDs from `other` are offset by this graph's max ID so the
    /// two graphs' nodes never collide.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let combined = prices.zip(volumes);  // Graph<R, (f64, f64), C>
    /// ```
    #[must_use]
    pub fn zip<O2>(mut self, other: CompiledGraph<R, O2, C>) -> CompiledGraph<R, (O, O2), C>
    where
        O2: ExtractOutput,
    {
        let offset = self.max_node_id() + 1;

        // Offset all node IDs from other graph
        let other_nodes: Vec<CompiledNode<R>> = other
            .nodes
            .into_iter()
            .map(|mut n| {
                n.id = NodeId(n.id.0 + offset);
                n.offset_input_ids(offset);
                n
            })
            .collect();

        // Offset composition nodes
        let other_comp_nodes: Vec<CompositionNodeEntry> = other
            .composition_nodes
            .into_iter()
            .map(|mut e| {
                e.id = NodeId(e.id.0 + offset);
                e
            })
            .collect();

        // Extend our nodes
        self.nodes.extend(other_nodes);
        self.composition_nodes.extend(other_comp_nodes);

        // Combine output IDs: ours first, then offset others
        let mut combined_ids = self.output_ids;
        combined_ids.extend(offset_node_ids(&other.output_ids, offset));

        CompiledGraph {
            timestamp_fn: self.timestamp_fn,
            nodes: self.nodes,
            composition_nodes: self.composition_nodes,
            output_ids: combined_ids,
            store: ValueStore::new(),
            records_seen: 0,
            min_warmup: self.min_warmup.max(other.min_warmup),
            _phantom: PhantomData,
        }
    }

    /// Transform the output type, preserving context.
    ///
    /// This is a stateless transformation that applies a function to each
    /// output value. The pipeline context flows through unchanged.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let crosses = prices.map(|p| if p > 100.0 { ThresholdCrossEventMode::Rising } else { ThresholdCrossEventMode::Falling });
    /// // step() returns PipelineItem<C, ThresholdCrossEventMode> - context preserved
    /// ```
    #[must_use]
    pub fn map<O2, F>(mut self, f: F) -> CompiledGraph<R, O2, C>
    where
        O2: ExtractOutput,
        F: Fn(O) -> O2 + Send + Sync + 'static,
    {
        let new_id = NodeId(self.max_node_id() + 1);
        let input_ids = self.output_ids.clone();

        let mapper: Arc<dyn Fn(&ValueStore) -> Option<NodeOutput> + Send + Sync> =
            Arc::new(move |store| {
                let input = O::extract(store, &input_ids)?;
                let output = f(input);
                Some(NodeOutput::Other(Box::new(output)))
            });

        self.composition_nodes.push(CompositionNodeEntry {
            id: new_id,
            kind: CompositionNodeKind::Map { mapper },
        });

        CompiledGraph {
            timestamp_fn: self.timestamp_fn,
            nodes: self.nodes,
            composition_nodes: self.composition_nodes,
            output_ids: vec![new_id],
            store: ValueStore::new(),
            records_seen: 0,
            min_warmup: self.min_warmup,
            _phantom: PhantomData,
        }
    }

    /// Filter outputs based on a predicate, preserving context.
    ///
    /// Returns `Some(value)` when predicate is true, `None` otherwise.
    /// Context is always preserved.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let significant = prices.filter(|p| p.abs() > threshold);
    /// // step() returns PipelineItem<C, Option<f64>>
    /// ```
    #[must_use]
    pub fn filter<F>(self, predicate: F) -> CompiledGraph<R, Option<O>, C>
    where
        O: Clone + 'static,
        F: Fn(&O) -> bool + Send + Sync + 'static,
    {
        self.map(
            move |value| {
                if predicate(&value) { Some(value) } else { None }
            },
        )
    }

    /// Filter and transform in one operation.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let valid = results.filter_map(|r| r.ok());
    /// ```
    #[must_use]
    pub fn filter_map<O2, F>(self, f: F) -> CompiledGraph<R, Option<O2>, C>
    where
        O2: ExtractOutput + Clone + 'static,
        F: Fn(O) -> Option<O2> + Send + Sync + 'static,
    {
        self.map(f)
    }

    /// Fold with a stateful accumulator, preserving context.
    ///
    /// This provides stateful reduction across the stream.
    /// Each output carries the current record's context.
    ///
    /// The accumulator state is held behind a `Mutex` so the folder closure
    /// stays `Send + Sync`.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let running_total = values.fold(0.0, |acc, x| acc + x);
    /// // Each step returns PipelineItem<C, f64> with current timestamp
    /// ```
    #[must_use]
    pub fn fold<Acc, F>(mut self, initial: Acc, f: F) -> CompiledGraph<R, Acc, C>
    where
        Acc: ExtractOutput + Clone,
        F: Fn(Acc, O) -> Acc + Send + Sync + 'static,
    {
        let new_id = NodeId(self.max_node_id() + 1);
        let input_ids = self.output_ids.clone();
        let state: Arc<Mutex<Box<dyn Any + Send + Sync>>> = Arc::new(Mutex::new(Box::new(initial)));
        let state_clone = Arc::clone(&state);

        let folder: Arc<
            dyn Fn(&ValueStore, &Mutex<Box<dyn Any + Send + Sync>>) -> Option<NodeOutput>
                + Send
                + Sync,
        > = Arc::new(move |store, acc_mutex| {
            let input = O::extract(store, &input_ids)?;
            let mut guard = acc_mutex.lock().ok()?;
            let current = guard.downcast_ref::<Acc>()?.clone();
            let next = f(current, input);
            *guard = Box::new(next.clone());
            Some(NodeOutput::Other(Box::new(next)))
        });

        self.composition_nodes.push(CompositionNodeEntry {
            id: new_id,
            kind: CompositionNodeKind::Fold {
                state: state_clone,
                folder,
            },
        });

        CompiledGraph {
            timestamp_fn: self.timestamp_fn,
            nodes: self.nodes,
            composition_nodes: self.composition_nodes,
            output_ids: vec![new_id],
            store: ValueStore::new(),
            records_seen: 0,
            min_warmup: self.min_warmup,
            _phantom: PhantomData,
        }
    }

    /// Change the context type of this graph.
    ///
    /// This is useful for switching between time-based and sequence-based
    /// processing.
    ///
    /// # Example
    ///
    /// ```ignore
    /// // Switch from time-based to sequence-based
    /// let seq_graph = time_graph.with_context::<Sequenced>();
    /// ```
    #[must_use]
    pub fn with_context<C2: PipelineContext>(self) -> CompiledGraph<R, O, C2> {
        CompiledGraph {
            timestamp_fn: self.timestamp_fn,
            nodes: self.nodes,
            composition_nodes: self.composition_nodes,
            output_ids: self.output_ids,
            store: ValueStore::new(),
            records_seen: 0,
            min_warmup: self.min_warmup,
            _phantom: PhantomData,
        }
    }
}

// Reduce is only available on tuple outputs
impl<R, A, B, C> CompiledGraph<R, (A, B), C>
where
    A: ExtractOutput,
    B: ExtractOutput,
    C: PipelineContext,
{
    /// Collapse a tuple to a single value.
    ///
    /// This is the inverse of `zip` - it combines two values into one.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let ratio = prices.zip(volumes).reduce(|p, v| p / v);
    /// ```
    #[must_use]
    pub fn reduce<D, F>(self, f: F) -> CompiledGraph<R, D, C>
    where
        D: ExtractOutput,
        F: Fn(A, B) -> D + Send + Sync + 'static,
    {
        self.map(move |(a, b)| f(a, b))
    }
}

// 3-tuple reduce
impl<R, A, B, Ctx, C> CompiledGraph<R, (A, B, Ctx), C>
where
    A: ExtractOutput,
    B: ExtractOutput,
    Ctx: ExtractOutput,
    C: PipelineContext,
{
    /// Collapse a 3-tuple to a single value.
    #[must_use]
    pub fn reduce3<D, F>(self, f: F) -> CompiledGraph<R, D, C>
    where
        D: ExtractOutput,
        F: Fn(A, B, Ctx) -> D + Send + Sync + 'static,
    {
        self.map(move |(a, b, c)| f(a, b, c))
    }
}
