//! Compilation context for building [`CompiledNode`]s from [`Node`] definitions.
//!
//! [`CompilationCtx`] holds any dependencies needed during graph compilation.
//! Currently it is a marker, but future options (validation hooks, feature flags,
//! custom type resolvers, …) can be added here without changing the signature of
//! `compile_node`.

use crate::comp::Node;
use crate::comp::NodeId;
use crate::compile::{CompiledNode, NodeOp, NodeState, RsiWilderState};
use crate::primitives::{
    CorrelationCountWindow, CorrelationTimeWindow, CountEma, CountWindow, CrossDetector,
    CumulativeMax, CumulativeMin, CumulativeProduct, CumulativeSum, GlitchFilter,
    HysteresisCrossDetector, LagBuffer, MedianCountWindow, MedianTimeWindow, MomentsCountWindow,
    MomentsTimeWindow, PrevByTracker, PrevTracker, PulseWidthDetector, RsiCountWindow,
    RsiTimeWindow, RuntDetector, TimeEma, TimeWindow, WindowDetector, WmaCountWindow,
    WmaTimeWindow,
};
use crate::window::Window;

/// Context object that carries dependencies for graph compilation.
///
/// Pass this as the first argument to [`compile_node`][Self::compile_node].
/// The struct is intentionally empty for now; new fields can be added here
/// without modifying the `compile_node` signature.
pub struct CompilationCtx<R> {
    _marker: std::marker::PhantomData<R>,
}

impl<R> CompilationCtx<R> {
    /// Create a new compilation context.
    pub fn new() -> Self {
        Self {
            _marker: std::marker::PhantomData,
        }
    }

    /// Compile a single [`Node`] into a [`CompiledNode`].
    ///
    /// This method maps each node variant to its corresponding operation
    /// and initialises the appropriate state tracker (stateless, windowed,
    /// cumulative, cross-detection, trigger-primitive, …).
    pub fn compile_node(&self, id: NodeId, node: Node<R>) -> CompiledNode<R> {
        match node {
            Node::Prop(f) => CompiledNode {
                id,
                op: NodeOp::Prop(f),
                state: NodeState::Stateless,
            },
            Node::Const(v) => CompiledNode {
                id,
                op: NodeOp::Const(v),
                state: NodeState::Stateless,
            },
            Node::Sma(input, window) => CompiledNode {
                id,
                op: NodeOp::Sma(input),
                state: match window {
                    Window::Time(d) => NodeState::TimeWindow(TimeWindow::new(d)),
                    Window::Count(n) => NodeState::CountWindow(CountWindow::new(n)),
                },
            },
            Node::Ema(input, window) => CompiledNode {
                id,
                op: NodeOp::Ema(input),
                state: match window {
                    Window::Time(d) => NodeState::TimeEma(TimeEma::new(d)),
                    Window::Count(n) => NodeState::CountEma(CountEma::new(n)),
                },
            },
            Node::Std(input, window) => CompiledNode {
                id,
                op: NodeOp::Std(input),
                state: match window {
                    Window::Time(d) => NodeState::TimeWindow(TimeWindow::new(d)),
                    Window::Count(n) => NodeState::CountWindow(CountWindow::new(n)),
                },
            },
            Node::Variance(input, window) => CompiledNode {
                id,
                op: NodeOp::Variance(input),
                state: match window {
                    Window::Time(d) => NodeState::TimeWindow(TimeWindow::new(d)),
                    Window::Count(n) => NodeState::CountWindow(CountWindow::new(n)),
                },
            },
            Node::Max(input, window) => CompiledNode {
                id,
                op: NodeOp::Max(input),
                state: match window {
                    Window::Time(d) => NodeState::TimeWindow(TimeWindow::new(d)),
                    Window::Count(n) => NodeState::CountWindow(CountWindow::new(n)),
                },
            },
            Node::Min(input, window) => CompiledNode {
                id,
                op: NodeOp::Min(input),
                state: match window {
                    Window::Time(d) => NodeState::TimeWindow(TimeWindow::new(d)),
                    Window::Count(n) => NodeState::CountWindow(CountWindow::new(n)),
                },
            },
            Node::Sum(input, window) => CompiledNode {
                id,
                op: NodeOp::Sum(input),
                state: match window {
                    Window::Time(d) => NodeState::TimeWindow(TimeWindow::new(d)),
                    Window::Count(n) => NodeState::CountWindow(CountWindow::new(n)),
                },
            },
            Node::Count(input, window) => CompiledNode {
                id,
                op: NodeOp::Count(input),
                state: match window {
                    Window::Time(d) => NodeState::TimeWindow(TimeWindow::new(d)),
                    Window::Count(n) => NodeState::CountWindow(CountWindow::new(n)),
                },
            },
            Node::Prev(input) => CompiledNode {
                id,
                op: NodeOp::Prev(input),
                state: NodeState::Prev(PrevTracker::new()),
            },
            Node::PrevBy(input, key_fn) => CompiledNode {
                id,
                op: NodeOp::PrevBy(input, key_fn),
                state: NodeState::PrevBy(PrevByTracker::new()),
            },
            Node::Lag(input, duration) => CompiledNode {
                id,
                op: NodeOp::Lag(input),
                state: NodeState::Lag(LagBuffer::new(duration)),
            },
            Node::Delta(input, duration) => CompiledNode {
                id,
                op: NodeOp::Delta(input),
                state: NodeState::Lag(LagBuffer::new(duration)),
            },
            Node::Add(a, b) => CompiledNode {
                id,
                op: NodeOp::Add(a, b),
                state: NodeState::Stateless,
            },
            Node::Sub(a, b) => CompiledNode {
                id,
                op: NodeOp::Sub(a, b),
                state: NodeState::Stateless,
            },
            Node::Mul(a, b) => CompiledNode {
                id,
                op: NodeOp::Mul(a, b),
                state: NodeState::Stateless,
            },
            Node::Div(a, b) => CompiledNode {
                id,
                op: NodeOp::Div(a, b),
                state: NodeState::Stateless,
            },
            Node::MulConst(input, c) => CompiledNode {
                id,
                op: NodeOp::MulConst(input, c),
                state: NodeState::Stateless,
            },
            Node::AddConst(input, c) => CompiledNode {
                id,
                op: NodeOp::AddConst(input, c),
                state: NodeState::Stateless,
            },
            Node::Abs(input) => CompiledNode {
                id,
                op: NodeOp::Abs(input),
                state: NodeState::Stateless,
            },
            Node::Sqrt(input) => CompiledNode {
                id,
                op: NodeOp::Sqrt(input),
                state: NodeState::Stateless,
            },
            Node::Ln(input) => CompiledNode {
                id,
                op: NodeOp::Ln(input),
                state: NodeState::Stateless,
            },
            Node::Neg(input) => CompiledNode {
                id,
                op: NodeOp::Neg(input),
                state: NodeState::Stateless,
            },
            Node::Cross(a, b) => CompiledNode {
                id,
                op: NodeOp::Cross(a, b),
                state: NodeState::Cross(CrossDetector::new()),
            },
            Node::CrossAbove(a, b) => CompiledNode {
                id,
                op: NodeOp::CrossAbove(a, b),
                state: NodeState::Cross(CrossDetector::new()),
            },
            Node::CrossUnder(a, b) => CompiledNode {
                id,
                op: NodeOp::CrossUnder(a, b),
                state: NodeState::Cross(CrossDetector::new()),
            },
            Node::CrossHysteresis(a, b, margin) => CompiledNode {
                id,
                op: NodeOp::CrossHysteresis(a, b),
                state: NodeState::CrossHysteresis(HysteresisCrossDetector::new(margin)),
            },
            Node::Rate(input, _duration) => CompiledNode {
                id,
                op: NodeOp::Rate(input),
                state: NodeState::Rate {
                    prev_ts: None,
                    prev_value: None,
                },
            },
            Node::Velocity(input, _duration) => CompiledNode {
                id,
                op: NodeOp::Velocity(input),
                state: NodeState::Velocity {
                    prev_ts: None,
                    prev_value: None,
                },
            },
            Node::Acceleration(input, _duration) => CompiledNode {
                id,
                op: NodeOp::Acceleration(input),
                state: NodeState::Acceleration {
                    prev_ts: None,
                    prev_velocity: None,
                    velocity_state: Box::new(NodeState::Velocity {
                        prev_ts: None,
                        prev_value: None,
                    }),
                },
            },
            Node::Gt(a, b) => CompiledNode {
                id,
                op: NodeOp::Gt(a, b),
                state: NodeState::Stateless,
            },
            Node::Gte(a, b) => CompiledNode {
                id,
                op: NodeOp::Gte(a, b),
                state: NodeState::Stateless,
            },
            Node::Lt(a, b) => CompiledNode {
                id,
                op: NodeOp::Lt(a, b),
                state: NodeState::Stateless,
            },
            Node::Lte(a, b) => CompiledNode {
                id,
                op: NodeOp::Lte(a, b),
                state: NodeState::Stateless,
            },
            Node::Eq(a, b) => CompiledNode {
                id,
                op: NodeOp::Eq(a, b),
                state: NodeState::Stateless,
            },
            // Statistical
            Node::Median(input, window) => CompiledNode {
                id,
                op: NodeOp::Median(input),
                state: match window {
                    Window::Time(d) => NodeState::MedianTimeWindow(MedianTimeWindow::new(d)),
                    Window::Count(n) => NodeState::MedianCountWindow(MedianCountWindow::new(n)),
                },
            },
            Node::Quantile(input, window, q) => CompiledNode {
                id,
                op: NodeOp::Quantile(input, q),
                state: match window {
                    Window::Time(d) => NodeState::MedianTimeWindow(MedianTimeWindow::new(d)),
                    Window::Count(n) => NodeState::MedianCountWindow(MedianCountWindow::new(n)),
                },
            },
            Node::Correlation(a, b, window) => CompiledNode {
                id,
                op: NodeOp::Correlation(a, b),
                state: match window {
                    Window::Time(d) => {
                        NodeState::CorrelationTimeWindow(CorrelationTimeWindow::new(d))
                    }
                    Window::Count(n) => {
                        NodeState::CorrelationCountWindow(CorrelationCountWindow::new(n))
                    }
                },
            },
            Node::Covariance(a, b, window) => CompiledNode {
                id,
                op: NodeOp::Covariance(a, b),
                state: match window {
                    Window::Time(d) => {
                        NodeState::CorrelationTimeWindow(CorrelationTimeWindow::new(d))
                    }
                    Window::Count(n) => {
                        NodeState::CorrelationCountWindow(CorrelationCountWindow::new(n))
                    }
                },
            },
            Node::Skewness(input, window) => CompiledNode {
                id,
                op: NodeOp::Skewness(input),
                state: match window {
                    Window::Time(d) => NodeState::MomentsTimeWindow(MomentsTimeWindow::new(d)),
                    Window::Count(n) => NodeState::MomentsCountWindow(MomentsCountWindow::new(n)),
                },
            },
            Node::Kurtosis(input, window) => CompiledNode {
                id,
                op: NodeOp::Kurtosis(input),
                state: match window {
                    Window::Time(d) => NodeState::MomentsTimeWindow(MomentsTimeWindow::new(d)),
                    Window::Count(n) => NodeState::MomentsCountWindow(MomentsCountWindow::new(n)),
                },
            },
            Node::Rank(input, window) => CompiledNode {
                id,
                op: NodeOp::Rank(input),
                state: match window {
                    Window::Time(d) => NodeState::MedianTimeWindow(MedianTimeWindow::new(d)),
                    Window::Count(n) => NodeState::MedianCountWindow(MedianCountWindow::new(n)),
                },
            },
            // Moving averages
            Node::Wma(input, window) => CompiledNode {
                id,
                op: NodeOp::Wma(input),
                state: match window {
                    Window::Time(d) => NodeState::WmaTimeWindow(WmaTimeWindow::new(d)),
                    Window::Count(n) => NodeState::WmaCountWindow(WmaCountWindow::new(n)),
                },
            },
            // Momentum
            Node::Rsi(input, window) => CompiledNode {
                id,
                op: NodeOp::Rsi(input),
                state: match window {
                    Window::Time(d) => NodeState::RsiTimeWindow(RsiTimeWindow::new(d)),
                    Window::Count(n) => NodeState::RsiCountWindow(RsiCountWindow::new(n)),
                },
            },
            Node::RsiWilder(input, period) => CompiledNode {
                id,
                op: NodeOp::Rsi(input),
                state: NodeState::RsiWilderState(RsiWilderState::new(period)),
            },
            // Cumulative
            Node::CumSum(input) => CompiledNode {
                id,
                op: NodeOp::CumSum(input),
                state: NodeState::CumSum(CumulativeSum::new()),
            },
            Node::CumMax(input) => CompiledNode {
                id,
                op: NodeOp::CumMax(input),
                state: NodeState::CumMax(CumulativeMax::new()),
            },
            Node::CumMin(input) => CompiledNode {
                id,
                op: NodeOp::CumMin(input),
                state: NodeState::CumMin(CumulativeMin::new()),
            },
            Node::CumProd(input) => CompiledNode {
                id,
                op: NodeOp::CumProd(input),
                state: NodeState::CumProd(CumulativeProduct::new()),
            },
            // Returns
            Node::PctChange(input) => CompiledNode {
                id,
                op: NodeOp::PctChange(input),
                state: NodeState::PctChange { prev: None },
            },
            Node::LogReturn(input) => CompiledNode {
                id,
                op: NodeOp::LogReturn(input),
                state: NodeState::LogReturn { prev: None },
            },
            // Math functions
            Node::Pow(input, n) => CompiledNode {
                id,
                op: NodeOp::Pow(input, n),
                state: NodeState::Stateless,
            },
            Node::Exp(input) => CompiledNode {
                id,
                op: NodeOp::Exp(input),
                state: NodeState::Stateless,
            },
            Node::Log10(input) => CompiledNode {
                id,
                op: NodeOp::Log10(input),
                state: NodeState::Stateless,
            },
            Node::Log2(input) => CompiledNode {
                id,
                op: NodeOp::Log2(input),
                state: NodeState::Stateless,
            },
            Node::Clamp(input, min, max) => CompiledNode {
                id,
                op: NodeOp::Clamp(input, min, max),
                state: NodeState::Stateless,
            },
            Node::Floor(input) => CompiledNode {
                id,
                op: NodeOp::Floor(input),
                state: NodeState::Stateless,
            },
            Node::Ceil(input) => CompiledNode {
                id,
                op: NodeOp::Ceil(input),
                state: NodeState::Stateless,
            },
            Node::Round(input) => CompiledNode {
                id,
                op: NodeOp::Round(input),
                state: NodeState::Stateless,
            },
            Node::DivConst(input, c) => CompiledNode {
                id,
                op: NodeOp::DivConst(input, c),
                state: NodeState::Stateless,
            },
            // Trigger primitives
            Node::GlitchFilterNode(input, threshold, min_duration_ms) => CompiledNode {
                id,
                op: NodeOp::GlitchFilter(input),
                state: NodeState::GlitchFilterState(GlitchFilter::new(threshold, min_duration_ms)),
            },
            Node::RuntDetectNode(input, low, high) => CompiledNode {
                id,
                op: NodeOp::RuntDetect(input),
                state: NodeState::RuntDetectorState(RuntDetector::new(low, high)),
            },
            Node::PulseWidthNode(input, threshold, min_width_ms, max_width_ms) => CompiledNode {
                id,
                op: NodeOp::PulseWidth(input),
                state: NodeState::PulseWidthState(PulseWidthDetector::new(
                    threshold,
                    min_width_ms,
                    max_width_ms,
                )),
            },
            Node::WindowDetectNode(input, low, high) => CompiledNode {
                id,
                op: NodeOp::WindowDetect(input),
                state: NodeState::WindowDetectorState(WindowDetector::new(low, high)),
            },
            // Custom functional operators
            Node::MapF64 { input, f, .. } => CompiledNode {
                id,
                op: NodeOp::MapF64(input, f),
                state: NodeState::Stateless,
            },
            Node::Map2F64 { a, b, f, .. } => CompiledNode {
                id,
                op: NodeOp::Map2F64(a, b, f),
                state: NodeState::Stateless,
            },
            Node::FilterF64 {
                input, predicate, ..
            } => CompiledNode {
                id,
                op: NodeOp::FilterF64(input, predicate),
                state: NodeState::Stateless,
            },
            Node::FilterMapF64 { input, f, .. } => CompiledNode {
                id,
                op: NodeOp::FilterMapF64(input, f),
                state: NodeState::Stateless,
            },
            Node::ScanF64 {
                input, ctor, step, ..
            } => {
                let initial_state = ctor();
                CompiledNode {
                    id,
                    op: NodeOp::ScanF64(input, ctor, step),
                    state: NodeState::ScanState(initial_state),
                }
            }
            Node::Scan2F64 {
                a, b, ctor, step, ..
            } => {
                let initial_state = ctor();
                CompiledNode {
                    id,
                    op: NodeOp::Scan2F64(a, b, ctor, step),
                    state: NodeState::Scan2State(initial_state),
                }
            }
            // Plugin node: `factory()` builds a fresh instance so each
            // compiled graph (including per-key graphs) gets independent state.
            Node::Plugin { inputs, factory } => CompiledNode {
                id,
                op: NodeOp::Plugin { inputs },
                state: NodeState::Plugin(factory()),
            },
        }
    }
}

impl<R> Default for CompilationCtx<R> {
    fn default() -> Self {
        Self::new()
    }
}
