//! Node evaluation dispatch.
//!
//! [`eval_node`] is the top-level match that delegates each [`NodeOp`] variant
//! to the appropriate helper from `helpers.rs` or to an inline computation.
//!
//! Every `f64`-producing node threads a [`Computed`] — a finite value or a
//! typed [`Absent`] reason. Total math `.map`s the reason through; partial math
//! (`sqrt`/`ln`/`div`) turns a bad argument into a typed `Err`; binary ops
//! propagate the first absent input; stateful nodes skip their state update on
//! an absent input rather than advancing with a substitute value.

use crate::comp::NodeId;
use crate::compile::{
    Absent, CompiledGraph, CompiledNode, Computed, NodeOp, NodeState, Value, ValueStore,
    finite_or_warming,
};
use crate::event::ThresholdCrossEventMode;
use crate::pipeline::PipelineContext;
use crate::primitives::CrossDetector;

/// Apply a binary operation to two [`Computed`] inputs.
///
/// The first absent input short-circuits — `a`'s reason is preferred over
/// `b`'s when both are absent.
#[inline]
fn binary(a: Computed, b: Computed, f: impl FnOnce(f64, f64) -> Computed) -> Computed {
    f(a?, b?)
}

impl<R, O, C: PipelineContext> CompiledGraph<R, O, C> {
    /// Read a node's [`Computed`] output from the store.
    ///
    /// A node with no stored value (an input not evaluated this step) is
    /// treated as still warming up.
    #[inline]
    pub(super) fn get_computed(store: &ValueStore, id: &NodeId) -> Computed {
        store.get_computed(id).unwrap_or(Err(Absent::WarmingUp))
    }

    /// Evaluate a single compiled node against the current record.
    ///
    /// Dispatches to the appropriate stateless computation or stateful helper
    /// based on the node's operation variant.
    pub(super) fn eval_node(
        node: &mut CompiledNode<R>,
        record: &R,
        ts: i64,
        store: &ValueStore,
    ) -> Value {
        match &node.op {
            // ---- Sources ----
            NodeOp::Prop(f) => Value::from(f(record)),
            NodeOp::Const(v) => Value::from(*v),

            // ---- Windowed aggregations ----
            NodeOp::Sma(input) => Value::from(Self::eval_windowed(
                store,
                &mut node.state,
                input,
                ts,
                |w| w.mean(),
                |w| w.mean(),
            )),
            NodeOp::Std(input) => Value::from(Self::eval_windowed(
                store,
                &mut node.state,
                input,
                ts,
                |w| w.std(),
                |w| w.std(),
            )),
            NodeOp::Variance(input) => Value::from(Self::eval_windowed(
                store,
                &mut node.state,
                input,
                ts,
                |w| w.variance(),
                |w| w.variance(),
            )),
            NodeOp::Max(input) => Value::from(Self::eval_windowed(
                store,
                &mut node.state,
                input,
                ts,
                |w| w.max(),
                |w| w.max(),
            )),
            NodeOp::Min(input) => Value::from(Self::eval_windowed(
                store,
                &mut node.state,
                input,
                ts,
                |w| w.min(),
                |w| w.min(),
            )),
            NodeOp::Sum(input) => Value::from(Self::eval_windowed(
                store,
                &mut node.state,
                input,
                ts,
                |w| w.sum(),
                |w| w.sum(),
            )),
            NodeOp::Count(input) => Value::from(Self::eval_windowed(
                store,
                &mut node.state,
                input,
                ts,
                |w| w.count() as f64,
                |w| w.count() as f64,
            )),

            // ---- EMA ----
            NodeOp::Ema(input) => Value::from(match Self::get_computed(store, input) {
                Err(e) => Err(e),
                Ok(v) => match &mut node.state {
                    NodeState::TimeEma(e) => finite_or_warming(e.push(ts, v)),
                    NodeState::CountEma(e) => finite_or_warming(e.push(v)),
                    _ => Err(Absent::WarmingUp),
                },
            }),

            // ---- WMA / RSI ----
            NodeOp::Wma(input) => Value::from(Self::eval_wma(store, &mut node.state, input, ts)),
            NodeOp::Rsi(input) => Value::from(Self::eval_rsi(store, &mut node.state, input, ts)),

            // ---- Stateful trackers ----
            NodeOp::Prev(input) => Value::from(Self::eval_prev(store, &mut node.state, input)),
            NodeOp::PrevBy(input, key_fn) => Value::from(Self::eval_prev_by(
                store,
                &mut node.state,
                input,
                key_fn,
                record,
            )),
            NodeOp::Lag(input) => Value::from(Self::eval_lag(store, &mut node.state, input, ts)),
            NodeOp::Delta(input) => {
                Value::from(Self::eval_delta(store, &mut node.state, input, ts))
            }
            NodeOp::Rate(input) => Value::from(Self::eval_rate_derivative(
                store,
                &mut node.state,
                input,
                ts,
            )),
            NodeOp::Velocity(input) => {
                Value::from(Self::eval_velocity(store, &mut node.state, input, ts))
            }
            NodeOp::Acceleration(input) => {
                Value::from(Self::eval_acceleration(store, &mut node.state, input, ts))
            }

            // ---- Cumulative ----
            NodeOp::CumSum(input) => {
                Value::from(Self::eval_cumulative(store, &mut node.state, input))
            }
            NodeOp::CumMax(input) => {
                Value::from(Self::eval_cumulative(store, &mut node.state, input))
            }
            NodeOp::CumMin(input) => {
                Value::from(Self::eval_cumulative(store, &mut node.state, input))
            }
            NodeOp::CumProd(input) => {
                Value::from(Self::eval_cumulative(store, &mut node.state, input))
            }

            // ---- Returns ----
            NodeOp::PctChange(input) => {
                Value::from(Self::eval_pct_change(store, &mut node.state, input))
            }
            NodeOp::LogReturn(input) => {
                Value::from(Self::eval_log_return(store, &mut node.state, input))
            }

            // ---- Stateless math (unary) ----
            NodeOp::Abs(input) => Value::from(Self::get_computed(store, input).map(f64::abs)),
            NodeOp::Sqrt(input) => Value::from(Self::get_computed(store, input).and_then(|x| {
                if x < 0.0 {
                    Err(Absent::DomainError)
                } else {
                    Ok(x.sqrt())
                }
            })),
            NodeOp::Ln(input) => Value::from(Self::get_computed(store, input).and_then(|x| {
                if x <= 0.0 {
                    Err(Absent::DomainError)
                } else {
                    Ok(x.ln())
                }
            })),
            NodeOp::Neg(input) => Value::from(Self::get_computed(store, input).map(|x| -x)),
            NodeOp::Exp(input) => Value::from(Self::get_computed(store, input).map(f64::exp)),
            NodeOp::Log10(input) => Value::from(Self::get_computed(store, input).and_then(|x| {
                if x <= 0.0 {
                    Err(Absent::DomainError)
                } else {
                    Ok(x.log10())
                }
            })),
            NodeOp::Log2(input) => Value::from(Self::get_computed(store, input).and_then(|x| {
                if x <= 0.0 {
                    Err(Absent::DomainError)
                } else {
                    Ok(x.log2())
                }
            })),
            NodeOp::Floor(input) => Value::from(Self::get_computed(store, input).map(f64::floor)),
            NodeOp::Ceil(input) => Value::from(Self::get_computed(store, input).map(f64::ceil)),
            NodeOp::Round(input) => Value::from(Self::get_computed(store, input).map(f64::round)),
            NodeOp::Pow(input, n) => {
                Value::from(Self::get_computed(store, input).map(|x| x.powf(*n)))
            }
            NodeOp::Clamp(input, min, max) => {
                Value::from(Self::get_computed(store, input).map(|x| x.clamp(*min, *max)))
            }

            // ---- Stateless math (binary) ----
            NodeOp::Add(a, b) => Value::from(binary(
                Self::get_computed(store, a),
                Self::get_computed(store, b),
                |x, y| Ok(x + y),
            )),
            NodeOp::Sub(a, b) => Value::from(binary(
                Self::get_computed(store, a),
                Self::get_computed(store, b),
                |x, y| Ok(x - y),
            )),
            NodeOp::Mul(a, b) => Value::from(binary(
                Self::get_computed(store, a),
                Self::get_computed(store, b),
                |x, y| Ok(x * y),
            )),
            NodeOp::Div(a, b) => Value::from(binary(
                Self::get_computed(store, a),
                Self::get_computed(store, b),
                |x, y| {
                    if y == 0.0 {
                        Err(Absent::DivideByZero)
                    } else {
                        Ok(x / y)
                    }
                },
            )),
            NodeOp::MulConst(input, c) => {
                Value::from(Self::get_computed(store, input).map(|x| x * *c))
            }
            NodeOp::AddConst(input, c) => {
                Value::from(Self::get_computed(store, input).map(|x| x + *c))
            }
            NodeOp::DivConst(input, c) => {
                Value::from(Self::get_computed(store, input).and_then(|x| {
                    if *c == 0.0 {
                        Err(Absent::DivideByZero)
                    } else {
                        Ok(x / *c)
                    }
                }))
            }

            // ---- Comparisons ----
            NodeOp::Gt(a, b) => Value::from(binary(
                Self::get_computed(store, a),
                Self::get_computed(store, b),
                |x, y| Ok(if x > y { 1.0 } else { 0.0 }),
            )),
            NodeOp::Gte(a, b) => Value::from(binary(
                Self::get_computed(store, a),
                Self::get_computed(store, b),
                |x, y| Ok(if x >= y { 1.0 } else { 0.0 }),
            )),
            NodeOp::Lt(a, b) => Value::from(binary(
                Self::get_computed(store, a),
                Self::get_computed(store, b),
                |x, y| Ok(if x < y { 1.0 } else { 0.0 }),
            )),
            NodeOp::Lte(a, b) => Value::from(binary(
                Self::get_computed(store, a),
                Self::get_computed(store, b),
                |x, y| Ok(if x <= y { 1.0 } else { 0.0 }),
            )),
            NodeOp::Eq(a, b) => Value::from(binary(
                Self::get_computed(store, a),
                Self::get_computed(store, b),
                |x, y| {
                    Ok(if (x - y).abs() < f64::EPSILON {
                        1.0
                    } else {
                        0.0
                    })
                },
            )),

            // ---- Cross detection ----
            NodeOp::Cross(a, b) => {
                Self::eval_cross(store, &mut node.state, a, b, CrossDetector::update)
            }
            NodeOp::CrossAbove(a, b) => {
                Self::eval_cross(store, &mut node.state, a, b, CrossDetector::update_above)
            }
            NodeOp::CrossUnder(a, b) => {
                Self::eval_cross(store, &mut node.state, a, b, CrossDetector::update_below)
            }
            NodeOp::CrossHysteresis(a, b) => {
                let va = Self::get_computed(store, a).unwrap_or(f64::NAN);
                let vb = Self::get_computed(store, b).unwrap_or(f64::NAN);
                let edge = match &mut node.state {
                    NodeState::CrossHysteresis(h) => h.update(va, vb),
                    _ => ThresholdCrossEventMode::None,
                };
                Value::from(edge)
            }

            // ---- Statistical ----
            NodeOp::Median(input) => Value::from(Self::eval_median(
                store,
                &mut node.state,
                input,
                ts,
                |w| w.median(),
                |w| w.median(),
            )),
            NodeOp::Quantile(input, q) => Value::from(Self::eval_median(
                store,
                &mut node.state,
                input,
                ts,
                |w| w.quantile(*q),
                |w| w.quantile(*q),
            )),
            NodeOp::Rank(input) => Value::from(Self::eval_median(
                store,
                &mut node.state,
                input,
                ts,
                |w| w.current_rank(),
                |w| w.current_rank(),
            )),
            NodeOp::Correlation(a, b) => Value::from(Self::eval_bivariate(
                store,
                &mut node.state,
                a,
                b,
                ts,
                |w| w.correlation(),
                |w| w.correlation(),
            )),
            NodeOp::Covariance(a, b) => Value::from(Self::eval_bivariate(
                store,
                &mut node.state,
                a,
                b,
                ts,
                |w| w.covariance(),
                |w| w.covariance(),
            )),
            NodeOp::Skewness(input) => Value::from(Self::eval_moments(
                store,
                &mut node.state,
                input,
                ts,
                |w| w.skewness(),
                |w| w.skewness(),
            )),
            NodeOp::Kurtosis(input) => Value::from(Self::eval_moments(
                store,
                &mut node.state,
                input,
                ts,
                |w| w.kurtosis(),
                |w| w.kurtosis(),
            )),

            // ---- Trigger primitives ----
            NodeOp::GlitchFilter(input) => Self::eval_glitch(store, &mut node.state, input, ts),
            NodeOp::RuntDetect(input) => Self::eval_runt(store, &mut node.state, input),
            NodeOp::PulseWidth(input) => Self::eval_pulse_width(store, &mut node.state, input, ts),
            NodeOp::WindowDetect(input) => Self::eval_window_detect(store, &mut node.state, input),

            // ---- Custom functional operators ----
            NodeOp::MapF64(input, f) => Value::from(Self::get_computed(store, input).map(|x| f(x))),
            NodeOp::Map2F64(a, b, f) => Value::from(binary(
                Self::get_computed(store, a),
                Self::get_computed(store, b),
                |x, y| Ok(f(x, y)),
            )),
            NodeOp::FilterF64(input, f) => {
                Value::from(Self::get_computed(store, input).and_then(|x| {
                    if f(x) {
                        Ok(x)
                    } else {
                        Err(Absent::FilteredOut)
                    }
                }))
            }
            NodeOp::FilterMapF64(input, f) => Value::from(
                Self::get_computed(store, input).and_then(|x| f(x).ok_or(Absent::FilteredOut)),
            ),
            NodeOp::ScanF64(input, state_factory, step) => {
                // A scan does not advance its state on an absent input — it
                // propagates the reason and leaves the accumulator untouched.
                Value::from(match Self::get_computed(store, input) {
                    Err(e) => Err(e),
                    Ok(v) => match &mut node.state {
                        NodeState::ScanState(state) => step(state, v),
                        _ => {
                            let mut new_state = state_factory();
                            let result = step(&mut new_state, v);
                            node.state = NodeState::ScanState(new_state);
                            result
                        }
                    },
                })
            }
            NodeOp::Scan2F64(a, b, state_factory, step) => Value::from(
                match (Self::get_computed(store, a), Self::get_computed(store, b)) {
                    (Err(e), _) | (Ok(_), Err(e)) => Err(e),
                    (Ok(va), Ok(vb)) => match &mut node.state {
                        NodeState::Scan2State(state) => step(state, va, vb),
                        _ => {
                            let mut new_state = state_factory();
                            let result = step(&mut new_state, va, vb);
                            node.state = NodeState::Scan2State(new_state);
                            result
                        }
                    },
                },
            ),

            // ---- Custom plugin nodes ----
            NodeOp::Custom { inputs } => {
                let values: Vec<Computed> = inputs
                    .iter()
                    .map(|id| Self::get_computed(store, id))
                    .collect();
                Value::from(match &mut node.state {
                    NodeState::Custom(n) => n.eval(&values),
                    _ => Err(Absent::WarmingUp),
                })
            }
        }
    }
}
