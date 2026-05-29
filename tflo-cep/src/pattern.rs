//! The [`Pattern`] builder.
//!
//! The builder produces a compiled pattern wrapped around the shared
//! [`engine::Compiled`] type, instantiated with Arc-based callback types
//! so the pattern can be shared across threads (the standard Rust use
//! case). `tflo-cep-wasm` instantiates [`engine`] with `Rc`-based,
//! JS-function-backed callbacks instead — same engine, single-threaded
//! callbacks.

use crate::engine::{Compiled, EmitCallback, Predicate, Step, TimestampCallback};
use crate::matched::Match;
use std::sync::Arc;
use std::time::Duration;

// ── Arc-based callback wrappers (the default Pattern uses these) ────

/// Predicate stored as `Arc<dyn Fn + Send + Sync>` — shareable across
/// threads.
pub struct ArcPredicate<E> {
    f: Arc<dyn Fn(&E) -> bool + Send + Sync>,
}

impl<E> Clone for ArcPredicate<E> {
    fn clone(&self) -> Self {
        Self { f: self.f.clone() }
    }
}

impl<E: 'static> Predicate<E> for ArcPredicate<E> {
    fn evaluate(&self, event: &E) -> bool {
        (self.f)(event)
    }
}

/// Emit callback stored as `Arc<dyn Fn + Send + Sync>`.
pub struct ArcEmit<E, M> {
    f: Arc<dyn Fn(&Match<E>) -> M + Send + Sync>,
}

impl<E: 'static, M: 'static> EmitCallback<E, M> for ArcEmit<E, M> {
    fn emit(&self, m: &Match<E>) -> M {
        (self.f)(m)
    }
}

/// Timestamp callback stored as `Arc<dyn Fn + Send + Sync>`.
pub struct ArcTimestamp<E> {
    f: Arc<dyn Fn(&E) -> i64 + Send + Sync>,
}

impl<E: 'static> TimestampCallback<E> for ArcTimestamp<E> {
    fn timestamp(&self, event: &E) -> i64 {
        (self.f)(event)
    }
}

// ── Errors ──────────────────────────────────────────────────────────

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

// ── The builder ─────────────────────────────────────────────────────

/// Intermediate step representation used during building (carries the
/// `is_negative` flag in the same place as `Step` so we can validate
/// before constructing the final `engine::Step`).
enum BuilderStep<E> {
    Positive {
        name: String,
        predicate: ArcPredicate<E>,
        within_ms: Option<i64>,
    },
    Negative {
        name: String,
        predicate: ArcPredicate<E>,
        within_ms: Option<i64>,
    },
}

impl<E> BuilderStep<E> {
    fn name(&self) -> &str {
        match self {
            Self::Positive { name, .. } | Self::Negative { name, .. } => name,
        }
    }
    const fn set_within(&mut self, ms: i64) {
        match self {
            Self::Positive { within_ms, .. } | Self::Negative { within_ms, .. } => {
                *within_ms = Some(ms);
            }
        }
    }
    const fn is_negative(&self) -> bool {
        matches!(self, Self::Negative { .. })
    }
    const fn within_ms(&self) -> Option<i64> {
        match self {
            Self::Positive { within_ms, .. } | Self::Negative { within_ms, .. } => *within_ms,
        }
    }
}

/// A pattern over events of type `E` that emits values of type `M` on
/// successful matches.
///
/// Build with [`Pattern::new`], chain steps, and finalize with
/// [`Pattern::emit`]. The emit closure consumes a [`Match<E>`] (capturing
/// the matched events) and returns the user's chosen output type.
///
/// Internally the finalized pattern owns an [`crate::engine::Compiled`]
/// with Arc-based callbacks — the pattern is `Send + Sync` and can be
/// cloned cheaply for distribution to worker threads.
pub struct Pattern<E, M = ()> {
    name: String,
    timestamp_fn: Option<ArcTimestamp<E>>,
    steps: Vec<BuilderStep<E>>,
    compiled: Option<Compiled<E, M, ArcPredicate<E>, ArcEmit<E, M>, ArcTimestamp<E>>>,
}

impl<E: Clone + 'static, M: 'static> Pattern<E, M> {
    /// Begin a new pattern with the given diagnostic name.
    #[must_use]
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            timestamp_fn: None,
            steps: Vec::new(),
            compiled: None,
        }
    }

    /// Set the event-time extractor.
    #[must_use]
    pub fn timestamp<F>(mut self, f: F) -> Self
    where
        F: Fn(&E) -> i64 + Send + Sync + 'static,
    {
        self.timestamp_fn = Some(ArcTimestamp { f: Arc::new(f) });
        self
    }

    /// Initial match step.
    #[must_use]
    pub fn when<F>(mut self, p: F) -> Self
    where
        F: Fn(&E) -> bool + Send + Sync + 'static,
    {
        let name = self.alloc_name("when");
        let predicate = ArcPredicate { f: Arc::new(p) };
        let step = BuilderStep::Positive {
            name,
            predicate,
            within_ms: None,
        };
        if let Some(first) = self.steps.first_mut() {
            *first = step;
        } else {
            self.steps.push(step);
        }
        self
    }

    /// Positive sequential step.
    #[must_use]
    pub fn then<F>(self, p: F) -> Self
    where
        F: Fn(&E) -> bool + Send + Sync + 'static,
    {
        let name = format!("then_{}", self.steps.len());
        self.then_named(name, p)
    }

    /// Positive sequential step with an explicit name.
    #[must_use]
    pub fn then_named<F>(mut self, name: impl Into<String>, p: F) -> Self
    where
        F: Fn(&E) -> bool + Send + Sync + 'static,
    {
        self.steps.push(BuilderStep::Positive {
            name: name.into(),
            predicate: ArcPredicate { f: Arc::new(p) },
            within_ms: None,
        });
        self
    }

    /// Negative terminal step.
    #[must_use]
    pub fn not_then<F>(self, p: F) -> Self
    where
        F: Fn(&E) -> bool + Send + Sync + 'static,
    {
        let name = format!("not_then_{}", self.steps.len());
        self.not_then_named(name, p)
    }

    /// Negative terminal step with an explicit name.
    #[must_use]
    pub fn not_then_named<F>(mut self, name: impl Into<String>, p: F) -> Self
    where
        F: Fn(&E) -> bool + Send + Sync + 'static,
    {
        self.steps.push(BuilderStep::Negative {
            name: name.into(),
            predicate: ArcPredicate { f: Arc::new(p) },
            within_ms: None,
        });
        self
    }

    /// Attach a time bound to the most recently added step.
    #[must_use]
    pub fn within(mut self, d: Duration) -> Self {
        let ms = d.as_millis().try_into().unwrap_or(i64::MAX);
        if let Some(last) = self.steps.last_mut() {
            last.set_within(ms);
        }
        self
    }

    /// Finalize the pattern with an emit closure.
    ///
    /// # Errors
    ///
    /// Returns [`PatternError`] when the builder state is structurally
    /// invalid (see [`PatternError`]).
    pub fn emit<F, M2: 'static>(self, f: F) -> Result<Pattern<E, M2>, PatternError>
    where
        F: Fn(&Match<E>) -> M2 + Send + Sync + 'static,
    {
        if self.steps.is_empty() {
            return Err(PatternError::NoWhenStep);
        }
        for (i, step) in self.steps.iter().enumerate() {
            if step.is_negative() {
                if step.within_ms().is_none() {
                    return Err(PatternError::NotThenMissingWithin {
                        step_name: step.name().to_string(),
                    });
                }
                if i != self.steps.len().saturating_sub(1) {
                    return Err(PatternError::NotThenNotTerminal {
                        step_name: step.name().to_string(),
                    });
                }
            }
        }

        // Convert builder steps into engine steps.
        let engine_steps: Vec<Step<E, ArcPredicate<E>>> = self
            .steps
            .into_iter()
            .map(|s| match s {
                BuilderStep::Positive {
                    name,
                    predicate,
                    within_ms,
                } => Step::positive(name, predicate, within_ms),
                BuilderStep::Negative {
                    name,
                    predicate,
                    within_ms,
                } => Step::negative(name, predicate, within_ms),
            })
            .collect();

        let emit_fn = ArcEmit { f: Arc::new(f) };
        let compiled = Compiled::new(self.name.clone(), engine_steps, emit_fn, self.timestamp_fn);

        Ok(Pattern {
            name: self.name,
            timestamp_fn: None,
            steps: Vec::new(),
            compiled: Some(compiled),
        })
    }

    fn alloc_name(&self, prefix: &str) -> String {
        format!("{prefix}_{}", self.steps.len())
    }

    // ── internal accessors used by the runtime ──

    /// Take ownership of the compiled engine pattern. Returns `None` if
    /// the builder has not been finalized via [`emit`](Self::emit).
    pub(crate) const fn take_compiled(
        &mut self,
    ) -> Option<Compiled<E, M, ArcPredicate<E>, ArcEmit<E, M>, ArcTimestamp<E>>> {
        self.compiled.take()
    }

    /// Pattern name (for diagnostics).
    #[allow(dead_code)]
    pub(crate) fn name_str(&self) -> &str {
        &self.name
    }
}
