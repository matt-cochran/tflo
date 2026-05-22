//! Computation graph nodes and handles.
//!
//! This module defines the core computation description types:
//! - [`Node`]: A node in the computation graph
//! - [`Comp`]: A handle to a computation that can be composed
//! - [`ThresholdCross`]: Threshold crossing modes (Rising, Falling, None)

mod cross;
mod custom;
mod dual_use;
mod math;
mod plugin;
mod stateful;
mod windowed;

use crate::builder::BuilderState;
use crate::event::ThresholdCrossEventMode;
use crate::operator::OperatorFactory;
use crate::window::Window;
// Note: CrossBuilderExt is intentionally not imported here to avoid conflict with existing cross() method
// Users can import it explicitly if they want the fluent builder API
use std::cell::RefCell;
use std::marker::PhantomData;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

/// Unique identifier for a node in the computation graph.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct NodeId(pub(crate) usize);

/// A node in the computation graph.
///
/// Nodes describe computations to be performed on streaming data.
/// They form a DAG (directed acyclic graph) that is compiled into
/// an executable form.
#[derive(Clone)]
pub enum Node<R> {
    // === Sources ===
    /// Extract a property from the input record.
    Prop(Arc<dyn Fn(&R) -> f64 + Send + Sync>),
    /// Constant value.
    Const(f64),

    // === Time-based aggregations ===
    /// Simple moving average over a window.
    Sma(NodeId, Window),
    /// Exponential moving average (time-based decay or count-based).
    Ema(NodeId, Window),
    /// Standard deviation over a window.
    Std(NodeId, Window),
    /// Variance over a window.
    Variance(NodeId, Window),
    /// Maximum over a window.
    Max(NodeId, Window),
    /// Minimum over a window.
    Min(NodeId, Window),
    /// Sum over a window.
    Sum(NodeId, Window),
    /// Count of values in a window.
    Count(NodeId, Window),

    // === Lookback ===
    /// Previous value.
    Prev(NodeId),
    /// Previous value partitioned by key.
    PrevBy(NodeId, Arc<dyn Fn(&R) -> u64 + Send + Sync>),
    /// Value from a specified time ago.
    Lag(NodeId, Duration),
    /// Current value minus value from a specified time ago.
    Delta(NodeId, Duration),

    // === Arithmetic ===
    /// Addition of two nodes.
    Add(NodeId, NodeId),
    /// Subtraction of two nodes.
    Sub(NodeId, NodeId),
    /// Multiplication of two nodes.
    Mul(NodeId, NodeId),
    /// Division of two nodes.
    Div(NodeId, NodeId),
    /// Multiplication by a constant.
    MulConst(NodeId, f64),
    /// Addition of a constant.
    AddConst(NodeId, f64),
    /// Absolute value.
    Abs(NodeId),
    /// Square root.
    Sqrt(NodeId),
    /// Natural logarithm.
    Ln(NodeId),
    /// Negation.
    Neg(NodeId),

    // === Cross detection ===
    /// Detect when first node crosses second node (either direction).
    Cross(NodeId, NodeId),
    /// Detect when first node crosses above second node.
    CrossAbove(NodeId, NodeId),
    /// Detect when first node crosses under second node.
    CrossUnder(NodeId, NodeId),
    /// Cross detection with hysteresis (margin for noise immunity).
    CrossHysteresis(NodeId, NodeId, f64),

    // === Trigger primitives ===
    /// Glitch filter: filters out pulses shorter than minimum duration.
    /// Parameters: input, threshold, min_duration_ms
    GlitchFilterNode(NodeId, f64, i64),
    /// Runt detector: detects incomplete amplitude transitions.
    /// Parameters: input, low_threshold, high_threshold
    RuntDetectNode(NodeId, f64, f64),
    /// Pulse width detector: validates pulse duration.
    /// Parameters: input, threshold, min_width_ms, max_width_ms
    PulseWidthNode(NodeId, f64, i64, i64),
    /// Window detector: monitors signal entering/exiting amplitude bounds.
    /// Parameters: input, low_threshold, high_threshold
    WindowDetectNode(NodeId, f64, f64),

    // === Custom functional graph primitives ===
    /// Stateless unary transform: `f64 -> f64`.
    MapF64 {
        /// Input node.
        input: NodeId,
        /// The closure.
        f: Arc<dyn Fn(f64) -> f64 + Send + Sync>,
        /// Optional human-readable name.
        name: Option<String>,
    },
    /// Stateless binary transform: `(f64, f64) -> f64`.
    Map2F64 {
        /// First input.
        a: NodeId,
        /// Second input.
        b: NodeId,
        /// The closure.
        f: Arc<dyn Fn(f64, f64) -> f64 + Send + Sync>,
        /// Optional human-readable name.
        name: Option<String>,
    },
    /// Filter: keep/drop based on predicate.
    FilterF64 {
        /// Input node.
        input: NodeId,
        /// The predicate.
        predicate: Arc<dyn Fn(f64) -> bool + Send + Sync>,
        /// Optional human-readable name.
        name: Option<String>,
    },
    /// Filter-map: transform and optionally suppress output.
    FilterMapF64 {
        /// Input node.
        input: NodeId,
        /// The closure.
        f: Arc<dyn Fn(f64) -> Option<f64> + Send + Sync>,
        /// Optional human-readable name.
        name: Option<String>,
    },
    /// Stateful unary scan.
    ScanF64 {
        /// Input node.
        input: NodeId,
        /// State constructor.
        ctor: Arc<dyn Fn() -> Box<dyn std::any::Any + Send + Sync> + Send + Sync>,
        /// Step closure. Yields a [`Computed`](crate::compile::Computed) so a
        /// state-type mismatch degrades to an absence rather than a panic.
        step: Arc<
            dyn Fn(&mut Box<dyn std::any::Any + Send + Sync>, f64) -> crate::compile::Computed
                + Send
                + Sync,
        >,
        /// Optional human-readable name.
        name: Option<String>,
    },
    /// Stateful binary scan.
    Scan2F64 {
        /// First input.
        a: NodeId,
        /// Second input.
        b: NodeId,
        /// State constructor.
        ctor: Arc<dyn Fn() -> Box<dyn std::any::Any + Send + Sync> + Send + Sync>,
        /// Step closure. Yields a [`Computed`](crate::compile::Computed) so a
        /// state-type mismatch degrades to an absence rather than a panic.
        step: Arc<
            dyn Fn(&mut Box<dyn std::any::Any + Send + Sync>, f64, f64) -> crate::compile::Computed
                + Send
                + Sync,
        >,
        /// Optional human-readable name.
        name: Option<String>,
    },

    // === Rate-based ===
    /// Rate of change per unit time.
    Rate(NodeId, Duration),
    /// First derivative (velocity).
    Velocity(NodeId, Duration),
    /// Second derivative (acceleration).
    Acceleration(NodeId, Duration),

    // === Comparison ===
    /// Greater than: returns 1.0 if true, 0.0 if false.
    Gt(NodeId, NodeId),
    /// Greater than or equal.
    Gte(NodeId, NodeId),
    /// Less than.
    Lt(NodeId, NodeId),
    /// Less than or equal.
    Lte(NodeId, NodeId),
    /// Equal (within epsilon).
    Eq(NodeId, NodeId),

    // === Statistical ===
    /// Rolling median over a window.
    Median(NodeId, Window),
    /// Rolling quantile at given percentile (0.0 to 1.0).
    Quantile(NodeId, Window, f64),
    /// Rolling correlation between two values.
    Correlation(NodeId, NodeId, Window),
    /// Rolling covariance between two values.
    Covariance(NodeId, NodeId, Window),
    /// Rolling skewness.
    Skewness(NodeId, Window),
    /// Rolling excess kurtosis.
    Kurtosis(NodeId, Window),
    /// Rolling rank (percentile of current value within window).
    Rank(NodeId, Window),

    // === Moving Averages ===
    /// Weighted moving average (linear weights).
    Wma(NodeId, Window),

    // === Momentum ===
    /// Relative Strength Index.
    Rsi(NodeId, Window),
    /// Relative Strength Index with Wilder's smoothing.
    RsiWilder(NodeId, usize),

    // === Cumulative (Expanding) ===
    /// Cumulative sum since start.
    CumSum(NodeId),
    /// Cumulative maximum (high-water mark).
    CumMax(NodeId),
    /// Cumulative minimum.
    CumMin(NodeId),
    /// Cumulative product.
    CumProd(NodeId),

    // === Returns ===
    /// Percentage change from previous value: (current - prev) / prev * 100.
    PctChange(NodeId),
    /// Log return: ln(current / prev).
    LogReturn(NodeId),

    // === Math functions ===
    /// Power function: x^n.
    Pow(NodeId, f64),
    /// Exponential: e^x.
    Exp(NodeId),
    /// Base-10 logarithm.
    Log10(NodeId),
    /// Base-2 logarithm.
    Log2(NodeId),
    /// Clamp value to [min, max].
    Clamp(NodeId, f64, f64),
    /// Floor.
    Floor(NodeId),
    /// Ceiling.
    Ceil(NodeId),
    /// Round to nearest integer.
    Round(NodeId),
    /// Division by constant.
    DivConst(NodeId, f64),

    // === Plugin nodes ===
    /// A user-defined runtime node contributed by an external crate via the
    /// [`Operator`](crate::operator::Operator) trait.
    Plugin {
        /// Input node IDs, in declaration order.
        inputs: Vec<NodeId>,
        /// Factory producing a fresh operator instance per compiled graph.
        factory: OperatorFactory,
    },
}

impl<R> std::fmt::Debug for Node<R> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Prop(_) => write!(f, "Prop(<fn>)"),
            Self::Const(v) => write!(f, "Const({v})"),
            Self::Sma(id, w) => write!(f, "Sma({id:?}, {w:?})"),
            Self::Ema(id, w) => write!(f, "Ema({id:?}, {w:?})"),
            Self::Std(id, w) => write!(f, "Std({id:?}, {w:?})"),
            Self::Variance(id, w) => write!(f, "Variance({id:?}, {w:?})"),
            Self::Max(id, w) => write!(f, "Max({id:?}, {w:?})"),
            Self::Min(id, w) => write!(f, "Min({id:?}, {w:?})"),
            Self::Sum(id, w) => write!(f, "Sum({id:?}, {w:?})"),
            Self::Count(id, w) => write!(f, "Count({id:?}, {w:?})"),
            Self::Prev(id) => write!(f, "Prev({id:?})"),
            Self::PrevBy(id, _) => write!(f, "PrevBy({id:?}, <fn>)"),
            Self::Lag(id, d) => write!(f, "Lag({id:?}, {d:?})"),
            Self::Delta(id, d) => write!(f, "Delta({id:?}, {d:?})"),
            Self::Add(a, b) => write!(f, "Add({a:?}, {b:?})"),
            Self::Sub(a, b) => write!(f, "Sub({a:?}, {b:?})"),
            Self::Mul(a, b) => write!(f, "Mul({a:?}, {b:?})"),
            Self::Div(a, b) => write!(f, "Div({a:?}, {b:?})"),
            Self::MulConst(id, c) => write!(f, "MulConst({id:?}, {c})"),
            Self::AddConst(id, c) => write!(f, "AddConst({id:?}, {c})"),
            Self::Abs(id) => write!(f, "Abs({id:?})"),
            Self::Sqrt(id) => write!(f, "Sqrt({id:?})"),
            Self::Ln(id) => write!(f, "Ln({id:?})"),
            Self::Neg(id) => write!(f, "Neg({id:?})"),
            Self::Cross(a, b) => write!(f, "Cross({a:?}, {b:?})"),
            Self::CrossAbove(a, b) => write!(f, "CrossAbove({a:?}, {b:?})"),
            Self::CrossUnder(a, b) => write!(f, "CrossUnder({a:?}, {b:?})"),
            Self::CrossHysteresis(a, b, m) => write!(f, "CrossHysteresis({a:?}, {b:?}, {m})"),
            // Trigger primitives
            Self::GlitchFilterNode(id, t, d) => write!(f, "GlitchFilterNode({id:?}, {t}, {d})"),
            Self::RuntDetectNode(id, l, h) => write!(f, "RuntDetectNode({id:?}, {l}, {h})"),
            Self::PulseWidthNode(id, t, min, max) => {
                write!(f, "PulseWidthNode({id:?}, {t}, {min}, {max})")
            }
            Self::WindowDetectNode(id, l, h) => write!(f, "WindowDetectNode({id:?}, {l}, {h})"),
            Self::Rate(id, d) => write!(f, "Rate({id:?}, {d:?})"),
            Self::Velocity(id, d) => write!(f, "Velocity({id:?}, {d:?})"),
            Self::Acceleration(id, d) => write!(f, "Acceleration({id:?}, {d:?})"),
            Self::Gt(a, b) => write!(f, "Gt({a:?}, {b:?})"),
            Self::Gte(a, b) => write!(f, "Gte({a:?}, {b:?})"),
            Self::Lt(a, b) => write!(f, "Lt({a:?}, {b:?})"),
            Self::Lte(a, b) => write!(f, "Lte({a:?}, {b:?})"),
            Self::Eq(a, b) => write!(f, "Eq({a:?}, {b:?})"),
            Self::Median(id, w) => write!(f, "Median({id:?}, {w:?})"),
            Self::Quantile(id, w, q) => write!(f, "Quantile({id:?}, {w:?}, {q})"),
            Self::Correlation(a, b, w) => write!(f, "Correlation({a:?}, {b:?}, {w:?})"),
            Self::Covariance(a, b, w) => write!(f, "Covariance({a:?}, {b:?}, {w:?})"),
            Self::Skewness(id, w) => write!(f, "Skewness({id:?}, {w:?})"),
            Self::Kurtosis(id, w) => write!(f, "Kurtosis({id:?}, {w:?})"),
            Self::Rank(id, w) => write!(f, "Rank({id:?}, {w:?})"),
            Self::Wma(id, w) => write!(f, "Wma({id:?}, {w:?})"),
            Self::Rsi(id, w) => write!(f, "Rsi({id:?}, {w:?})"),
            Self::RsiWilder(id, n) => write!(f, "RsiWilder({id:?}, period={n})"),
            Self::CumSum(id) => write!(f, "CumSum({id:?})"),
            Self::CumMax(id) => write!(f, "CumMax({id:?})"),
            Self::CumMin(id) => write!(f, "CumMin({id:?})"),
            Self::CumProd(id) => write!(f, "CumProd({id:?})"),
            Self::PctChange(id) => write!(f, "PctChange({id:?})"),
            Self::LogReturn(id) => write!(f, "LogReturn({id:?})"),
            Self::Pow(id, n) => write!(f, "Pow({id:?}, {n})"),
            Self::Exp(id) => write!(f, "Exp({id:?})"),
            Self::Log10(id) => write!(f, "Log10({id:?})"),
            Self::Log2(id) => write!(f, "Log2({id:?})"),
            Self::Clamp(id, min, max) => write!(f, "Clamp({id:?}, {min}, {max})"),
            Self::Floor(id) => write!(f, "Floor({id:?})"),
            Self::Ceil(id) => write!(f, "Ceil({id:?})"),
            Self::Round(id) => write!(f, "Round({id:?})"),
            Self::DivConst(id, c) => write!(f, "DivConst({id:?}, {c})"),
            // Custom functional operators
            Self::MapF64 { input, name, .. } => match name {
                Some(n) => write!(f, "MapF64({input:?}, name=\"{n}\")"),
                None => write!(f, "MapF64({input:?})"),
            },
            Self::Map2F64 { a, b, name, .. } => match name {
                Some(n) => write!(f, "Map2F64({a:?}, {b:?}, name=\"{n}\")"),
                None => write!(f, "Map2F64({a:?}, {b:?})"),
            },
            Self::FilterF64 { input, name, .. } => match name {
                Some(n) => write!(f, "FilterF64({input:?}, name=\"{n}\")"),
                None => write!(f, "FilterF64({input:?})"),
            },
            Self::FilterMapF64 { input, name, .. } => match name {
                Some(n) => write!(f, "FilterMapF64({input:?}, name=\"{n}\")"),
                None => write!(f, "FilterMapF64({input:?})"),
            },
            Self::ScanF64 { input, name, .. } => match name {
                Some(n) => write!(f, "ScanF64({input:?}, name=\"{n}\")"),
                None => write!(f, "ScanF64({input:?})"),
            },
            Self::Scan2F64 { a, b, name, .. } => match name {
                Some(n) => write!(f, "Scan2F64({a:?}, {b:?}, name=\"{n}\")"),
                None => write!(f, "Scan2F64({a:?}, {b:?})"),
            },
            Self::Plugin { inputs, .. } => write!(f, "Plugin({inputs:?})"),
        }
    }
}

/// A handle to a computation in the graph.
///
/// `Comp<R, T>` represents a computation over records of type `R` that
/// produces values of type `T`. The computation is described lazily
/// and compiled into an executable form when the iterator is created.
///
/// `Comp` stores only a `NodeId` and a reference to shared builder state.
/// All `Comp` instances from the same builder share the same underlying state.
/// Cloning is cheap since it only increments a reference count.
///
/// # Type Parameters
///
/// - `R`: The input record type
/// - `T`: The output type (usually `f64` or `ThresholdCrossEventMode`)
pub struct Comp<R, T = f64> {
    pub(crate) id: NodeId,
    pub(crate) state: Rc<RefCell<BuilderState<R>>>,
    pub(crate) _marker: PhantomData<T>,
}

impl<R, T> Clone for Comp<R, T> {
    fn clone(&self) -> Self {
        Comp {
            id: self.id,
            state: Rc::clone(&self.state),
            _marker: PhantomData,
        }
    }
}

impl<R, T> std::fmt::Debug for Comp<R, T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Comp({:?})", self.id)
    }
}

impl<R, T> Comp<R, T> {
    /// Helper method to add a node to the shared builder state.
    pub(crate) fn add_node_to_state(
        state: &Rc<RefCell<BuilderState<R>>>,
        node: Node<R>,
    ) -> Comp<R, f64> {
        let mut builder_state = state.borrow_mut();
        let id = builder_state.next_id();
        builder_state.nodes.push((id, node));
        Comp {
            id,
            state: Rc::clone(state),
            _marker: PhantomData,
        }
    }

    /// Helper method to add a signal node to the shared builder state.
    pub(crate) fn add_signal_node_to_state(
        state: &Rc<RefCell<BuilderState<R>>>,
        node: Node<R>,
    ) -> Comp<R, ThresholdCrossEventMode> {
        let mut builder_state = state.borrow_mut();
        let id = builder_state.next_id();
        builder_state.nodes.push((id, node));
        Comp {
            id,
            state: Rc::clone(state),
            _marker: PhantomData,
        }
    }

    /// Add a node and tag the returned [`Comp`] with a caller-chosen output
    /// marker `O`.
    ///
    /// Identical to [`add_node_to_state`](Self::add_node_to_state) except the
    /// output type is generic rather than hardcoded to `f64`. The marker is
    /// pure [`PhantomData`] — it has no effect on the node stored in the
    /// builder. It lets a typed-output plugin builder yield `Comp<R, O>` for a
    /// non-`f64` `O` (e.g. an event enum).
    pub(crate) fn add_node_to_state_typed<O>(
        state: &Rc<RefCell<BuilderState<R>>>,
        node: Node<R>,
    ) -> Comp<R, O> {
        let mut builder_state = state.borrow_mut();
        let id = builder_state.next_id();
        builder_state.nodes.push((id, node));
        Comp {
            id,
            state: Rc::clone(state),
            _marker: PhantomData,
        }
    }
}

// ============================================================================
// OPERATOR OVERLOADS
// ============================================================================

impl<R: 'static> std::ops::Add for &Comp<R> {
    type Output = Comp<R>;

    fn add(self, rhs: &Comp<R>) -> Comp<R> {
        Comp::<R, f64>::add_node_to_state(&self.state, Node::Add(self.id, rhs.id))
    }
}

impl<R: 'static> std::ops::Sub for &Comp<R> {
    type Output = Comp<R>;

    fn sub(self, rhs: &Comp<R>) -> Comp<R> {
        Comp::<R, f64>::add_node_to_state(&self.state, Node::Sub(self.id, rhs.id))
    }
}

impl<R: 'static> std::ops::Mul for &Comp<R> {
    type Output = Comp<R>;

    fn mul(self, rhs: &Comp<R>) -> Comp<R> {
        Comp::<R, f64>::add_node_to_state(&self.state, Node::Mul(self.id, rhs.id))
    }
}

impl<R: 'static> std::ops::Div for &Comp<R> {
    type Output = Comp<R>;

    fn div(self, rhs: &Comp<R>) -> Comp<R> {
        Comp::<R, f64>::add_node_to_state(&self.state, Node::Div(self.id, rhs.id))
    }
}

// Allow owned Comp / &Comp for chaining (e.g., `(a - b) / &c`)
impl<R: 'static> std::ops::Div<&Comp<R>> for Comp<R> {
    type Output = Comp<R>;

    fn div(self, rhs: &Comp<R>) -> Comp<R> {
        Comp::<R, f64>::add_node_to_state(&self.state, Node::Div(self.id, rhs.id))
    }
}

impl<R: 'static> std::ops::Mul<&Comp<R>> for Comp<R> {
    type Output = Comp<R>;

    fn mul(self, rhs: &Comp<R>) -> Comp<R> {
        Comp::<R, f64>::add_node_to_state(&self.state, Node::Mul(self.id, rhs.id))
    }
}

impl<R: 'static> std::ops::Add<&Comp<R>> for Comp<R> {
    type Output = Comp<R>;

    fn add(self, rhs: &Comp<R>) -> Comp<R> {
        Comp::<R, f64>::add_node_to_state(&self.state, Node::Add(self.id, rhs.id))
    }
}

impl<R: 'static> std::ops::Sub<&Comp<R>> for Comp<R> {
    type Output = Comp<R>;

    fn sub(self, rhs: &Comp<R>) -> Comp<R> {
        Comp::<R, f64>::add_node_to_state(&self.state, Node::Sub(self.id, rhs.id))
    }
}

impl<R: 'static> std::ops::Mul<f64> for &Comp<R> {
    type Output = Comp<R>;

    fn mul(self, rhs: f64) -> Comp<R> {
        Comp::<R, f64>::add_node_to_state(&self.state, Node::MulConst(self.id, rhs))
    }
}

impl<R: 'static> std::ops::Add<f64> for &Comp<R> {
    type Output = Comp<R>;

    fn add(self, rhs: f64) -> Comp<R> {
        Comp::<R, f64>::add_node_to_state(&self.state, Node::AddConst(self.id, rhs))
    }
}

impl<R: 'static> std::ops::Sub<f64> for &Comp<R> {
    type Output = Comp<R>;

    fn sub(self, rhs: f64) -> Comp<R> {
        Comp::<R, f64>::add_node_to_state(&self.state, Node::AddConst(self.id, -rhs))
    }
}

impl<R: 'static> std::ops::Neg for &Comp<R> {
    type Output = Comp<R>;

    fn neg(self) -> Comp<R> {
        Comp::<R, f64>::add_node_to_state(&self.state, Node::Neg(self.id))
    }
}

// ============================================================================
// OWNED OPERATORS WITH f64 (for chaining: `(a - b) * 100.0`)
// ============================================================================

impl<R: 'static> std::ops::Mul<f64> for Comp<R> {
    type Output = Comp<R>;

    fn mul(self, rhs: f64) -> Comp<R> {
        Comp::<R, f64>::add_node_to_state(&self.state, Node::MulConst(self.id, rhs))
    }
}

impl<R: 'static> std::ops::Div<f64> for Comp<R> {
    type Output = Comp<R>;

    fn div(self, rhs: f64) -> Comp<R> {
        Comp::<R, f64>::add_node_to_state(&self.state, Node::DivConst(self.id, rhs))
    }
}

impl<R: 'static> std::ops::Div<f64> for &Comp<R> {
    type Output = Comp<R>;

    fn div(self, rhs: f64) -> Comp<R> {
        Comp::<R, f64>::add_node_to_state(&self.state, Node::DivConst(self.id, rhs))
    }
}

impl<R: 'static> std::ops::Add<f64> for Comp<R> {
    type Output = Comp<R>;

    fn add(self, rhs: f64) -> Comp<R> {
        Comp::<R, f64>::add_node_to_state(&self.state, Node::AddConst(self.id, rhs))
    }
}

impl<R: 'static> std::ops::Sub<f64> for Comp<R> {
    type Output = Comp<R>;

    fn sub(self, rhs: f64) -> Comp<R> {
        Comp::<R, f64>::add_node_to_state(&self.state, Node::AddConst(self.id, -rhs))
    }
}

// Owned Comp / Owned Comp for convenience
impl<R: 'static> std::ops::Div for Comp<R> {
    type Output = Comp<R>;

    fn div(self, rhs: Comp<R>) -> Comp<R> {
        Comp::<R, f64>::add_node_to_state(&self.state, Node::Div(self.id, rhs.id))
    }
}

impl<R: 'static> std::ops::Add for Comp<R> {
    type Output = Comp<R>;

    fn add(self, rhs: Comp<R>) -> Comp<R> {
        Comp::<R, f64>::add_node_to_state(&self.state, Node::Add(self.id, rhs.id))
    }
}

impl<R: 'static> std::ops::Sub for Comp<R> {
    type Output = Comp<R>;

    fn sub(self, rhs: Comp<R>) -> Comp<R> {
        Comp::<R, f64>::add_node_to_state(&self.state, Node::Sub(self.id, rhs.id))
    }
}

impl<R: 'static> std::ops::Mul for Comp<R> {
    type Output = Comp<R>;

    fn mul(self, rhs: Comp<R>) -> Comp<R> {
        Comp::<R, f64>::add_node_to_state(&self.state, Node::Mul(self.id, rhs.id))
    }
}
