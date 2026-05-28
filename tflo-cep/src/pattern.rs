//! The [`Pattern`] builder and its compiled form.
//!
//! A pattern is a linear sequence of *steps*:
//!
//! - `when(p)` — required first; opens a match when `p(event)` holds.
//! - `then(p)` — positive: the next event satisfying `p` (within the most
//!   recent [`within`](Pattern::within) bound, if any) advances the match.
//! - `not_then(p)` — negative: the match succeeds when **no** event
//!   satisfying `p` arrives within the [`within`](Pattern::within) bound.
//!   Always paired with `within`.
//! - `within(d)` — modifier; attaches a deadline to the most recently added
//!   `then` / `not_then` step.
//! - `emit(closure)` — final step; converts a successful match into the
//!   user's output type.

use crate::matched::Match;
use std::sync::Arc;
use std::time::Duration;

/// A predicate over an event of type `E`.
///
/// Constructed automatically when the user passes a `Fn(&E) -> bool` to
/// any of the builder methods.
pub(crate) type Predicate<E> = Arc<dyn Fn(&E) -> bool + Send + Sync>;

/// One step in a pattern.
pub(crate) enum Step<E> {
    /// Positive match — advance the partial-match cursor.
    Positive {
        name: String,
        predicate: Predicate<E>,
        within_ms: Option<i64>,
    },
    /// Negative match — succeed when **no** event satisfies `predicate`
    /// within `within_ms`. `within_ms` is required (validated at `emit`
    /// time).
    Negative {
        name: String,
        predicate: Predicate<E>,
        within_ms: Option<i64>,
    },
}

impl<E> Step<E> {
    pub(crate) fn name(&self) -> &str {
        match self {
            Self::Positive { name, .. } | Self::Negative { name, .. } => name,
        }
    }
    pub(crate) fn within_ms(&self) -> Option<i64> {
        match self {
            Self::Positive { within_ms, .. } | Self::Negative { within_ms, .. } => *within_ms,
        }
    }
    pub(crate) fn predicate(&self) -> &Predicate<E> {
        match self {
            Self::Positive { predicate, .. } | Self::Negative { predicate, .. } => predicate,
        }
    }
    pub(crate) fn is_negative(&self) -> bool {
        matches!(self, Self::Negative { .. })
    }
}

/// Errors surfaced by [`Pattern`] construction at [`emit`](Pattern::emit) time.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PatternError {
    /// No `when` step was added before `emit`.
    NoWhenStep,
    /// A `not_then` step has no `within` bound — required because the only
    /// way a negative step can succeed is by a deadline firing without a
    /// matching event.
    NotThenMissingWithin {
        /// The step's name.
        step_name: String,
    },
    /// A `not_then` step is followed by another step. The negative is
    /// terminal — once it fires (or fails), the pattern is decided.
    NotThenNotTerminal {
        /// The step's name.
        step_name: String,
    },
}

impl std::fmt::Display for PatternError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoWhenStep => f.write_str("pattern is missing the initial .when(...) step"),
            Self::NotThenMissingWithin { step_name } => write!(
                f,
                "pattern step `{step_name}` is `not_then` but has no `.within(...)` bound"
            ),
            Self::NotThenNotTerminal { step_name } => write!(
                f,
                "pattern step `{step_name}` is `not_then` and must be the last step"
            ),
        }
    }
}

impl std::error::Error for PatternError {}

type EmitFn<E, M> = Arc<dyn Fn(&Match<E>) -> M + Send + Sync>;
type TsFn<E> = Arc<dyn Fn(&E) -> i64 + Send + Sync>;

/// A pattern over events of type `E` that emits values of type `M` on
/// successful matches.
///
/// Build with [`Pattern::new`], chain steps, and finalize with
/// [`Pattern::emit`]. The emit closure consumes a [`Match<E>`] (capturing
/// the matched events) and returns the user's chosen output type.
pub struct Pattern<E, M = ()> {
    name: String,
    timestamp_fn: Option<TsFn<E>>,
    steps: Vec<Step<E>>,
    emit_fn: Option<EmitFn<E, M>>,
    /// Pending name for the next step (set by builder methods that need
    /// a defaulted name).
    next_auto_name: u32,
}

impl<E: Clone + 'static> Pattern<E, ()> {
    /// Begin a new pattern with the given diagnostic name (used by the
    /// `Match` returned to emit closures).
    #[must_use]
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            timestamp_fn: None,
            steps: Vec::new(),
            emit_fn: None,
            next_auto_name: 0,
        }
    }
}

impl<E: Clone + 'static, M: 'static> Pattern<E, M> {
    /// Required: extract the event-time (in milliseconds) from each event.
    ///
    /// `within(...)` bounds are interpreted in the same units this function
    /// returns. Most callers use `Duration::as_millis() as i64` semantics,
    /// matching the rest of the tflo engine.
    #[must_use]
    pub fn timestamp<F>(mut self, f: F) -> Self
    where
        F: Fn(&E) -> i64 + Send + Sync + 'static,
    {
        self.timestamp_fn = Some(Arc::new(f));
        self
    }

    /// Required first step: open a match when `p(event)` returns true.
    ///
    /// Subsequent calls overwrite the previous `when` — `Pattern` is a
    /// linear sequence with exactly one starting step.
    #[must_use]
    pub fn when<F>(mut self, p: F) -> Self
    where
        F: Fn(&E) -> bool + Send + Sync + 'static,
    {
        let name = self.alloc_name("when");
        let step = Step::Positive {
            name,
            predicate: Arc::new(p),
            within_ms: None,
        };
        // `when` is always step 0; replace if user calls it twice.
        if self.steps.is_empty() {
            self.steps.push(step);
        } else {
            self.steps[0] = step;
        }
        self
    }

    /// Add a positive sequential step.
    ///
    /// On a partial match advanced to this step, the next event satisfying
    /// `p` (within the bound set by a following [`within`](Self::within),
    /// if any) advances the match.
    #[must_use]
    pub fn then<F>(self, p: F) -> Self
    where
        F: Fn(&E) -> bool + Send + Sync + 'static,
    {
        let name = self.next_auto_name("then");
        self.then_named(name, p)
    }

    /// Same as [`then`](Self::then) but with an explicit step name (used by
    /// `Match::at("name")` to look up the captured event).
    #[must_use]
    pub fn then_named<F>(mut self, name: impl Into<String>, p: F) -> Self
    where
        F: Fn(&E) -> bool + Send + Sync + 'static,
    {
        self.steps.push(Step::Positive {
            name: name.into(),
            predicate: Arc::new(p),
            within_ms: None,
        });
        self
    }

    /// Add a negative terminal step.
    ///
    /// The pattern succeeds when no event satisfying `p` arrives within the
    /// deadline (set by a following [`within`](Self::within), which is
    /// required for negative steps). The match resolves the moment the
    /// deadline passes — the emitted `Match<E>` carries the preceding
    /// positive captures but no capture for the negative step itself.
    #[must_use]
    pub fn not_then<F>(self, p: F) -> Self
    where
        F: Fn(&E) -> bool + Send + Sync + 'static,
    {
        let name = self.next_auto_name("not_then");
        self.not_then_named(name, p)
    }

    /// Same as [`not_then`](Self::not_then) with an explicit step name.
    #[must_use]
    pub fn not_then_named<F>(mut self, name: impl Into<String>, p: F) -> Self
    where
        F: Fn(&E) -> bool + Send + Sync + 'static,
    {
        self.steps.push(Step::Negative {
            name: name.into(),
            predicate: Arc::new(p),
            within_ms: None,
        });
        self
    }

    /// Attach a time bound to the most recently added step.
    ///
    /// For a positive step, the bound is a relaxation: events arriving more
    /// than `d` after the previous capture without satisfying the step's
    /// predicate cause the partial match to be dropped.
    ///
    /// For a negative step, the bound is the deadline at which absence of
    /// the matching event resolves the pattern successfully.
    ///
    /// Calling `within` on a fresh pattern (no steps yet) is a no-op so the
    /// builder API can stay safe to call in any order.
    #[must_use]
    pub fn within(mut self, d: Duration) -> Self {
        let ms = d.as_millis().try_into().unwrap_or(i64::MAX);
        if let Some(last) = self.steps.last_mut() {
            match last {
                Step::Positive { within_ms, .. } | Step::Negative { within_ms, .. } => {
                    *within_ms = Some(ms);
                }
            }
        }
        self
    }

    /// Finalize the pattern with an emit closure.
    ///
    /// Returns the compiled pattern ready to feed an iterator adapter via
    /// [`PatternIter::match_pattern`](crate::PatternIter::match_pattern),
    /// or a [`PatternError`] if the builder state is invalid (missing
    /// `when`, missing `within` on a negative step, etc.).
    ///
    /// # Errors
    ///
    /// Returns [`PatternError`] when the builder state is structurally
    /// invalid: no initial `when` step, a `not_then` without a
    /// corresponding `within` bound, or a `not_then` followed by another
    /// step.
    pub fn emit<F, M2: 'static>(self, f: F) -> Result<Pattern<E, M2>, PatternError>
    where
        F: Fn(&Match<E>) -> M2 + Send + Sync + 'static,
    {
        if self.steps.is_empty() {
            return Err(PatternError::NoWhenStep);
        }
        // not_then validity: requires within, must be terminal.
        for (i, step) in self.steps.iter().enumerate() {
            if let Step::Negative { name, within_ms, .. } = step {
                if within_ms.is_none() {
                    return Err(PatternError::NotThenMissingWithin {
                        step_name: name.clone(),
                    });
                }
                if i != self.steps.len() - 1 {
                    return Err(PatternError::NotThenNotTerminal {
                        step_name: name.clone(),
                    });
                }
            }
        }
        Ok(Pattern {
            name: self.name,
            timestamp_fn: self.timestamp_fn,
            steps: self.steps,
            emit_fn: Some(Arc::new(f)),
            next_auto_name: self.next_auto_name,
        })
    }

    // --- internal accessors used by the runtime ---

    pub(crate) fn name_str(&self) -> &str {
        &self.name
    }
    pub(crate) fn steps(&self) -> &[Step<E>] {
        &self.steps
    }
    pub(crate) fn timestamp_of(&self, e: &E) -> Option<i64> {
        self.timestamp_fn.as_ref().map(|f| f(e))
    }
    pub(crate) fn emit_with(&self, m: &Match<E>) -> Option<M> {
        self.emit_fn.as_ref().map(|f| f(m))
    }

    fn alloc_name(&mut self, prefix: &str) -> String {
        let n = self.next_auto_name;
        self.next_auto_name = self.next_auto_name.saturating_add(1);
        format!("{prefix}_{n}")
    }
    fn next_auto_name(&self, prefix: &str) -> String {
        // Builder methods that take `self` by move can't update the counter
        // before constructing the step; we synthesize without bumping. The
        // caller (`then_named` / `not_then_named`) is what owns the name in
        // the steps Vec, so collisions only happen if a user writes two
        // anonymous steps at the same index, which they can't — the builder
        // pushes monotonically.
        format!("{prefix}_{}", self.steps.len())
    }
}
