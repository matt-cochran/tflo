//! The [`Pattern`] builder.
//!
//! The builder produces a compiled pattern wrapped around the shared
//! [`engine::Compiled`] type, instantiated with Arc-based callback types
//! so the pattern can be shared across threads (the standard Rust use
//! case). `tflo-cep-wasm` instantiates [`engine`] with `Rc`-based,
//! JS-function-backed callbacks instead — same engine, single-threaded
//! callbacks.

use crate::engine::{
    Compiled, Contiguity, EmitCallback, Predicate, RepeatSpec, Runtime, Step, TimestampCallback,
};
use crate::matched::Match;
use std::sync::Arc;
use std::time::Duration;

// ── Arc-based callback wrappers (the default Pattern uses these) ────

/// Predicate stored as `Arc<dyn Fn + Send + Sync>` — shareable across
/// threads.
///
/// The closure is context-aware (`event` plus the prior captures) so a
/// predicate can correlate across steps. Plain `Fn(&E) -> bool` closures from
/// `when`/`then`/`not_then` ignore the captures (built via [`Self::from_fn`]);
/// CEL predicates use them for `first_*`/`prev_*`/`s{i}_*` parity.
pub struct ArcPredicate<E> {
    f: Arc<dyn Fn(&E, &[(String, E)]) -> bool + Send + Sync>,
}

impl<E: 'static> ArcPredicate<E> {
    /// Wrap a context-free predicate (the captures are ignored).
    fn from_fn<F>(p: F) -> Self
    where
        F: Fn(&E) -> bool + Send + Sync + 'static,
    {
        Self {
            f: Arc::new(move |e, _caps| p(e)),
        }
    }
}

impl<E> Clone for ArcPredicate<E> {
    fn clone(&self) -> Self {
        Self { f: self.f.clone() }
    }
}

impl<E: 'static> Predicate<E> for ArcPredicate<E> {
    fn evaluate(&self, event: &E) -> bool {
        (self.f)(event, &[])
    }
    fn evaluate_in_context(&self, event: &E, captures: &[(String, E)]) -> bool {
        (self.f)(event, captures)
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
    /// A CEL predicate string (`when_cel`/`then_cel`/`not_then_cel`) failed the
    /// safety guard or the `cel-interpreter` parser. Surfaced at `emit` time so
    /// the builder stays chainable. Only produced with the `cel` feature.
    CelCompile {
        /// The step's name.
        step_name: String,
        /// The validator/parser reason.
        reason: String,
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
            Self::CelCompile { step_name, reason } => {
                write!(f, "pattern step `{step_name}` has an invalid CEL predicate: {reason}")
            }
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
        contiguity: Contiguity,
        repeat: Option<RepeatSpec>,
        forbidden: Option<ArcPredicate<E>>,
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
    /// Set strict-`next` contiguity on a positive step (no-op on negatives).
    const fn set_contiguity(&mut self, c: Contiguity) {
        if let Self::Positive { contiguity, .. } = self {
            *contiguity = c;
        }
    }
    /// Attach a `repeated(min..=max)` quantifier to a positive step.
    const fn set_repeat(&mut self, spec: RepeatSpec) {
        if let Self::Positive { repeat, .. } = self {
            *repeat = Some(spec);
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
    /// First CEL-predicate compile error, deferred so the builder stays
    /// chainable; surfaced by [`emit`](Self::emit). Always `None` without `cel`.
    pending_error: Option<PatternError>,
    /// An interior-negation guard set by `not_between`, pending attachment to the
    /// next positive (`then`) step. See [`engine::Step::forbidden`].
    pending_forbidden: Option<ArcPredicate<E>>,
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
            pending_error: None,
            pending_forbidden: None,
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
        let predicate = ArcPredicate::from_fn(p);
        let step = BuilderStep::Positive {
            name,
            predicate,
            within_ms: None,
            contiguity: Contiguity::Eventually,
            repeat: None,
            forbidden: None,
        };
        if let Some(first) = self.steps.first_mut() {
            *first = step;
        } else {
            self.steps.push(step);
        }
        self
    }

    /// Attach an **interior-negation guard** to the NEXT positive step: while the
    /// match skips events waiting for that step, any event satisfying `p` kills
    /// the partial. Expresses "A then B with no C in between" —
    /// `.when(a).not_between(c).then(b)` — which a terminal `not_then` cannot.
    #[must_use]
    pub fn not_between<F>(mut self, p: F) -> Self
    where
        F: Fn(&E) -> bool + Send + Sync + 'static,
    {
        self.pending_forbidden = Some(ArcPredicate::from_fn(p));
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
        let forbidden = self.pending_forbidden.take();
        self.steps.push(BuilderStep::Positive {
            name: name.into(),
            predicate: ArcPredicate::from_fn(p),
            within_ms: None,
            contiguity: Contiguity::Eventually,
            repeat: None,
            forbidden,
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
            predicate: ArcPredicate::from_fn(p),
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

    /// Make the most recently added step **strictly contiguous**: the event
    /// immediately following the previous capture must satisfy it, else the
    /// partial match dies. Lets a pattern express "B directly follows A" / "no
    /// event between A and B". No-op on the initial `when` / on negatives.
    #[must_use]
    pub fn next(mut self) -> Self {
        if let Some(last) = self.steps.last_mut() {
            last.set_contiguity(Contiguity::Next);
        }
        self
    }

    /// Make the most recently added step a `repeated(min..=max)` quantifier: it
    /// matches its predicate between `min` and `max` times (capturing each),
    /// then advances. Replaces faked `count-via-repeated-.then`. `min`/`max` are
    /// clamped to `1 <= min <= max`.
    #[must_use]
    pub fn times(mut self, min: usize, max: usize) -> Self {
        let min = min.max(1);
        let max = max.max(min);
        if let Some(last) = self.steps.last_mut() {
            last.set_repeat(RepeatSpec { min, max });
        }
        self
    }

    /// Finalize the pattern with an emit closure.
    ///
    /// # Errors
    ///
    /// Returns [`PatternError`] when the builder state is structurally
    /// invalid (see [`PatternError`]).
    pub fn emit<F, M2: 'static>(mut self, f: F) -> Result<Pattern<E, M2>, PatternError>
    where
        F: Fn(&Match<E>) -> M2 + Send + Sync + 'static,
    {
        // Surface any deferred CEL-predicate compile error first.
        if let Some(err) = self.pending_error.take() {
            return Err(err);
        }
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
                    contiguity,
                    repeat,
                    forbidden,
                } => {
                    let mut step = Step::positive(name, predicate, within_ms)
                        .with_contiguity(contiguity);
                    if let Some(spec) = repeat {
                        step = step.with_repeat(spec);
                    }
                    if let Some(guard) = forbidden {
                        step = step.with_forbidden(guard);
                    }
                    step
                }
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
            pending_error: None,
            pending_forbidden: None,
        })
    }

    fn alloc_name(&self, prefix: &str) -> String {
        format!("{prefix}_{}", self.steps.len())
    }

    /// Consume this finalized pattern into a push-based streaming [`Runtime`] —
    /// the native equivalent of the wasm `WasmPatternRuntime`, for hosts that
    /// drive `push` / `tick` / `flush` directly (e.g. a server reading a Kafka
    /// partition, or any consumer injecting its own clock: `rt.tick(clock.now_ms())`).
    /// The iterator adapter [`match_pattern`](crate::PatternIter::match_pattern)
    /// remains the batch convenience.
    ///
    /// Returns `None` if the builder was not finalized via [`emit`](Self::emit).
    #[must_use]
    pub fn into_runtime(
        mut self,
    ) -> Option<Runtime<E, M, ArcPredicate<E>, ArcEmit<E, M>, ArcTimestamp<E>>> {
        self.compiled.take().map(Runtime::new)
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

// ── CEL-string predicates (feature `cel`) ───────────────────────────────────
//
// These mirror `tflo-cep-wasm`'s CEL surface so a serialized pattern spec
// compiles to byte-identical matching on both tiers (cross-tier parity). They
// require `E: Serialize` (the engine builds the CEL variable context from the
// event's scalar fields). A bad CEL string is deferred to `emit` as
// `PatternError::CelCompile` so the builder stays chainable.
#[cfg(feature = "cel")]
impl<E: Clone + serde::Serialize + 'static, M: 'static> Pattern<E, M> {
    /// Build a context-aware predicate from an already-compiled CEL program.
    fn cel_predicate(prog: cel_interpreter::Program) -> ArcPredicate<E> {
        let prog = Arc::new(prog);
        ArcPredicate {
            f: Arc::new(move |e: &E, caps: &[(String, E)]| crate::cel::eval_in_context(&prog, e, caps)),
        }
    }

    /// Compile a CEL string into a predicate, deferring any error to `emit`.
    /// Returns a never-matching placeholder on failure so step structure holds.
    fn compile_cel_pred(&mut self, name: &str, expr: &str) -> ArcPredicate<E> {
        match crate::cel::compile(expr) {
            Ok(prog) => Self::cel_predicate(prog),
            Err(reason) => {
                if self.pending_error.is_none() {
                    self.pending_error = Some(PatternError::CelCompile {
                        step_name: name.to_string(),
                        reason,
                    });
                }
                ArcPredicate::from_fn(|_| false)
            }
        }
    }

    /// Initial match step with a CEL-string predicate, e.g.
    /// `kind == "add_to_cart"`. The same string the browser/server evaluate.
    #[must_use]
    pub fn when_cel(mut self, expr: &str) -> Self {
        let name = self.alloc_name("when");
        let predicate = self.compile_cel_pred(&name, expr);
        let step = BuilderStep::Positive {
            name,
            predicate,
            within_ms: None,
            contiguity: Contiguity::Eventually,
            repeat: None,
            forbidden: None,
        };
        if let Some(first) = self.steps.first_mut() {
            *first = step;
        } else {
            self.steps.push(step);
        }
        self
    }

    /// Positive sequential step with a CEL-string predicate. May reference
    /// earlier captures via `first_<field>` / `prev_<field>` / `s{i}_<field>`.
    #[must_use]
    pub fn then_cel(mut self, expr: &str) -> Self {
        let name = format!("then_{}", self.steps.len());
        let predicate = self.compile_cel_pred(&name, expr);
        let forbidden = self.pending_forbidden.take();
        self.steps.push(BuilderStep::Positive {
            name,
            predicate,
            within_ms: None,
            contiguity: Contiguity::Eventually,
            repeat: None,
            forbidden,
        });
        self
    }

    /// Attach an **interior-negation guard** as a CEL string to the next
    /// positive step — the serializable twin of [`not_between`](Self::not_between).
    /// `.when_cel(a).not_between_cel(c).then_cel(b)` expresses "A then B with no
    /// C in between" as patterns-as-data the browser and server share verbatim.
    #[must_use]
    pub fn not_between_cel(mut self, expr: &str) -> Self {
        let name = format!("not_between_{}", self.steps.len());
        let predicate = self.compile_cel_pred(&name, expr);
        self.pending_forbidden = Some(predicate);
        self
    }

    /// Negative terminal step with a CEL-string predicate (requires `within`).
    #[must_use]
    pub fn not_then_cel(mut self, expr: &str) -> Self {
        let name = format!("not_then_{}", self.steps.len());
        let predicate = self.compile_cel_pred(&name, expr);
        self.steps.push(BuilderStep::Negative {
            name,
            predicate,
            within_ms: None,
        });
        self
    }
}
