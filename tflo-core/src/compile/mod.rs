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
//! Computed node outputs are held in a [`ValueStore`] as a typed [`Value`]
//! (`f64` inline, everything else boxed). The external API stays fully
//! type-safe through generics and the [`ExtractOutput`] trait.

mod absent;
mod eval;
mod extract;
mod inspect;
mod node;
mod pipeline;
#[cfg(test)]
mod tests;
mod value;

use crate::comp::NodeId;
use crate::custom_node::BoxedCustomNode;
use crate::event::ThresholdCrossEventMode;
use crate::pipeline::{PipelineContext, Timestamped};
use crate::primitives::{
    CorrelationCountWindow, CorrelationTimeWindow, CountEma, CountWindow, CrossDetector,
    CumulativeMax, CumulativeMin, CumulativeProduct, CumulativeSum, GlitchFilter, GlitchResult,
    HysteresisCrossDetector, LagBuffer, MedianCountWindow, MedianTimeWindow, MomentsCountWindow,
    MomentsTimeWindow, PrevByTracker, PrevTracker, PulseWidthDetector, PulseWidthResult,
    RsiCountWindow, RsiTimeWindow, RuntDetector, RuntResult, TimeEma, TimeWindow, WindowDetector,
    WindowEvent, WmaCountWindow, WmaTimeWindow,
};
pub use absent::{Absent, Computed};
pub use extract::ExtractOutput;
pub use inspect::{GraphPlan, GraphStateSummary};
pub use node::{CompiledNode, CompositionNodeEntry, offset_node_ids};
pub use pipeline::{PipelinedGraph, StepResult};
pub(crate) use value::Value;
use std::any::Any;
use std::collections::HashMap;
use std::marker::PhantomData;
use std::sync::{Arc, Mutex};

// ============================================================================
// VALUE STORE - Type-erased storage for computed values
// ============================================================================

/// Storage for computed node outputs, keyed by [`NodeId`].
///
/// Values are held as a typed [`Value`] — `f64` inline, everything else boxed.
#[derive(Default)]
pub struct ValueStore {
    pub(crate) values: HashMap<NodeId, Value>,
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

// Primitive types
impl_extract_output!(
    f64, f32, bool, i8, i16, i32, i64, i128, u8, u16, u32, u64, u128, usize, isize, String
);

// Domain types
impl_extract_output!(
    ThresholdCrossEventMode,
    GlitchResult,
    RuntResult,
    PulseWidthResult,
    WindowEvent
);

/// Blanket impl for Option<T> - handles both filtered and direct values
impl<T: ExtractOutput + Clone + 'static> ExtractOutput for Option<T> {
    fn extract(store: &ValueStore, ids: &[NodeId]) -> Option<Self> {
        let id = ids.first()?;
        // First try Option<T> (from filter/filter_map that store Option directly)
        if let Some(opt) = store.get_cloned::<Option<T>>(id) {
            return Some(opt);
        }
        // Fall back to T directly (for optional outputs, missing = None)
        Some(store.get_cloned::<T>(id))
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
        mapper: Arc<dyn Fn(&ValueStore) -> Option<Value> + Send + Sync>,
    },
    Fold {
        state: Arc<Mutex<Box<dyn Any + Send + Sync>>>,
        folder: Arc<
            dyn Fn(&ValueStore, &Mutex<Box<dyn Any + Send + Sync>>) -> Option<Value>
                + Send
                + Sync,
        >,
    },
}

/// State for a compiled node.
pub(crate) enum NodeState {
    /// No state needed.
    Stateless,
    /// Time-based window.
    TimeWindow(TimeWindow),
    /// Count-based window.
    CountWindow(CountWindow),
    /// Time-based EMA.
    TimeEma(TimeEma),
    /// Count-based EMA.
    CountEma(CountEma),
    /// Previous value tracker.
    Prev(PrevTracker),
    /// Previous value by key.
    PrevBy(PrevByTracker<u64>),
    /// Lag buffer.
    Lag(LagBuffer),
    /// Cross detector.
    Cross(CrossDetector),
    /// Hysteresis cross detector.
    CrossHysteresis(HysteresisCrossDetector),
    /// Glitch filter.
    GlitchFilterState(GlitchFilter),
    /// Runt detector.
    RuntDetectorState(RuntDetector),
    /// Pulse width detector state.
    PulseWidthState(PulseWidthDetector),
    /// Window detector state.
    WindowDetectorState(WindowDetector),
    /// State for scan_f64.
    ScanState(Box<dyn Any + Send + Sync>),
    /// State for scan2_f64.
    Scan2State(Box<dyn Any + Send + Sync>),
    /// Rate tracker (stores previous timestamp and value).
    Rate {
        prev_ts: Option<i64>,
        prev_value: Option<f64>,
    },
    /// Velocity tracker (first derivative).
    Velocity {
        prev_ts: Option<i64>,
        prev_value: Option<f64>,
    },
    /// Acceleration tracker (second derivative).
    Acceleration {
        prev_ts: Option<i64>,
        prev_velocity: Option<f64>,
        velocity_state: Box<NodeState>,
    },
    /// Median/quantile time window.
    MedianTimeWindow(MedianTimeWindow),
    /// Median/quantile count window.
    MedianCountWindow(MedianCountWindow),
    /// Correlation time window (holds two series).
    CorrelationTimeWindow(CorrelationTimeWindow),
    /// Correlation count window (holds two series).
    CorrelationCountWindow(CorrelationCountWindow),
    /// Higher moments time window.
    MomentsTimeWindow(MomentsTimeWindow),
    /// Higher moments count window.
    MomentsCountWindow(MomentsCountWindow),
    /// WMA time window.
    WmaTimeWindow(WmaTimeWindow),
    /// WMA count window.
    WmaCountWindow(WmaCountWindow),
    /// RSI time window.
    RsiTimeWindow(RsiTimeWindow),
    /// RSI count window.
    RsiCountWindow(RsiCountWindow),
    /// RSI with Wilder smoothing.
    RsiWilderState(RsiWilderState),
    /// Cumulative sum.
    CumSum(CumulativeSum),
    /// Cumulative max.
    CumMax(CumulativeMax),
    /// Cumulative min.
    CumMin(CumulativeMin),
    /// Cumulative product.
    CumProd(CumulativeProduct),
    /// Percentage change tracker.
    PctChange { prev: Option<f64> },
    /// Log return tracker.
    LogReturn { prev: Option<f64> },
    /// State for a custom plugin node.
    Custom(BoxedCustomNode),
}

// ============================================================================
// RSI Wilder State Structure
// ============================================================================

/// State tracker for RSI with Wilder smoothing.
pub(crate) struct RsiWilderState {
    pub period: usize,
    pub prev: Option<f64>,
    pub count: usize,
    pub sum_gain: f64,
    pub sum_loss: f64,
    pub avg_gain: f64,
    pub avg_loss: f64,
    pub initialized: bool,
}

impl RsiWilderState {
    pub fn new(period: usize) -> Self {
        Self {
            period,
            prev: None,
            count: 0,
            sum_gain: 0.0,
            sum_loss: 0.0,
            avg_gain: 0.0,
            avg_loss: 0.0,
            initialized: false,
        }
    }
}

/// The operation to perform for a node.
pub(crate) enum NodeOp<R> {
    Prop(Arc<dyn Fn(&R) -> f64 + Send + Sync>),
    Const(f64),
    Sma(NodeId),
    Ema(NodeId),
    Std(NodeId),
    Variance(NodeId),
    Max(NodeId),
    Min(NodeId),
    Sum(NodeId),
    Count(NodeId),
    Prev(NodeId),
    PrevBy(NodeId, Arc<dyn Fn(&R) -> u64 + Send + Sync>),
    Lag(NodeId),
    Delta(NodeId),
    Add(NodeId, NodeId),
    Sub(NodeId, NodeId),
    Mul(NodeId, NodeId),
    Div(NodeId, NodeId),
    MulConst(NodeId, f64),
    AddConst(NodeId, f64),
    Abs(NodeId),
    Sqrt(NodeId),
    Ln(NodeId),
    Neg(NodeId),
    Cross(NodeId, NodeId),
    CrossAbove(NodeId, NodeId),
    CrossUnder(NodeId, NodeId),
    CrossHysteresis(NodeId, NodeId),
    Rate(NodeId),
    Velocity(NodeId),
    Acceleration(NodeId),
    Gt(NodeId, NodeId),
    Gte(NodeId, NodeId),
    Lt(NodeId, NodeId),
    Lte(NodeId, NodeId),
    Eq(NodeId, NodeId),
    // Statistical
    Median(NodeId),
    Quantile(NodeId, f64),
    Correlation(NodeId, NodeId),
    Covariance(NodeId, NodeId),
    Skewness(NodeId),
    Kurtosis(NodeId),
    Rank(NodeId),
    // Moving averages
    Wma(NodeId),
    // Momentum
    Rsi(NodeId),
    // Cumulative
    CumSum(NodeId),
    CumMax(NodeId),
    CumMin(NodeId),
    CumProd(NodeId),
    // Returns
    PctChange(NodeId),
    LogReturn(NodeId),
    // Math
    Pow(NodeId, f64),
    Exp(NodeId),
    Log10(NodeId),
    Log2(NodeId),
    Clamp(NodeId, f64, f64),
    Floor(NodeId),
    Ceil(NodeId),
    Round(NodeId),
    DivConst(NodeId, f64),
    // Trigger primitives
    GlitchFilter(NodeId),
    RuntDetect(NodeId),
    PulseWidth(NodeId),
    WindowDetect(NodeId),
    // Custom functional operators
    MapF64(NodeId, Arc<dyn Fn(f64) -> f64 + Send + Sync>),
    Map2F64(NodeId, NodeId, Arc<dyn Fn(f64, f64) -> f64 + Send + Sync>),
    FilterF64(NodeId, Arc<dyn Fn(f64) -> bool + Send + Sync>),
    FilterMapF64(NodeId, Arc<dyn Fn(f64) -> Option<f64> + Send + Sync>),
    ScanF64(
        NodeId,
        Arc<dyn Fn() -> Box<dyn Any + Send + Sync> + Send + Sync>,
        Arc<dyn Fn(&mut Box<dyn Any + Send + Sync>, f64) -> f64 + Send + Sync>,
    ),
    Scan2F64(
        NodeId,
        NodeId,
        Arc<dyn Fn() -> Box<dyn Any + Send + Sync> + Send + Sync>,
        Arc<dyn Fn(&mut Box<dyn Any + Send + Sync>, f64, f64) -> f64 + Send + Sync>,
    ),
    /// Custom plugin node: resolves `inputs` and delegates to a
    /// [`CustomNode`](crate::custom_node::CustomNode) held in `NodeState`.
    Custom {
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

        let mapper: Arc<dyn Fn(&ValueStore) -> Option<Value> + Send + Sync> =
            Arc::new(move |store| {
                let input = O::extract(store, &input_ids)?;
                let output = f(input);
                Some(Value::Other(Box::new(output)))
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
            dyn Fn(&ValueStore, &Mutex<Box<dyn Any + Send + Sync>>) -> Option<Value>
                + Send
                + Sync,
        > = Arc::new(move |store, acc_mutex| {
            let input = O::extract(store, &input_ids)?;
            let mut guard = acc_mutex.lock().ok()?;
            let current = guard.downcast_ref::<Acc>()?.clone();
            let next = f(current, input);
            *guard = Box::new(next.clone());
            Some(Value::Other(Box::new(next)))
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
