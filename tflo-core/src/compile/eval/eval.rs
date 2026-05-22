//! Node evaluation dispatch.
//!
//! [`eval_node`] is the top-level match that delegates each [`NodeOp`] variant
//! to the appropriate helper from `helpers.rs` or to an inline computation.

use crate::comp::NodeId;
use crate::compile::{CompiledGraph, CompiledNode, NodeOp, NodeState, Value, ValueStore};
use crate::event::ThresholdCrossEventMode;
use crate::pipeline::PipelineContext;
use crate::primitives::CrossDetector;

impl<R, O, C: PipelineContext> CompiledGraph<R, O, C> {
    /// Helper to get f64 from store, returning NaN if missing.
    #[inline]
    pub(super) fn get_f64(store: &ValueStore, id: &NodeId) -> f64 {
        store.get_f64(id).unwrap_or(f64::NAN)
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
            NodeOp::Sma(input) => Self::eval_windowed(
                store,
                &mut node.state,
                input,
                ts,
                |w| w.mean(),
                |w| w.mean(),
            ),
            NodeOp::Std(input) => {
                Self::eval_windowed(store, &mut node.state, input, ts, |w| w.std(), |w| w.std())
            }
            NodeOp::Variance(input) => Self::eval_windowed(
                store,
                &mut node.state,
                input,
                ts,
                |w| w.variance(),
                |w| w.variance(),
            ),
            NodeOp::Max(input) => {
                Self::eval_windowed(store, &mut node.state, input, ts, |w| w.max(), |w| w.max())
            }
            NodeOp::Min(input) => {
                Self::eval_windowed(store, &mut node.state, input, ts, |w| w.min(), |w| w.min())
            }
            NodeOp::Sum(input) => {
                Self::eval_windowed(store, &mut node.state, input, ts, |w| w.sum(), |w| w.sum())
            }
            NodeOp::Count(input) => Self::eval_windowed(
                store,
                &mut node.state,
                input,
                ts,
                |w| w.count() as f64,
                |w| w.count() as f64,
            ),

            // ---- EMA ----
            NodeOp::Ema(input) => {
                let v = Self::get_f64(store, input);
                let result = match &mut node.state {
                    NodeState::TimeEma(e) => e.push(ts, v),
                    NodeState::CountEma(e) => e.push(v),
                    _ => f64::NAN,
                };
                Value::from(result)
            }

            // ---- WMA / RSI ----
            NodeOp::Wma(input) => Self::eval_wma(store, &mut node.state, input, ts),
            NodeOp::Rsi(input) => Self::eval_rsi(store, &mut node.state, input, ts),

            // ---- Stateful trackers ----
            NodeOp::Prev(input) => Self::eval_prev(store, &mut node.state, input),
            NodeOp::PrevBy(input, key_fn) => {
                Self::eval_prev_by(store, &mut node.state, input, key_fn, record)
            }
            NodeOp::Lag(input) => Self::eval_lag(store, &mut node.state, input, ts),
            NodeOp::Delta(input) => Self::eval_delta(store, &mut node.state, input, ts),
            NodeOp::Rate(input) => Self::eval_rate_derivative(store, &mut node.state, input, ts),
            NodeOp::Velocity(input) => Self::eval_velocity(store, &mut node.state, input, ts),
            NodeOp::Acceleration(input) => {
                Self::eval_acceleration(store, &mut node.state, input, ts)
            }

            // ---- Cumulative ----
            NodeOp::CumSum(input) => Self::eval_cumulative(store, &mut node.state, input),
            NodeOp::CumMax(input) => Self::eval_cumulative(store, &mut node.state, input),
            NodeOp::CumMin(input) => Self::eval_cumulative(store, &mut node.state, input),
            NodeOp::CumProd(input) => Self::eval_cumulative(store, &mut node.state, input),

            // ---- Returns ----
            NodeOp::PctChange(input) => Self::eval_pct_change(store, &mut node.state, input),
            NodeOp::LogReturn(input) => Self::eval_log_return(store, &mut node.state, input),

            // ---- Stateless math (unary) ----
            NodeOp::Abs(input) => Value::from(Self::get_f64(store, input).abs()),
            NodeOp::Sqrt(input) => Value::from(Self::get_f64(store, input).sqrt()),
            NodeOp::Ln(input) => Value::from(Self::get_f64(store, input).ln()),
            NodeOp::Neg(input) => Value::from(-Self::get_f64(store, input)),
            NodeOp::Exp(input) => Value::from(Self::get_f64(store, input).exp()),
            NodeOp::Log10(input) => Value::from(Self::get_f64(store, input).log10()),
            NodeOp::Log2(input) => Value::from(Self::get_f64(store, input).log2()),
            NodeOp::Floor(input) => Value::from(Self::get_f64(store, input).floor()),
            NodeOp::Ceil(input) => Value::from(Self::get_f64(store, input).ceil()),
            NodeOp::Round(input) => Value::from(Self::get_f64(store, input).round()),
            NodeOp::Pow(input, n) => Value::from(Self::get_f64(store, input).powf(*n)),
            NodeOp::Clamp(input, min, max) => {
                Value::from(Self::get_f64(store, input).clamp(*min, *max))
            }

            // ---- Stateless math (binary) ----
            NodeOp::Add(a, b) => Value::from(Self::get_f64(store, a) + Self::get_f64(store, b)),
            NodeOp::Sub(a, b) => Value::from(Self::get_f64(store, a) - Self::get_f64(store, b)),
            NodeOp::Mul(a, b) => Value::from(Self::get_f64(store, a) * Self::get_f64(store, b)),
            NodeOp::Div(a, b) => {
                let vb = Self::get_f64(store, b);
                Value::from(if vb == 0.0 {
                    f64::NAN
                } else {
                    Self::get_f64(store, a) / vb
                })
            }
            NodeOp::MulConst(input, c) => Value::from(Self::get_f64(store, input) * c),
            NodeOp::AddConst(input, c) => Value::from(Self::get_f64(store, input) + c),
            NodeOp::DivConst(input, c) => {
                let v = Self::get_f64(store, input);
                Value::from(if *c == 0.0 { f64::NAN } else { v / c })
            }

            // ---- Comparisons ----
            NodeOp::Gt(a, b) => Value::from(if Self::get_f64(store, a) > Self::get_f64(store, b) {
                1.0
            } else {
                0.0
            }),
            NodeOp::Gte(a, b) => Value::from(if Self::get_f64(store, a) >= Self::get_f64(store, b) {
                1.0
            } else {
                0.0
            }),
            NodeOp::Lt(a, b) => Value::from(if Self::get_f64(store, a) < Self::get_f64(store, b) {
                1.0
            } else {
                0.0
            }),
            NodeOp::Lte(a, b) => Value::from(if Self::get_f64(store, a) <= Self::get_f64(store, b) {
                1.0
            } else {
                0.0
            }),
            NodeOp::Eq(a, b) => Value::from(
                if (Self::get_f64(store, a) - Self::get_f64(store, b)).abs() < f64::EPSILON {
                    1.0
                } else {
                    0.0
                },
            ),

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
                let va = Self::get_f64(store, a);
                let vb = Self::get_f64(store, b);
                let edge = match &mut node.state {
                    NodeState::CrossHysteresis(h) => h.update(va, vb),
                    _ => ThresholdCrossEventMode::None,
                };
                Value::from(edge)
            }

            // ---- Statistical ----
            NodeOp::Median(input) => Self::eval_median(
                store,
                &mut node.state,
                input,
                ts,
                |w| w.median(),
                |w| w.median(),
            ),
            NodeOp::Quantile(input, q) => Self::eval_median(
                store,
                &mut node.state,
                input,
                ts,
                |w| w.quantile(*q),
                |w| w.quantile(*q),
            ),
            NodeOp::Rank(input) => Self::eval_median(
                store,
                &mut node.state,
                input,
                ts,
                |w| w.current_rank(),
                |w| w.current_rank(),
            ),
            NodeOp::Correlation(a, b) => Self::eval_bivariate(
                store,
                &mut node.state,
                a,
                b,
                ts,
                |w| w.correlation(),
                |w| w.correlation(),
            ),
            NodeOp::Covariance(a, b) => Self::eval_bivariate(
                store,
                &mut node.state,
                a,
                b,
                ts,
                |w| w.covariance(),
                |w| w.covariance(),
            ),
            NodeOp::Skewness(input) => Self::eval_moments(
                store,
                &mut node.state,
                input,
                ts,
                |w| w.skewness(),
                |w| w.skewness(),
            ),
            NodeOp::Kurtosis(input) => Self::eval_moments(
                store,
                &mut node.state,
                input,
                ts,
                |w| w.kurtosis(),
                |w| w.kurtosis(),
            ),

            // ---- Trigger primitives ----
            NodeOp::GlitchFilter(input) => Self::eval_glitch(store, &mut node.state, input, ts),
            NodeOp::RuntDetect(input) => Self::eval_runt(store, &mut node.state, input),
            NodeOp::PulseWidth(input) => Self::eval_pulse_width(store, &mut node.state, input, ts),
            NodeOp::WindowDetect(input) => Self::eval_window_detect(store, &mut node.state, input),

            // ---- Custom functional operators ----
            NodeOp::MapF64(input, f) => {
                let v = Self::get_f64(store, input);
                Value::from(f(v))
            }
            NodeOp::Map2F64(a, b, f) => {
                let va = Self::get_f64(store, a);
                let vb = Self::get_f64(store, b);
                Value::from(f(va, vb))
            }
            NodeOp::FilterF64(input, f) => {
                let v = Self::get_f64(store, input);
                Value::from(if f(v) { v } else { f64::NAN })
            }
            NodeOp::FilterMapF64(input, f) => {
                let v = Self::get_f64(store, input);
                Value::from(f(v).unwrap_or(f64::NAN))
            }
            NodeOp::ScanF64(input, state_factory, step) => {
                let v = Self::get_f64(store, input);
                let result = match &mut node.state {
                    NodeState::ScanState(state) => step(state, v),
                    _ => {
                        let mut new_state = state_factory();
                        let result = step(&mut new_state, v);
                        node.state = NodeState::ScanState(new_state);
                        result
                    }
                };
                Value::from(result)
            }
            NodeOp::Scan2F64(a, b, state_factory, step) => {
                let va = Self::get_f64(store, a);
                let vb = Self::get_f64(store, b);
                let result = match &mut node.state {
                    NodeState::Scan2State(state) => step(state, va, vb),
                    _ => {
                        let mut new_state = state_factory();
                        let result = step(&mut new_state, va, vb);
                        node.state = NodeState::Scan2State(new_state);
                        result
                    }
                };
                Value::from(result)
            }

            // ---- Custom plugin nodes ----
            NodeOp::Custom { inputs } => {
                let values: Vec<f64> =
                    inputs.iter().map(|id| Self::get_f64(store, id)).collect();
                let result = match &mut node.state {
                    NodeState::Custom(n) => n.eval(&values),
                    _ => f64::NAN,
                };
                Value::from(result)
            }
        }
    }
}
