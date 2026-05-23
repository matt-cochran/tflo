//! Computation graph nodes and handles.
//!
//! This module defines the core computation description types:
//! - [`Node`]: A node in the computation graph (sources, closures, and plugin delegates)
//! - [`Comp`]: A handle to a computation that can be composed

mod custom;
mod plugin;

use crate::builder::BuilderState;
use crate::operator::OperatorFactory;
use std::cell::RefCell;
use std::marker::PhantomData;
use std::rc::Rc;
use std::sync::Arc;

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
/// - `T`: The output type (usually `f64` or an event type from `tflo-ops`)
pub struct Comp<R, T = f64> {
    pub(crate) id: NodeId,
    pub(crate) state: Rc<RefCell<BuilderState<R>>>,
    pub(crate) _marker: PhantomData<T>,
}

impl<R, T> Clone for Comp<R, T> {
    fn clone(&self) -> Self {
        Self {
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
        self.map2_f64(rhs, |a, b| a + b)
    }
}

impl<R: 'static> std::ops::Sub for &Comp<R> {
    type Output = Comp<R>;

    fn sub(self, rhs: &Comp<R>) -> Comp<R> {
        self.map2_f64(rhs, |a, b| a - b)
    }
}

impl<R: 'static> std::ops::Mul for &Comp<R> {
    type Output = Comp<R>;

    fn mul(self, rhs: &Comp<R>) -> Comp<R> {
        self.map2_f64(rhs, |a, b| a * b)
    }
}

/// Builds a closure node — division by zero produces `f64::INFINITY` /
/// `f64::NAN`, which downstream `finite_or_warming` mapping turns into
/// `Absent::WarmingUp` (not the older `Absent::DivideByZero`).
impl<R: 'static> std::ops::Div for &Comp<R> {
    type Output = Comp<R>;

    fn div(self, rhs: &Comp<R>) -> Comp<R> {
        self.map2_f64(rhs, |a, b| a / b)
    }
}

// Allow owned Comp / &Comp for chaining (e.g., `(a - b) / &c`)
/// Builds a closure node — division by zero produces `f64::INFINITY` /
/// `f64::NAN`, which downstream `finite_or_warming` mapping turns into
/// `Absent::WarmingUp` (not the older `Absent::DivideByZero`).
impl<R: 'static> std::ops::Div<&Self> for Comp<R> {
    type Output = Self;

    fn div(self, rhs: &Self) -> Self {
        self.map2_f64(rhs, |a, b| a / b)
    }
}

impl<R: 'static> std::ops::Mul<&Self> for Comp<R> {
    type Output = Self;

    fn mul(self, rhs: &Self) -> Self {
        self.map2_f64(rhs, |a, b| a * b)
    }
}

impl<R: 'static> std::ops::Add<&Self> for Comp<R> {
    type Output = Self;

    fn add(self, rhs: &Self) -> Self {
        self.map2_f64(rhs, |a, b| a + b)
    }
}

impl<R: 'static> std::ops::Sub<&Self> for Comp<R> {
    type Output = Self;

    fn sub(self, rhs: &Self) -> Self {
        self.map2_f64(rhs, |a, b| a - b)
    }
}

impl<R: 'static> std::ops::Mul<f64> for &Comp<R> {
    type Output = Comp<R>;

    fn mul(self, rhs: f64) -> Comp<R> {
        self.map_f64(move |a| a * rhs)
    }
}

impl<R: 'static> std::ops::Add<f64> for &Comp<R> {
    type Output = Comp<R>;

    fn add(self, rhs: f64) -> Comp<R> {
        self.map_f64(move |a| a + rhs)
    }
}

impl<R: 'static> std::ops::Sub<f64> for &Comp<R> {
    type Output = Comp<R>;

    fn sub(self, rhs: f64) -> Comp<R> {
        self.map_f64(move |a| a - rhs)
    }
}

impl<R: 'static> std::ops::Neg for &Comp<R> {
    type Output = Comp<R>;

    fn neg(self) -> Comp<R> {
        self.map_f64(|a| -a)
    }
}

// ============================================================================
// OWNED OPERATORS WITH f64 (for chaining: `(a - b) * 100.0`)
// ============================================================================

impl<R: 'static> std::ops::Mul<f64> for Comp<R> {
    type Output = Self;

    fn mul(self, rhs: f64) -> Self {
        self.map_f64(move |a| a * rhs)
    }
}

/// Builds a closure node — division by zero produces `f64::INFINITY` /
/// `f64::NAN`, which downstream `finite_or_warming` mapping turns into
/// `Absent::WarmingUp` (not the older `Absent::DivideByZero`).
impl<R: 'static> std::ops::Div<f64> for Comp<R> {
    type Output = Self;

    fn div(self, rhs: f64) -> Self {
        self.map_f64(move |a| a / rhs)
    }
}

/// Builds a closure node — division by zero produces `f64::INFINITY` /
/// `f64::NAN`, which downstream `finite_or_warming` mapping turns into
/// `Absent::WarmingUp` (not the older `Absent::DivideByZero`).
impl<R: 'static> std::ops::Div<f64> for &Comp<R> {
    type Output = Comp<R>;

    fn div(self, rhs: f64) -> Comp<R> {
        self.map_f64(move |a| a / rhs)
    }
}

impl<R: 'static> std::ops::Add<f64> for Comp<R> {
    type Output = Self;

    fn add(self, rhs: f64) -> Self {
        self.map_f64(move |a| a + rhs)
    }
}

impl<R: 'static> std::ops::Sub<f64> for Comp<R> {
    type Output = Self;

    fn sub(self, rhs: f64) -> Self {
        self.map_f64(move |a| a - rhs)
    }
}

// Owned Comp / Owned Comp for convenience
/// Builds a closure node — division by zero produces `f64::INFINITY` /
/// `f64::NAN`, which downstream `finite_or_warming` mapping turns into
/// `Absent::WarmingUp` (not the older `Absent::DivideByZero`).
impl<R: 'static> std::ops::Div for Comp<R> {
    type Output = Self;

    fn div(self, rhs: Self) -> Self {
        self.map2_f64(&rhs, |a, b| a / b)
    }
}

impl<R: 'static> std::ops::Add for Comp<R> {
    type Output = Self;

    fn add(self, rhs: Self) -> Self {
        self.map2_f64(&rhs, |a, b| a + b)
    }
}

impl<R: 'static> std::ops::Sub for Comp<R> {
    type Output = Self;

    fn sub(self, rhs: Self) -> Self {
        self.map2_f64(&rhs, |a, b| a - b)
    }
}

impl<R: 'static> std::ops::Mul for Comp<R> {
    type Output = Self;

    fn mul(self, rhs: Self) -> Self {
        self.map2_f64(&rhs, |a, b| a * b)
    }
}
