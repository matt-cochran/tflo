//! Stateful-tracker operators and the [`StatefulOps`] extension trait.
//!
//! These operators fold each record's value into a small piece of running
//! state: previous values, lag buffers, cumulative aggregates, returns, and
//! rate-based derivatives. Every one is a [`StatefulTracker<S, Step>`] — a
//! serialized state `S` paired with a zero-sized [`TrackStep<S>`] step.
//!
//! The step functions are ported verbatim from the legacy `tflo-core` catalog
//! (`compile/eval/helpers.rs`), including the typed [`Absent`] reasons the
//! hardening pass introduced: a `dt <= 0` derivative yields
//! [`Absent::ZeroTimeDelta`], a `pct_change` from a zero base yields
//! [`Absent::DivideByZero`], a `log_return` from a non-positive base yields
//! [`Absent::DomainError`], and every warm-up step yields
//! [`Absent::WarmingUp`]. Results are bit-identical to the old catalog.
//!
//! Every method is exposed on `Comp<R, f64>` through the single [`StatefulOps`]
//! extension trait, mirroring [`WindowOps`](crate::ops::windows::WindowOps).
//!
//! # `prev_by`
//!
//! `prev_by` partitions previous values by a key extracted from the *record*.
//! [`Operator::eval`](tflo_core::operator::Operator::eval) never receives the
//! record — only resolved `f64` inputs — so the key is supplied as a *second*
//! graph input: [`StatefulOps::prev_by`] wires a hidden record-extraction
//! source node via [`Comp::prop_from_record`] and feeds it alongside the value
//! into the 2-input [`PrevByOp`].

use crate::checkpoint;
use crate::shapes::{StatefulTracker, TrackStep};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::Duration;
use tflo_core::comp::Comp;
use tflo_core::compile::{Absent, Computed, NodeOutput};
use tflo_core::operator::{Operator, OperatorLoadError, require};
use tflo_core::primitives::{
    CumulativeMax, CumulativeMin, CumulativeProduct, CumulativeSum, LagBuffer, PrevTracker,
};

// ============================================================================
// State types
// ============================================================================

/// Running `(prev_ts, prev_value)` pair for the rate / velocity derivatives.
///
/// Both fields are `None` until the first record arrives, so the first step
/// always reports [`Absent::WarmingUp`].
#[derive(Default, Serialize, Deserialize)]
pub struct DerivativeState {
    prev_ts: Option<i64>,
    prev_value: Option<f64>,
}

/// Running state for the second derivative (acceleration).
///
/// `inner` keeps the velocity tracker; `prev_ts`/`prev_velocity` keep the
/// previous *velocity* sample so its rate of change can be taken.
#[derive(Default, Serialize, Deserialize)]
pub struct AccelerationState {
    prev_ts: Option<i64>,
    prev_velocity: Option<f64>,
    inner: DerivativeState,
}

/// Running previous value for [`pct_change`](StatefulOps::pct_change).
#[derive(Default, Serialize, Deserialize)]
pub struct PctChangeState {
    prev: Option<f64>,
}

/// Running previous value for [`log_return`](StatefulOps::log_return).
#[derive(Default, Serialize, Deserialize)]
pub struct LogReturnState {
    prev: Option<f64>,
}

// ============================================================================
// Step units (zero-sized, `#[serde(skip)]`-ped inside `StatefulTracker`)
// ============================================================================

/// Previous value — ported from `eval_prev`.
#[derive(Default)]
pub struct PrevStep;

impl TrackStep<PrevTracker> for PrevStep {
    fn step(&self, state: &mut PrevTracker, value: f64, _ts: i64) -> Computed {
        state.update(value).ok_or(Absent::WarmingUp)
    }
}

/// Value from a fixed duration ago — ported from `eval_lag`.
#[derive(Default)]
pub struct LagStep;

impl TrackStep<LagBuffer> for LagStep {
    fn step(&self, state: &mut LagBuffer, value: f64, ts: i64) -> Computed {
        state.push(ts, value).ok_or(Absent::WarmingUp)
    }
}

/// Current value minus the value from a fixed duration ago — ported from
/// `eval_delta`.
#[derive(Default)]
pub struct DeltaStep;

impl TrackStep<LagBuffer> for DeltaStep {
    fn step(&self, state: &mut LagBuffer, value: f64, ts: i64) -> Computed {
        state
            .push(ts, value)
            .map(|lag| value - lag)
            .ok_or(Absent::WarmingUp)
    }
}

/// Rate of change per unit time — ported from `eval_rate_derivative`.
///
/// The legacy catalog scales by `1000.0` (the value change per second when
/// timestamps are milliseconds). `dt <= 0` is [`Absent::ZeroTimeDelta`]; the
/// first record is [`Absent::WarmingUp`].
#[derive(Default)]
pub struct RateStep;

impl TrackStep<DerivativeState> for RateStep {
    fn step(&self, state: &mut DerivativeState, value: f64, ts: i64) -> Computed {
        let result = derivative(state.prev_ts, state.prev_value, value, ts);
        state.prev_ts = Some(ts);
        state.prev_value = Some(value);
        result
    }
}

/// First derivative (velocity) — identical step logic to [`RateStep`], ported
/// from `eval_velocity`.
#[derive(Default)]
pub struct VelocityStep;

impl TrackStep<DerivativeState> for VelocityStep {
    fn step(&self, state: &mut DerivativeState, value: f64, ts: i64) -> Computed {
        let result = derivative(state.prev_ts, state.prev_value, value, ts);
        state.prev_ts = Some(ts);
        state.prev_value = Some(value);
        result
    }
}

/// Second derivative (acceleration) — ported from `eval_acceleration`.
///
/// The inner velocity keeps its own `NaN` "no sample" sentinel — it is a
/// private intermediate, never observed outside this step, exactly as the
/// legacy helper did.
#[derive(Default)]
pub struct AccelerationStep;

impl TrackStep<AccelerationState> for AccelerationStep {
    fn step(&self, state: &mut AccelerationState, value: f64, ts: i64) -> Computed {
        // Inner velocity: a private `NaN` sentinel marks "no sample yet".
        let current_velocity = match (state.inner.prev_ts, state.inner.prev_value) {
            (Some(pt), Some(pv)) => {
                let dt = (ts - pt) as f64;
                if dt > 0.0 {
                    (value - pv) / dt * 1000.0
                } else {
                    f64::NAN
                }
            }
            _ => f64::NAN,
        };
        state.inner.prev_ts = Some(ts);
        state.inner.prev_value = Some(value);

        let accel = match (state.prev_ts, state.prev_velocity) {
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
        state.prev_ts = Some(ts);
        if !current_velocity.is_nan() {
            state.prev_velocity = Some(current_velocity);
        }
        accel
    }
}

/// Shared rate / velocity derivative: `(value - prev) / dt * 1000`.
///
/// `dt <= 0` is [`Absent::ZeroTimeDelta`]; an absent previous sample is
/// [`Absent::WarmingUp`].
fn derivative(prev_ts: Option<i64>, prev_value: Option<f64>, value: f64, ts: i64) -> Computed {
    match (prev_ts, prev_value) {
        (Some(pt), Some(pv)) => {
            let dt = (ts - pt) as f64;
            if dt > 0.0 {
                Ok((value - pv) / dt * 1000.0)
            } else {
                Err(Absent::ZeroTimeDelta)
            }
        }
        _ => Err(Absent::WarmingUp),
    }
}

/// Cumulative sum — ported from `eval_cumulative` (`CumSum` arm).
#[derive(Default)]
pub struct CumSumStep;

impl TrackStep<CumulativeSum> for CumSumStep {
    fn step(&self, state: &mut CumulativeSum, value: f64, _ts: i64) -> Computed {
        Ok(state.push(value))
    }
}

/// Cumulative maximum — ported from `eval_cumulative` (`CumMax` arm).
#[derive(Default)]
pub struct CumMaxStep;

impl TrackStep<CumulativeMax> for CumMaxStep {
    fn step(&self, state: &mut CumulativeMax, value: f64, _ts: i64) -> Computed {
        Ok(state.push(value))
    }
}

/// Cumulative minimum — ported from `eval_cumulative` (`CumMin` arm).
#[derive(Default)]
pub struct CumMinStep;

impl TrackStep<CumulativeMin> for CumMinStep {
    fn step(&self, state: &mut CumulativeMin, value: f64, _ts: i64) -> Computed {
        Ok(state.push(value))
    }
}

/// Cumulative product — ported from `eval_cumulative` (`CumProd` arm).
#[derive(Default)]
pub struct CumProdStep;

impl TrackStep<CumulativeProduct> for CumProdStep {
    fn step(&self, state: &mut CumulativeProduct, value: f64, _ts: i64) -> Computed {
        Ok(state.push(value))
    }
}

/// Percentage change from the previous value — ported from `eval_pct_change`.
///
/// `(current - prev) / prev * 100`. A zero previous value is
/// [`Absent::DivideByZero`]; the first record is [`Absent::WarmingUp`].
#[derive(Default)]
pub struct PctChangeStep;

impl TrackStep<PctChangeState> for PctChangeStep {
    fn step(&self, state: &mut PctChangeState, value: f64, _ts: i64) -> Computed {
        let pct = match state.prev {
            Some(p) if p != 0.0 => Ok((value - p) / p * 100.0),
            Some(_) => Err(Absent::DivideByZero),
            None => Err(Absent::WarmingUp),
        };
        state.prev = Some(value);
        pct
    }
}

/// Log return from the previous value — ported from `eval_log_return`.
///
/// `ln(current / prev)`. A non-positive `prev` or `current` is
/// [`Absent::DomainError`]; the first record is [`Absent::WarmingUp`].
#[derive(Default)]
pub struct LogReturnStep;

impl TrackStep<LogReturnState> for LogReturnStep {
    fn step(&self, state: &mut LogReturnState, value: f64, _ts: i64) -> Computed {
        let ret = match state.prev {
            Some(p) if p > 0.0 && value > 0.0 => Ok((value / p).ln()),
            Some(_) => Err(Absent::DomainError),
            None => Err(Absent::WarmingUp),
        };
        state.prev = Some(value);
        ret
    }
}

// ============================================================================
// prev_by — partitioned previous value (2-input operator)
// ============================================================================

/// Encode an `f64` partition key to a stable map key, collapsing `-0.0`
/// and `+0.0` to the same bucket.
///
/// IEEE 754 defines `-0.0 == +0.0` but the two values have different bit
/// patterns, so a naïve `to_bits()` call would create two distinct partitions
/// for what is logically the same key.  This helper normalises both zeros to
/// `+0.0` before encoding.
#[inline]
fn key_bits(k: f64) -> u64 {
    (if k == 0.0 { 0.0_f64 } else { k }).to_bits()
}

/// Previous value partitioned by a per-record key — ported from `eval_prev_by`.
///
/// Unlike the single-state trackers above, `prev_by` is a 2-input
/// [`Operator`]: `inputs[0]` is the value and `inputs[1]` is the partition key.
/// The key is produced by a hidden record-extraction source node (see
/// [`StatefulOps::prev_by`]).
///
/// The legacy `tflo-core` `prev_by` keyed its `HashMap` on a `u64` hash of the
/// caller's key type. Here the key arrives as the `f64` of a graph input, so it
/// is keyed via [`key_bits`], which encodes the raw bit pattern after normalising
/// `-0.0` to `+0.0`. This is exact for any `f64` key, including integer-valued
/// keys.
///
/// # 2^53 exact-integer caveat
///
/// Callers typically map a discrete key (a symbol id, sensor id, …) to an
/// `f64`. An `f64` represents every integer up to `2^53` exactly, so distinct
/// integer keys below that bound never collide. Beyond `2^53`, distinct
/// integers can round to the same `f64` and would then share a partition —
/// keep key magnitudes under `2^53`.
///
/// # NaN keys
///
/// A `NaN` key passes through [`key_bits`] as a valid `u64` and creates its own
/// partition.  Because `NaN != NaN` in IEEE 754, this partition can only ever be
/// populated once (the second `NaN`-keyed record would see `WarmingUp` again).
/// Callers should ensure the key-extraction closure never yields `NaN`.
#[derive(Default, Serialize, Deserialize)]
pub struct PrevByOp {
    /// Previous value per key, keyed on the `f64` key's raw bits.
    prev: HashMap<u64, f64>,
}

impl Operator for PrevByOp {
    fn eval(&mut self, inputs: &[Computed], _ts: i64) -> NodeOutput {
        // An absent value or an absent key skips the step entirely — the map is
        // not mutated — matching the legacy `eval_prev_by` (`get_computed(..)?`
        // on the value, and the key node resolving before the record is read).
        let value = match require(inputs, 0) {
            Ok(v) => v,
            Err(e) => return NodeOutput::computed(Err(e)),
        };
        let key = match require(inputs, 1) {
            Ok(k) => key_bits(k),
            Err(e) => return NodeOutput::computed(Err(e)),
        };
        // `HashMap::insert` returns the displaced previous value, or `None` for
        // the first record of a key — that first record is `WarmingUp`.
        NodeOutput::computed(self.prev.insert(key, value).ok_or(Absent::WarmingUp))
    }

    fn name(&self) -> &str {
        "prev_by"
    }

    fn save(&self) -> Option<Vec<u8>> {
        checkpoint::save(self)
    }

    fn load(&mut self, bytes: &[u8]) -> Result<(), OperatorLoadError> {
        checkpoint::load(self, bytes)
    }
}

// ============================================================================
// StatefulOps extension trait
// ============================================================================

/// Stateful-tracker operations on `Comp`.
///
/// The single blanket impl below adds every tracker method to `Comp<R, f64>`,
/// so call sites read naturally — e.g. `price.delta(Duration::from_secs(1))`.
pub trait StatefulOps<R> {
    /// Previous value (one record back). Warming up on the first record.
    fn prev(&self) -> Comp<R, f64>;
    /// Previous value partitioned by a per-record key.
    ///
    /// `key_fn` extracts a partition key from each record; the previous value
    /// is tracked separately per key, and the first record of each key is
    /// warming up. The key is carried through the graph as an `f64`, so
    /// discrete keys must be encoded into one — see [`PrevByOp`] for the
    /// `2^53` exact-integer caveat and the NaN-key note.
    fn prev_by<F>(&self, key_fn: F) -> Comp<R, f64>
    where
        F: Fn(&R) -> f64 + Send + Sync + 'static;
    /// Value from `duration` ago.
    ///
    /// `duration` is held in the serialized `LagBuffer`, so it survives a
    /// checkpoint round-trip.
    fn lag(&self, duration: Duration) -> Comp<R, f64>;
    /// Current value minus the value from `duration` ago.
    ///
    /// `duration` is held in the serialized `LagBuffer`.
    fn delta(&self, duration: Duration) -> Comp<R, f64>;
    /// Rate of change per second (value change per `1000` ms of timestamp).
    ///
    /// `window` is accepted for API parity with the legacy catalog but does
    /// not affect the computation — the rate is always taken against the
    /// immediately preceding record.
    fn rate(&self, window: Duration) -> Comp<R, f64>;
    /// First derivative (velocity); same computation as [`rate`](Self::rate).
    fn velocity(&self, window: Duration) -> Comp<R, f64>;
    /// Second derivative (acceleration) — the rate of change of velocity.
    fn acceleration(&self, window: Duration) -> Comp<R, f64>;
    /// Cumulative sum since the start of the stream.
    fn cumsum(&self) -> Comp<R, f64>;
    /// Cumulative maximum (high-water mark) since the start of the stream.
    fn cummax(&self) -> Comp<R, f64>;
    /// Cumulative minimum since the start of the stream.
    fn cummin(&self) -> Comp<R, f64>;
    /// Cumulative product since the start of the stream.
    fn cumprod(&self) -> Comp<R, f64>;
    /// Percentage change from the previous value: `(cur - prev) / prev * 100`.
    fn pct_change(&self) -> Comp<R, f64>;
    /// Log return from the previous value: `ln(cur / prev)`.
    fn log_return(&self) -> Comp<R, f64>;
}

impl<R: 'static> StatefulOps<R> for Comp<R, f64> {
    fn prev(&self) -> Comp<R, f64> {
        self.custom_node1(|| StatefulTracker::new(PrevTracker::new(), PrevStep))
    }

    fn prev_by<F>(&self, key_fn: F) -> Comp<R, f64>
    where
        F: Fn(&R) -> f64 + Send + Sync + 'static,
    {
        // Wire a hidden source node that extracts the partition key from each
        // record, then feed it as the second input to the 2-input `PrevByOp`.
        let key = self.prop_from_record(key_fn);
        Comp::custom_node(self, &[&key], PrevByOp::default)
    }

    fn lag(&self, duration: Duration) -> Comp<R, f64> {
        self.custom_node1(move || StatefulTracker::new(LagBuffer::new(duration), LagStep))
    }

    fn delta(&self, duration: Duration) -> Comp<R, f64> {
        self.custom_node1(move || StatefulTracker::new(LagBuffer::new(duration), DeltaStep))
    }

    fn rate(&self, _window: Duration) -> Comp<R, f64> {
        self.custom_node1(|| StatefulTracker::new(DerivativeState::default(), RateStep))
    }

    fn velocity(&self, _window: Duration) -> Comp<R, f64> {
        self.custom_node1(|| StatefulTracker::new(DerivativeState::default(), VelocityStep))
    }

    fn acceleration(&self, _window: Duration) -> Comp<R, f64> {
        self.custom_node1(|| StatefulTracker::new(AccelerationState::default(), AccelerationStep))
    }

    fn cumsum(&self) -> Comp<R, f64> {
        self.custom_node1(|| StatefulTracker::new(CumulativeSum::new(), CumSumStep))
    }

    fn cummax(&self) -> Comp<R, f64> {
        self.custom_node1(|| StatefulTracker::new(CumulativeMax::new(), CumMaxStep))
    }

    fn cummin(&self) -> Comp<R, f64> {
        self.custom_node1(|| StatefulTracker::new(CumulativeMin::new(), CumMinStep))
    }

    fn cumprod(&self) -> Comp<R, f64> {
        self.custom_node1(|| StatefulTracker::new(CumulativeProduct::new(), CumProdStep))
    }

    fn pct_change(&self) -> Comp<R, f64> {
        self.custom_node1(|| StatefulTracker::new(PctChangeState::default(), PctChangeStep))
    }

    fn log_return(&self) -> Comp<R, f64> {
        self.custom_node1(|| StatefulTracker::new(LogReturnState::default(), LogReturnStep))
    }
}
