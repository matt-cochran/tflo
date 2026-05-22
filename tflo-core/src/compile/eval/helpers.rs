//! Evaluation helper methods for compiled graph nodes.
//!
//! These are the stateful evaluation helpers called from `eval_node()` in the
//! parent module. Each helper handles a category of node operations:
//! windowed aggregations, WMA/RSI, lookback trackers, cumulative ops,
//! returns, cross detection, statistical windows, and trigger primitives.
//!
//! The `f64`-producing helpers return a [`Computed`]: they `?`-propagate an
//! absent input (skipping any state update) and route a primitive's internal
//! `NaN` "insufficient data" sentinel through [`finite_or_warming`]. The
//! event-producing helpers ([`eval_cross`](CompiledGraph::eval_cross) and the
//! trigger detectors) keep their historical behaviour — an absent input is
//! fed to the detector as `NaN`.

use crate::comp::NodeId;
use crate::compile::{
    Absent, CompiledGraph, Computed, NodeOutput, NodeState, RsiWilderState, ValueStore,
    finite_or_warming,
};
use crate::event::ThresholdCrossEventMode;
use crate::pipeline::PipelineContext;
use crate::primitives::{
    CorrelationCountWindow, CorrelationTimeWindow, CountWindow, CrossDetector, GlitchResult,
    MedianCountWindow, MedianTimeWindow, MomentsCountWindow, MomentsTimeWindow, PulseWidthResult,
    RuntResult, TimeWindow, WindowEvent,
};
use std::sync::Arc;

impl<R, O, C: PipelineContext> CompiledGraph<R, O, C> {
    // ---- Windowed aggregation helper ----

    pub(super) fn eval_windowed(
        store: &ValueStore,
        state: &mut NodeState,
        input: &NodeId,
        ts: i64,
        time_fn: impl FnOnce(&mut TimeWindow) -> f64,
        count_fn: impl FnOnce(&mut CountWindow) -> f64,
    ) -> Computed {
        let v = Self::get_computed(store, input)?;
        match state {
            NodeState::TimeWindow(w) => {
                w.push(ts, v);
                finite_or_warming(time_fn(w))
            }
            NodeState::CountWindow(w) => {
                w.push(v);
                finite_or_warming(count_fn(w))
            }
            _ => Err(Absent::WarmingUp),
        }
    }

    // ---- WMA / RSI helpers ----

    pub(super) fn eval_wma(
        store: &ValueStore,
        state: &mut NodeState,
        input: &NodeId,
        ts: i64,
    ) -> Computed {
        let v = Self::get_computed(store, input)?;
        match state {
            NodeState::WmaTimeWindow(w) => {
                w.push(ts, v);
                finite_or_warming(w.wma())
            }
            NodeState::WmaCountWindow(w) => {
                w.push(v);
                finite_or_warming(w.wma())
            }
            _ => Err(Absent::WarmingUp),
        }
    }

    pub(super) fn eval_rsi(
        store: &ValueStore,
        state: &mut NodeState,
        input: &NodeId,
        ts: i64,
    ) -> Computed {
        let v = Self::get_computed(store, input)?;
        match state {
            NodeState::RsiTimeWindow(w) => {
                w.push(ts, v);
                finite_or_warming(w.rsi())
            }
            NodeState::RsiCountWindow(w) => {
                w.push(v);
                finite_or_warming(w.rsi())
            }
            NodeState::RsiWilderState(s) => Self::compute_rsi_wilder(s, v),
            _ => Err(Absent::WarmingUp),
        }
    }

    fn compute_rsi_wilder(state: &mut RsiWilderState, value: f64) -> Computed {
        if state.period == 0 {
            return Err(Absent::InvalidConfig);
        }

        let Some(prev) = state.prev else {
            state.prev = Some(value);
            return Err(Absent::WarmingUp);
        };

        let change = value - prev;
        let gain = if change > 0.0 { change } else { 0.0 };
        let loss = if change < 0.0 { -change } else { 0.0 };
        state.prev = Some(value);

        if !state.initialized {
            state.count += 1;
            state.sum_gain += gain;
            state.sum_loss += loss;
            if state.count < state.period {
                return Err(Absent::WarmingUp);
            }
            state.avg_gain = state.sum_gain / state.period as f64;
            state.avg_loss = state.sum_loss / state.period as f64;
            state.initialized = true;
        } else {
            state.avg_gain =
                (state.avg_gain * (state.period - 1) as f64 + gain) / state.period as f64;
            state.avg_loss =
                (state.avg_loss * (state.period - 1) as f64 + loss) / state.period as f64;
        }

        Ok(if state.avg_loss == 0.0 {
            if state.avg_gain == 0.0 { 50.0 } else { 100.0 }
        } else {
            100.0 - 100.0 / (1.0 + state.avg_gain / state.avg_loss)
        })
    }

    // ---- Stateful tracker helpers ----

    pub(super) fn eval_prev(store: &ValueStore, state: &mut NodeState, input: &NodeId) -> Computed {
        let v = Self::get_computed(store, input)?;
        match state {
            NodeState::Prev(p) => p.update(v).ok_or(Absent::WarmingUp),
            _ => Err(Absent::WarmingUp),
        }
    }

    pub(super) fn eval_prev_by(
        store: &ValueStore,
        state: &mut NodeState,
        input: &NodeId,
        key_fn: &Arc<dyn Fn(&R) -> u64 + Send + Sync>,
        record: &R,
    ) -> Computed {
        let v = Self::get_computed(store, input)?;
        let key = key_fn(record);
        match state {
            NodeState::PrevBy(p) => p.update(key, v).ok_or(Absent::WarmingUp),
            _ => Err(Absent::WarmingUp),
        }
    }

    pub(super) fn eval_lag(
        store: &ValueStore,
        state: &mut NodeState,
        input: &NodeId,
        ts: i64,
    ) -> Computed {
        let v = Self::get_computed(store, input)?;
        match state {
            NodeState::Lag(l) => l.push(ts, v).ok_or(Absent::WarmingUp),
            _ => Err(Absent::WarmingUp),
        }
    }

    pub(super) fn eval_delta(
        store: &ValueStore,
        state: &mut NodeState,
        input: &NodeId,
        ts: i64,
    ) -> Computed {
        let v = Self::get_computed(store, input)?;
        match state {
            NodeState::Lag(l) => l.push(ts, v).map(|lag| v - lag).ok_or(Absent::WarmingUp),
            _ => Err(Absent::WarmingUp),
        }
    }

    pub(super) fn eval_rate_derivative(
        store: &ValueStore,
        state: &mut NodeState,
        input: &NodeId,
        ts: i64,
    ) -> Computed {
        let v = Self::get_computed(store, input)?;
        match state {
            NodeState::Rate {
                prev_ts,
                prev_value,
            } => {
                let rate = match (*prev_ts, *prev_value) {
                    (Some(pt), Some(pv)) => {
                        let dt = (ts - pt) as f64;
                        if dt > 0.0 {
                            Ok((v - pv) / dt * 1000.0)
                        } else {
                            Err(Absent::ZeroTimeDelta)
                        }
                    }
                    _ => Err(Absent::WarmingUp),
                };
                *prev_ts = Some(ts);
                *prev_value = Some(v);
                rate
            }
            _ => Err(Absent::WarmingUp),
        }
    }

    pub(super) fn eval_velocity(
        store: &ValueStore,
        state: &mut NodeState,
        input: &NodeId,
        ts: i64,
    ) -> Computed {
        let v = Self::get_computed(store, input)?;
        match state {
            NodeState::Velocity {
                prev_ts,
                prev_value,
            } => {
                let velocity = match (*prev_ts, *prev_value) {
                    (Some(pt), Some(pv)) => {
                        let dt = (ts - pt) as f64;
                        if dt > 0.0 {
                            Ok((v - pv) / dt * 1000.0)
                        } else {
                            Err(Absent::ZeroTimeDelta)
                        }
                    }
                    _ => Err(Absent::WarmingUp),
                };
                *prev_ts = Some(ts);
                *prev_value = Some(v);
                velocity
            }
            _ => Err(Absent::WarmingUp),
        }
    }

    pub(super) fn eval_acceleration(
        store: &ValueStore,
        state: &mut NodeState,
        input: &NodeId,
        ts: i64,
    ) -> Computed {
        let v = Self::get_computed(store, input)?;
        match state {
            NodeState::Acceleration {
                prev_ts,
                prev_velocity,
                velocity_state,
            } => {
                // The inner velocity keeps its internal `NaN` sentinel — it is
                // a private intermediate, never observed outside this helper.
                let current_velocity = match velocity_state.as_mut() {
                    NodeState::Velocity {
                        prev_ts: vel_ts,
                        prev_value: vel_val,
                    } => {
                        let vel = match (*vel_ts, *vel_val) {
                            (Some(pt), Some(pv)) => {
                                let dt = (ts - pt) as f64;
                                if dt > 0.0 {
                                    (v - pv) / dt * 1000.0
                                } else {
                                    f64::NAN
                                }
                            }
                            _ => f64::NAN,
                        };
                        *vel_ts = Some(ts);
                        *vel_val = Some(v);
                        vel
                    }
                    _ => f64::NAN,
                };
                let accel = match (*prev_ts, *prev_velocity) {
                    (Some(pt), Some(pv)) if !current_velocity.is_nan() => {
                        let dt = (ts - pt) as f64;
                        if dt > 0.0 {
                            Ok((current_velocity - pv) / dt * 1000.0)
                        } else {
                            Err(Absent::ZeroTimeDelta)
                        }
                    }
                    _ => Err(Absent::WarmingUp),
                };
                *prev_ts = Some(ts);
                if !current_velocity.is_nan() {
                    *prev_velocity = Some(current_velocity);
                }
                accel
            }
            _ => Err(Absent::WarmingUp),
        }
    }

    // ---- Cumulative helpers ----

    pub(super) fn eval_cumulative(
        store: &ValueStore,
        state: &mut NodeState,
        input: &NodeId,
    ) -> Computed {
        let v = Self::get_computed(store, input)?;
        match state {
            NodeState::CumSum(c) => Ok(c.push(v)),
            NodeState::CumMax(c) => Ok(c.push(v)),
            NodeState::CumMin(c) => Ok(c.push(v)),
            NodeState::CumProd(c) => Ok(c.push(v)),
            _ => Err(Absent::WarmingUp),
        }
    }

    // ---- Returns helpers ----

    pub(super) fn eval_pct_change(
        store: &ValueStore,
        state: &mut NodeState,
        input: &NodeId,
    ) -> Computed {
        let v = Self::get_computed(store, input)?;
        match state {
            NodeState::PctChange { prev } => {
                let pct = match *prev {
                    Some(p) if p != 0.0 => Ok((v - p) / p * 100.0),
                    Some(_) => Err(Absent::DivideByZero),
                    None => Err(Absent::WarmingUp),
                };
                *prev = Some(v);
                pct
            }
            _ => Err(Absent::WarmingUp),
        }
    }

    pub(super) fn eval_log_return(
        store: &ValueStore,
        state: &mut NodeState,
        input: &NodeId,
    ) -> Computed {
        let v = Self::get_computed(store, input)?;
        match state {
            NodeState::LogReturn { prev } => {
                let ret = match *prev {
                    Some(p) if p > 0.0 && v > 0.0 => Ok((v / p).ln()),
                    Some(_) => Err(Absent::DomainError),
                    None => Err(Absent::WarmingUp),
                };
                *prev = Some(v);
                ret
            }
            _ => Err(Absent::WarmingUp),
        }
    }

    // ---- Cross detection helper ----

    pub(super) fn eval_cross(
        store: &ValueStore,
        state: &mut NodeState,
        a: &NodeId,
        b: &NodeId,
        update_fn: fn(&mut CrossDetector, f64, f64) -> ThresholdCrossEventMode,
    ) -> NodeOutput {
        let va = Self::get_computed(store, a).unwrap_or(f64::NAN);
        let vb = Self::get_computed(store, b).unwrap_or(f64::NAN);
        let edge = match state {
            NodeState::Cross(c) => update_fn(c, va, vb),
            _ => ThresholdCrossEventMode::None,
        };
        NodeOutput::other(edge)
    }

    // ---- Statistical helpers ----

    pub(super) fn eval_median(
        store: &ValueStore,
        state: &mut NodeState,
        input: &NodeId,
        ts: i64,
        time_fn: impl FnOnce(&mut MedianTimeWindow) -> f64,
        count_fn: impl FnOnce(&mut MedianCountWindow) -> f64,
    ) -> Computed {
        let v = Self::get_computed(store, input)?;
        match state {
            NodeState::MedianTimeWindow(w) => {
                w.push(ts, v);
                finite_or_warming(time_fn(w))
            }
            NodeState::MedianCountWindow(w) => {
                w.push(v);
                finite_or_warming(count_fn(w))
            }
            _ => Err(Absent::WarmingUp),
        }
    }

    pub(super) fn eval_bivariate(
        store: &ValueStore,
        state: &mut NodeState,
        a: &NodeId,
        b: &NodeId,
        ts: i64,
        time_fn: impl FnOnce(&mut CorrelationTimeWindow) -> f64,
        count_fn: impl FnOnce(&mut CorrelationCountWindow) -> f64,
    ) -> Computed {
        let va = Self::get_computed(store, a)?;
        let vb = Self::get_computed(store, b)?;
        match state {
            NodeState::CorrelationTimeWindow(w) => {
                w.push(ts, va, vb);
                finite_or_warming(time_fn(w))
            }
            NodeState::CorrelationCountWindow(w) => {
                w.push(va, vb);
                finite_or_warming(count_fn(w))
            }
            _ => Err(Absent::WarmingUp),
        }
    }

    pub(super) fn eval_moments(
        store: &ValueStore,
        state: &mut NodeState,
        input: &NodeId,
        ts: i64,
        time_fn: impl FnOnce(&mut MomentsTimeWindow) -> f64,
        count_fn: impl FnOnce(&mut MomentsCountWindow) -> f64,
    ) -> Computed {
        let v = Self::get_computed(store, input)?;
        match state {
            NodeState::MomentsTimeWindow(w) => {
                w.push(ts, v);
                finite_or_warming(time_fn(w))
            }
            NodeState::MomentsCountWindow(w) => {
                w.push(v);
                finite_or_warming(count_fn(w))
            }
            _ => Err(Absent::WarmingUp),
        }
    }

    // ---- Trigger helpers ----

    pub(super) fn eval_glitch(
        store: &ValueStore,
        state: &mut NodeState,
        input: &NodeId,
        ts: i64,
    ) -> NodeOutput {
        let v = Self::get_computed(store, input).unwrap_or(f64::NAN);
        let result: GlitchResult = match state {
            NodeState::GlitchFilterState(f) => match f.update(v, ts) {
                Some(true) => GlitchResult::ValidPulse,
                Some(false) => GlitchResult::Rejected,
                None => GlitchResult::NoTransition,
            },
            _ => GlitchResult::NoTransition,
        };
        NodeOutput::other(result)
    }

    pub(super) fn eval_runt(
        store: &ValueStore,
        state: &mut NodeState,
        input: &NodeId,
    ) -> NodeOutput {
        let v = Self::get_computed(store, input).unwrap_or(f64::NAN);
        let result: Option<RuntResult> = match state {
            NodeState::RuntDetectorState(d) => d.update(v),
            _ => None,
        };
        NodeOutput::other(result)
    }

    pub(super) fn eval_pulse_width(
        store: &ValueStore,
        state: &mut NodeState,
        input: &NodeId,
        ts: i64,
    ) -> NodeOutput {
        let v = Self::get_computed(store, input).unwrap_or(f64::NAN);
        let result: Option<PulseWidthResult> = match state {
            NodeState::PulseWidthState(d) => d.update(v, ts),
            _ => None,
        };
        NodeOutput::other(result)
    }

    pub(super) fn eval_window_detect(
        store: &ValueStore,
        state: &mut NodeState,
        input: &NodeId,
    ) -> NodeOutput {
        let v = Self::get_computed(store, input).unwrap_or(f64::NAN);
        let result: Option<WindowEvent> = match state {
            NodeState::WindowDetectorState(d) => d.update(v),
            _ => None,
        };
        NodeOutput::other(result)
    }
}
