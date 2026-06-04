//! Stateless unary math operators and the [`MathOps`] extension trait.
//!
//! All 11 operators are pure per-record transforms on `Comp<R, f64>`.
//!
//! Most operators (`abs`, `exp`, `pow`, `clamp`, `floor`, `ceil`, `round`) are
//! implemented as [`Comp::map_f64`] closures — they have no domain failures, so
//! a plain `f64 -> f64` closure is sufficient.
//!
//! Four operators (`sqrt`, `ln`, `log10`, `log2`) have domain restrictions and
//! surface typed [`Absent::DomainError`] on out-of-domain inputs, matching the
//! hardening-pass semantics in `tflo-core`'s `NodeOp` eval arms. These are
//! hand-written [`Operator`] structs that return
//! `NodeOutput::computed(Err(Absent::DomainError))` on domain violations.
//! A closure form would only produce `NaN` and lose the typed reason.
//!
//! Every method is exposed on `Comp<R, f64>` through the single [`MathOps`]
//! extension trait so call sites read naturally — e.g. `price.sqrt()`.

use serde::{Deserialize, Serialize};
use tflo_core::comp::Comp;
use tflo_core::compile::{Absent, Computed, NodeOutput};
use tflo_core::operator::{Operator, OperatorLoadError, require};

use crate::checkpoint;

// ============================================================================
// Hand-written domain-checking operators
// ============================================================================

/// Square root: returns [`Absent::DomainError`] for `x < 0`.
///
/// Domain predicate matches `tflo-core`'s `NodeOp::Sqrt` eval arm exactly.
#[derive(Default, Serialize, Deserialize)]
pub struct Sqrt;

impl Operator for Sqrt {
    fn eval(&mut self, inputs: &[Computed], _ts: i64) -> NodeOutput {
        let v = match require(inputs, 0) {
            Ok(v) => v,
            Err(e) => return NodeOutput::computed(Err(e)),
        };
        if v < 0.0 {
            NodeOutput::computed(Err(Absent::DomainError))
        } else {
            NodeOutput::computed(Ok(v.sqrt()))
        }
    }

    fn name(&self) -> &str {
        "sqrt"
    }

    fn save(&self) -> Option<Vec<u8>> {
        checkpoint::save(self)
    }

    fn load(&mut self, bytes: &[u8]) -> Result<(), OperatorLoadError> {
        checkpoint::load(self, bytes)
    }
}

/// Natural logarithm: returns [`Absent::DomainError`] for `x <= 0`.
///
/// Domain predicate matches `tflo-core`'s `NodeOp::Ln` eval arm exactly.
#[derive(Default, Serialize, Deserialize)]
pub struct Ln;

impl Operator for Ln {
    fn eval(&mut self, inputs: &[Computed], _ts: i64) -> NodeOutput {
        let v = match require(inputs, 0) {
            Ok(v) => v,
            Err(e) => return NodeOutput::computed(Err(e)),
        };
        if v <= 0.0 {
            NodeOutput::computed(Err(Absent::DomainError))
        } else {
            NodeOutput::computed(Ok(v.ln()))
        }
    }

    fn name(&self) -> &str {
        "ln"
    }

    fn save(&self) -> Option<Vec<u8>> {
        checkpoint::save(self)
    }

    fn load(&mut self, bytes: &[u8]) -> Result<(), OperatorLoadError> {
        checkpoint::load(self, bytes)
    }
}

/// Base-10 logarithm: returns [`Absent::DomainError`] for `x <= 0`.
///
/// Domain predicate matches `tflo-core`'s `NodeOp::Log10` eval arm exactly.
#[derive(Default, Serialize, Deserialize)]
pub struct Log10;

impl Operator for Log10 {
    fn eval(&mut self, inputs: &[Computed], _ts: i64) -> NodeOutput {
        let v = match require(inputs, 0) {
            Ok(v) => v,
            Err(e) => return NodeOutput::computed(Err(e)),
        };
        if v <= 0.0 {
            NodeOutput::computed(Err(Absent::DomainError))
        } else {
            NodeOutput::computed(Ok(v.log10()))
        }
    }

    fn name(&self) -> &str {
        "log10"
    }

    fn save(&self) -> Option<Vec<u8>> {
        checkpoint::save(self)
    }

    fn load(&mut self, bytes: &[u8]) -> Result<(), OperatorLoadError> {
        checkpoint::load(self, bytes)
    }
}

/// Base-2 logarithm: returns [`Absent::DomainError`] for `x <= 0`.
///
/// Domain predicate matches `tflo-core`'s `NodeOp::Log2` eval arm exactly.
#[derive(Default, Serialize, Deserialize)]
pub struct Log2;

impl Operator for Log2 {
    fn eval(&mut self, inputs: &[Computed], _ts: i64) -> NodeOutput {
        let v = match require(inputs, 0) {
            Ok(v) => v,
            Err(e) => return NodeOutput::computed(Err(e)),
        };
        if v <= 0.0 {
            NodeOutput::computed(Err(Absent::DomainError))
        } else {
            NodeOutput::computed(Ok(v.log2()))
        }
    }

    fn name(&self) -> &str {
        "log2"
    }

    fn save(&self) -> Option<Vec<u8>> {
        checkpoint::save(self)
    }

    fn load(&mut self, bytes: &[u8]) -> Result<(), OperatorLoadError> {
        checkpoint::load(self, bytes)
    }
}

// ============================================================================
// MathOps extension trait
// ============================================================================

/// Stateless unary math operations on `Comp`.
///
/// The single blanket impl below adds every math method to `Comp<R, f64>` so
/// call sites read naturally — e.g. `price.abs()`, `price.sqrt()`.
///
/// Four methods (`sqrt`, `ln`, `log10`, `log2`) surface typed
/// [`Absent::DomainError`] when the input is out of the mathematical domain,
/// matching the `tflo-core` oracle. The remaining seven (`abs`, `exp`, `pow`,
/// `clamp`, `floor`, `ceil`, `round`) are closures with no domain failures.
///
/// # UFCS note
///
/// If the legacy `tflo-core` inherent methods on `Comp<R, f64>` shadow these
/// trait methods under plain call syntax, use explicit UFCS at the call site:
/// `MathOps::sqrt(&comp)`.
pub trait MathOps<R> {
    /// Absolute value: `|x|`.
    fn abs(&self) -> Comp<R, f64>;
    /// Square root. Returns [`Absent::DomainError`] for `x < 0`.
    fn sqrt(&self) -> Comp<R, f64>;
    /// Natural logarithm. Returns [`Absent::DomainError`] for `x <= 0`.
    fn ln(&self) -> Comp<R, f64>;
    /// Base-10 logarithm. Returns [`Absent::DomainError`] for `x <= 0`.
    fn log10(&self) -> Comp<R, f64>;
    /// Base-2 logarithm. Returns [`Absent::DomainError`] for `x <= 0`.
    fn log2(&self) -> Comp<R, f64>;
    /// Exponential: `e^x`. No domain restriction.
    fn exp(&self) -> Comp<R, f64>;
    /// Power: `x^exponent`. No domain restriction (matches oracle).
    fn pow(&self, exponent: f64) -> Comp<R, f64>;
    /// Clamp to `[lo, hi]`.
    fn clamp(&self, lo: f64, hi: f64) -> Comp<R, f64>;
    /// Floor (round toward negative infinity).
    fn floor(&self) -> Comp<R, f64>;
    /// Ceiling (round toward positive infinity).
    fn ceil(&self) -> Comp<R, f64>;
    /// Round to nearest integer (half-way rounds away from zero).
    fn round(&self) -> Comp<R, f64>;
}

impl<R: 'static> MathOps<R> for Comp<R, f64> {
    fn abs(&self) -> Self {
        self.map_f64(f64::abs)
    }

    fn sqrt(&self) -> Self {
        self.custom_node1(Sqrt::default)
    }

    fn ln(&self) -> Self {
        self.custom_node1(Ln::default)
    }

    fn log10(&self) -> Self {
        self.custom_node1(Log10::default)
    }

    fn log2(&self) -> Self {
        self.custom_node1(Log2::default)
    }

    fn exp(&self) -> Self {
        self.map_f64(f64::exp)
    }

    fn pow(&self, exponent: f64) -> Self {
        self.map_f64(move |x| x.powf(exponent))
    }

    fn clamp(&self, lo: f64, hi: f64) -> Self {
        self.map_f64(move |x| x.clamp(lo, hi))
    }

    fn floor(&self) -> Self {
        self.map_f64(f64::floor)
    }

    fn ceil(&self) -> Self {
        self.map_f64(f64::ceil)
    }

    fn round(&self) -> Self {
        self.map_f64(f64::round)
    }
}
