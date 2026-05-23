//! Typed reasons a node produced no value on a given record.
//!
//! tflo's engine historically used `f64::NAN` as a sentinel for "no value" —
//! warming up, filtered out, divide-by-zero, and genuine math-domain errors
//! were all indistinguishable. [`Absent`] replaces that sentinel: a node's
//! per-record output is a [`Computed`] — either a finite `f64`, or an `Err`
//! carrying the specific reason the value is absent.

/// Why a node produced no `f64` value on a given record.
///
/// This is the `Err` half of [`Computed`]. It is `Copy` and pointer-free, so
/// it propagates through the engine with `?` at no cost.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Absent {
    /// Not enough data yet — a window or accumulator is still filling, or a
    /// stateful tracker has not yet seen a prior sample. Resolves on its own
    /// once enough records have been processed.
    WarmingUp,
    /// A parameter makes the node unable to *ever* produce a value — for
    /// example a window or period of zero. Unlike [`WarmingUp`](Absent::WarmingUp),
    /// this never resolves; it signals a misconfigured graph.
    InvalidConfig,
    /// A division had a zero denominator.
    DivideByZero,
    /// A math function received an argument outside its domain — for example
    /// `sqrt` or `ln` of a negative number, or `ln` of zero.
    DomainError,
    /// A rate or derivative could not be computed because two consecutive
    /// samples shared a timestamp (zero elapsed time).
    ZeroTimeDelta,
    /// A filter predicate suppressed this value.
    FilteredOut,
    /// An input to this node was itself absent; the absence propagated
    /// downstream. The original reason is preserved when a node has a single
    /// absent input — `UpstreamAbsent` is used only when reasons would
    /// otherwise be ambiguous.
    UpstreamAbsent,
}

impl Absent {
    /// A short, human-readable label for diagnostics and graph-plan output.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::WarmingUp => "warming up",
            Self::InvalidConfig => "invalid configuration",
            Self::DivideByZero => "divide by zero",
            Self::DomainError => "math domain error",
            Self::ZeroTimeDelta => "zero time delta",
            Self::FilteredOut => "filtered out",
            Self::UpstreamAbsent => "upstream absent",
        }
    }
}

impl std::fmt::Display for Absent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.label())
    }
}

/// A node's per-record output: a finite `f64`, or a typed [`Absent`] reason
/// for why no value is available.
///
/// This replaces the bare `f64`-with-NaN-sentinel that the engine used
/// internally. Arithmetic nodes propagate the first `Err` they see; stateful
/// nodes skip their state update on an absent input rather than advancing with
/// a substitute value.
pub type Computed = Result<f64, Absent>;

/// Map a primitive's raw `f64` result onto a typed [`Computed`].
///
/// The ~20 windowing and statistics primitives still use a non-finite `f64`
/// (in practice `NaN`) as their internal "not enough data yet / empty window"
/// sentinel — rewriting all of them is out of scope. This function is the
/// single seam that converts that sentinel into a typed reason: a finite value
/// passes through as `Ok`, anything else (`NaN`, `±inf`) becomes
/// `Err(Absent::WarmingUp)`.
///
/// # Errors
///
/// Returns `Err(Absent::WarmingUp)` when `x` is not finite (`NaN` or
/// `±inf`) — the "no value yet" sentinel for warming windows.
#[inline]
pub const fn finite_or_warming(x: f64) -> Computed {
    if x.is_finite() {
        Ok(x)
    } else {
        Err(Absent::WarmingUp)
    }
}
