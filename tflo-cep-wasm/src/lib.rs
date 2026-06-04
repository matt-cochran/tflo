#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]
#![deny(clippy::print_stdout)]
#![allow(
    clippy::use_self,
    clippy::missing_const_for_fn,
    clippy::needless_pass_by_value
)]
#![allow(missing_docs)] // wasm-bindgen-exposed types document themselves via TS .d.ts emission

//! WebAssembly bindings for `tflo-cep`.
//!
//! The bindings present three classes to JS:
//!
//! - [`WasmPattern`] — the builder. Mirrors the Rust `Pattern` surface but
//!   accepts JS `Function` callbacks for predicates and emit.
//! - [`WasmCompiledPattern`] — finalized pattern, ready to drive a runtime.
//! - [`WasmPatternRuntime`] — the streaming state machine. Push events one
//!   at a time, collect emitted signals.
//!
//! ## Thin wrapper, single engine
//!
//! All matching logic lives in `tflo-cep::engine`. This crate just
//! supplies WASM-friendly `Predicate` / `EmitCallback` /
//! `TimestampCallback` impls that wrap `js_sys::Function` (via `Rc`,
//! since wasm32 is single-threaded and `js_sys::Function` is `!Send`),
//! then drives `engine::Runtime` directly. No duplicate state machine.

use std::cell::RefCell;
use std::rc::Rc;

use js_sys::{Array, Function};
use wasm_bindgen::prelude::*;

use tflo_cep::Match;
use tflo_cep::engine::{
    Compiled, Contiguity, DropReason, EmitCallback, Predicate, RepeatSpec, Runtime, Step,
    TimestampCallback,
};

/// Initialize the panic hook for better error messages in the browser
/// console. Idempotent — safe to call multiple times.
#[wasm_bindgen(start)]
pub fn start() {
    console_error_panic_hook::set_once();
}

// ── JS-callback adapters that implement the engine's traits ─────────

#[derive(Clone)]
struct JsPredicate(Rc<Function>);

impl Predicate<JsValue> for JsPredicate {
    fn evaluate(&self, event: &JsValue) -> bool {
        match self.0.call1(&JsValue::NULL, event) {
            Ok(v) => v.is_truthy(),
            Err(_) => false,
        }
    }

    /// Context-aware evaluation: the JS predicate is called with a second
    /// argument — a match-context object `{ length, first(), last(), all(),
    /// at(name) }` over the events captured so far — so `then`/`notThen`
    /// closures can correlate across steps (e.g. `(e, ctx) => e.id ===
    /// ctx.first().id`). One-arg closures ignore the extra argument.
    fn evaluate_in_context(&self, event: &JsValue, captures: &[(String, JsValue)]) -> bool {
        let ctx = build_context_object(captures);
        match self.0.call2(&JsValue::NULL, event, &ctx) {
            Ok(v) => v.is_truthy(),
            Err(_) => false,
        }
    }
}

// ── Unified predicate: a JS closure or (optionally) a compiled CEL program ──
//
// The engine uses a single predicate type `P`. `WasmPred` lets a pattern mix
// JS-closure steps and CEL-string steps. A CEL predicate is *serializable data*
// (the string), evaluated by the same `cel-interpreter` that runs server-side —
// the basis for server-pushable patterns and cross-tier predicate parity. Gated
// behind the `cel` feature so the default CEP bundle stays lean.

#[derive(Clone)]
enum WasmPred {
    Js(JsPredicate),
    #[cfg(feature = "cel")]
    Cel(Rc<cel_interpreter::Program>),
}

impl Predicate<JsValue> for WasmPred {
    fn evaluate(&self, event: &JsValue) -> bool {
        match self {
            WasmPred::Js(p) => p.evaluate(event),
            #[cfg(feature = "cel")]
            WasmPred::Cel(prog) => eval_cel(prog, event),
        }
    }
    fn evaluate_in_context(&self, event: &JsValue, captures: &[(String, JsValue)]) -> bool {
        match self {
            WasmPred::Js(p) => p.evaluate_in_context(event, captures),
            // CEL predicates see earlier captures as `first_*`/`prev_*`/`s{i}_*`
            // scalar variables (parity with the JS MatchContext).
            #[cfg(feature = "cel")]
            WasmPred::Cel(prog) => eval_cel_in_context(prog, event, captures),
        }
    }
}

/// Add an event's scalar fields to a CEL context, each variable named
/// `{prefix}{field}` (empty prefix → top-level, e.g. `kind`). Nested
/// objects/arrays are skipped (matching `tflo-cel`'s scalar convention).
#[cfg(feature = "cel")]
fn add_event_vars(ctx: &mut cel_interpreter::Context, event: &JsValue, prefix: &str) {
    let Some(serde_json::Value::Object(map)) = js_to_json(event) else {
        return;
    };
    for (k, v) in &map {
        let name = if prefix.is_empty() {
            k.clone()
        } else {
            format!("{prefix}{k}")
        };
        match v {
            serde_json::Value::String(s) => drop(ctx.add_variable(&name, s.clone())),
            serde_json::Value::Number(n) => {
                if let Some(i) = n.as_i64() {
                    drop(ctx.add_variable(&name, i));
                } else if let Some(f) = n.as_f64() {
                    drop(ctx.add_variable(&name, f));
                }
            }
            serde_json::Value::Bool(b) => drop(ctx.add_variable(&name, *b)),
            // Nested arrays/objects and null are skipped (scalar-only convention).
            serde_json::Value::Null
            | serde_json::Value::Array(_)
            | serde_json::Value::Object(_) => {}
        }
    }
}

/// Evaluate a compiled CEL program against an event (no cross-step context).
/// The event's scalar fields are top-level variables, so a predicate reads
/// `kind == "add_to_cart"`. Returns `false` on any error.
#[cfg(feature = "cel")]
fn eval_cel(prog: &cel_interpreter::Program, event: &JsValue) -> bool {
    eval_cel_in_context(prog, event, &[])
}

/// Context-aware CEL evaluation. In addition to the current event's fields
/// (top-level), the events captured by EARLIER steps are exposed as scalar
/// variables so a CEL predicate can correlate across steps — full parity with
/// the JS `MatchContext`:
///
/// - `first_<field>` — the first captured step's event (`ctx.first()`),
/// - `prev_<field>`  — the immediately-preceding captured event (`ctx.last()`),
/// - `s0_<field>`, `s1_<field>`, … — each captured step by position.
///
/// e.g. a `scroll` on the same product as the opening `view`:
/// `kind == "scroll" && productId == first_productId`.
#[cfg(feature = "cel")]
fn eval_cel_in_context(
    prog: &cel_interpreter::Program,
    event: &JsValue,
    captures: &[(String, JsValue)],
) -> bool {
    use cel_interpreter::{Context, Value};
    let mut ctx = Context::default();
    add_event_vars(&mut ctx, event, ""); // current event, top-level
    for (i, (_, cap)) in captures.iter().enumerate() {
        add_event_vars(&mut ctx, cap, &format!("s{i}_"));
    }
    if let Some((_, first)) = captures.first() {
        add_event_vars(&mut ctx, first, "first_");
    }
    if let Some((_, prev)) = captures.last() {
        add_event_vars(&mut ctx, prev, "prev_");
    }
    matches!(prog.execute(&ctx), Ok(Value::Bool(true)))
}

#[cfg(feature = "cel")]
fn js_to_json(event: &JsValue) -> Option<serde_json::Value> {
    let s = js_sys::JSON::stringify(event).ok()?.as_string()?;
    serde_json::from_str(&s).ok()
}

/// Upper bound on a CEL predicate's length. A predicate is event-matching logic,
/// not a program — realistic ones are well under this. The cap exists to bound
/// the parser's worst-case recursion (see `validate_cel`).
#[cfg(feature = "cel")]
const MAX_CEL_LEN: usize = 1024;

/// Upper bound on syntactic nesting depth (parentheses/brackets/braces and runs
/// of prefix operators). `cel-interpreter`'s parser is recursive-descent, so a
/// deeply-nested expression overflows the **native** stack — which in wasm is an
/// uncatchable `unreachable`/`memory access out of bounds` trap that poisons the
/// whole module instance (`catch_unwind` can't recover it under `panic=abort`).
/// We therefore reject pathological nesting *before* it reaches the parser.
#[cfg(feature = "cel")]
const MAX_CEL_DEPTH: usize = 32;

/// Validate a CEL string is safe to parse: bounded length and bounded nesting.
/// Returns a catchable `JsError` (never a panic/trap) for anything that could
/// drive the parser to overflow.
#[cfg(feature = "cel")]
fn validate_cel(expr: &str) -> Result<(), JsError> {
    if expr.len() > MAX_CEL_LEN {
        return Err(JsError::new(&format!(
            "CEL expression too long ({} bytes; max {MAX_CEL_LEN})",
            expr.len()
        )));
    }
    // Track the deepest simultaneous bracket nesting AND the longest run of
    // consecutive prefix operators (`!`/`-`) — both grow parser recursion depth.
    // Whitespace does not break a prefix run (`! ! x` still nests twice).
    let (mut depth, mut max_depth, mut run, mut max_run) = (0usize, 0usize, 0usize, 0usize);
    for b in expr.bytes() {
        match b {
            b'(' | b'[' | b'{' => {
                depth = depth.saturating_add(1);
                max_depth = max_depth.max(depth);
            }
            b')' | b']' | b'}' => depth = depth.saturating_sub(1),
            b'!' | b'-' => {
                run = run.saturating_add(1);
                max_run = max_run.max(run);
            }
            b' ' | b'\t' | b'\n' | b'\r' => {}
            _ => run = 0,
        }
    }
    let nesting = max_depth.max(max_run);
    if nesting > MAX_CEL_DEPTH {
        return Err(JsError::new(&format!(
            "CEL expression nests too deeply ({nesting} levels; max {MAX_CEL_DEPTH})"
        )));
    }
    Ok(())
}

#[cfg(feature = "cel")]
fn compile_cel(expr: &str) -> Result<WasmPred, JsError> {
    validate_cel(expr)?;
    cel_interpreter::Program::compile(expr)
        .map(|p| WasmPred::Cel(Rc::new(p)))
        .map_err(|e| JsError::new(&format!("CEL compile error in `{expr}`: {e}")))
}

/// `EmitCallback` adapter for a JS `Function`.
///
/// The closure receives a JS-side `Match` object — see [`build_match_object`].
struct JsEmit(Rc<Function>);

impl EmitCallback<JsValue, JsValue> for JsEmit {
    fn emit(&self, m: &Match<JsValue>) -> JsValue {
        let match_obj = build_match_object(m);
        self.0
            .call1(&JsValue::NULL, &match_obj)
            .unwrap_or(JsValue::UNDEFINED)
    }
}

/// `TimestampCallback` adapter for a JS `Function`. The function must
/// return a number; non-number returns are treated as `0`.
#[derive(Clone)]
struct JsTimestamp(Rc<Function>);

impl TimestampCallback<JsValue> for JsTimestamp {
    fn timestamp(&self, event: &JsValue) -> i64 {
        match self.0.call1(&JsValue::NULL, event) {
            Ok(v) => v.as_f64().map(|n| n as i64).unwrap_or(0),
            Err(_) => 0,
        }
    }
}

// ── Builder state held by WasmPattern during construction ───────────

#[derive(Clone)]
struct BuilderStep {
    name: String,
    predicate: WasmPred,
    within_ms: Option<i32>,
    is_negative: bool,
    contiguity: Contiguity,
    repeat: Option<RepeatSpec>,
    /// Interior-negation guard (positive steps only): an event matching this
    /// while the partial skips toward the step kills it ("no C between").
    forbidden: Option<WasmPred>,
}

impl BuilderStep {
    fn positive(name: String, predicate: WasmPred) -> Self {
        Self {
            name,
            predicate,
            within_ms: None,
            is_negative: false,
            contiguity: Contiguity::Eventually,
            repeat: None,
            forbidden: None,
        }
    }
    fn negative(name: String, predicate: WasmPred) -> Self {
        Self {
            name,
            predicate,
            within_ms: None,
            is_negative: true,
            contiguity: Contiguity::Eventually,
            repeat: None,
            forbidden: None,
        }
    }
}

#[derive(Clone, Default)]
struct PatternState {
    name: String,
    steps: Vec<BuilderStep>,
    timestamp_fn: Option<JsTimestamp>,
    auto_name_counter: u32,
    /// Interior-negation guard awaiting the next positive (`then`) step.
    pending_forbidden: Option<WasmPred>,
}

impl PatternState {
    fn with_name(name: String) -> Self {
        Self {
            name,
            ..Self::default()
        }
    }

    fn next_name(&mut self, prefix: &str) -> String {
        let n = self.auto_name_counter;
        self.auto_name_counter = self.auto_name_counter.saturating_add(1);
        format!("{prefix}_{n}")
    }
}

// ── Pattern builder ──────────────────────────────────────────────────

#[wasm_bindgen]
pub struct WasmPattern {
    inner: Rc<RefCell<PatternState>>,
}

#[wasm_bindgen]
impl WasmPattern {
    #[wasm_bindgen(constructor)]
    pub fn new(name: String) -> WasmPattern {
        WasmPattern {
            inner: Rc::new(RefCell::new(PatternState::with_name(name))),
        }
    }

    /// Set the event-time extractor. Required for correct `within(...)`
    /// behavior; without it, every event is treated as ts=0.
    pub fn timestamp(self, f: Function) -> WasmPattern {
        self.inner.borrow_mut().timestamp_fn = Some(JsTimestamp(Rc::new(f)));
        self
    }

    pub fn when(self, p: Function) -> WasmPattern {
        let name = self.inner.borrow_mut().next_name("when");
        let step = BuilderStep::positive(name, WasmPred::Js(JsPredicate(Rc::new(p))));
        {
            let mut s = self.inner.borrow_mut();
            if let Some(first) = s.steps.first_mut() {
                *first = step;
            } else {
                s.steps.push(step);
            }
        }
        self
    }

    pub fn then(self, p: Function) -> WasmPattern {
        let name = self.inner.borrow_mut().next_name("then");
        self.then_named(name, p)
    }

    #[wasm_bindgen(js_name = thenNamed)]
    pub fn then_named(self, name: String, p: Function) -> WasmPattern {
        let mut step = BuilderStep::positive(name, WasmPred::Js(JsPredicate(Rc::new(p))));
        let mut s = self.inner.borrow_mut();
        step.forbidden = s.pending_forbidden.take();
        s.steps.push(step);
        drop(s);
        self
    }

    /// Attach an **interior-negation guard** (JS predicate) to the next positive
    /// step: while the match skips events waiting for that step, any event
    /// satisfying `p` kills the partial. Mirrors native `not_between`.
    #[wasm_bindgen(js_name = notBetween)]
    pub fn not_between(self, p: Function) -> WasmPattern {
        self.inner.borrow_mut().pending_forbidden =
            Some(WasmPred::Js(JsPredicate(Rc::new(p))));
        self
    }

    #[wasm_bindgen(js_name = notThen)]
    pub fn not_then(self, p: Function) -> WasmPattern {
        let name = self.inner.borrow_mut().next_name("not_then");
        self.not_then_named(name, p)
    }

    #[wasm_bindgen(js_name = notThenNamed)]
    pub fn not_then_named(self, name: String, p: Function) -> WasmPattern {
        self.inner
            .borrow_mut()
            .steps
            .push(BuilderStep::negative(name, WasmPred::Js(JsPredicate(Rc::new(p)))));
        self
    }

    /// `when` with a CEL predicate (a serializable string). The expression
    /// references event fields directly — e.g. `kind == "add_to_cart"`.
    ///
    /// # Errors
    /// Throws if the CEL expression fails to compile.
    #[cfg(feature = "cel")]
    #[wasm_bindgen(js_name = whenCel)]
    pub fn when_cel(self, expr: &str) -> Result<WasmPattern, JsError> {
        let predicate = compile_cel(expr)?;
        let name = self.inner.borrow_mut().next_name("when");
        let step = BuilderStep::positive(name, predicate);
        {
            let mut s = self.inner.borrow_mut();
            if let Some(first) = s.steps.first_mut() {
                *first = step;
            } else {
                s.steps.push(step);
            }
        }
        Ok(self)
    }

    /// `then` with a CEL predicate string.
    ///
    /// # Errors
    /// Throws if the CEL expression fails to compile.
    #[cfg(feature = "cel")]
    #[wasm_bindgen(js_name = thenCel)]
    pub fn then_cel(self, expr: &str) -> Result<WasmPattern, JsError> {
        let predicate = compile_cel(expr)?;
        let name = self.inner.borrow_mut().next_name("then");
        let mut step = BuilderStep::positive(name, predicate);
        let mut s = self.inner.borrow_mut();
        step.forbidden = s.pending_forbidden.take();
        s.steps.push(step);
        drop(s);
        Ok(self)
    }

    /// `notBetween` with a CEL predicate string — the serializable interior-
    /// negation guard for the next `thenCel` step ("A then B with no C between").
    ///
    /// # Errors
    /// Throws if the CEL expression fails to compile.
    #[cfg(feature = "cel")]
    #[wasm_bindgen(js_name = notBetweenCel)]
    pub fn not_between_cel(self, expr: &str) -> Result<WasmPattern, JsError> {
        let predicate = compile_cel(expr)?;
        self.inner.borrow_mut().pending_forbidden = Some(predicate);
        Ok(self)
    }

    /// `notThen` with a CEL predicate string.
    ///
    /// # Errors
    /// Throws if the CEL expression fails to compile.
    #[cfg(feature = "cel")]
    #[wasm_bindgen(js_name = notThenCel)]
    pub fn not_then_cel(self, expr: &str) -> Result<WasmPattern, JsError> {
        let predicate = compile_cel(expr)?;
        let name = self.inner.borrow_mut().next_name("not_then");
        self.inner
            .borrow_mut()
            .steps
            .push(BuilderStep::negative(name, predicate));
        Ok(self)
    }

    /// Attach a within-bound (milliseconds) to the most recently added
    /// step. Capped at `i32::MAX` (~24.8 days) so the JS-facing param is
    /// `number`, not `bigint`.
    pub fn within(self, ms: i32) -> WasmPattern {
        if let Some(last) = self.inner.borrow_mut().steps.last_mut() {
            last.within_ms = Some(ms);
        }
        self
    }

    /// Make the most recently added step **strictly contiguous** (`next`): the
    /// event immediately following the previous capture must satisfy it, else the
    /// partial match dies — expressing "B directly follows A" / "no event between
    /// A and B". No-op on the initial `when` / on negatives.
    #[wasm_bindgen(js_name = next)]
    pub fn next_contiguous(self) -> WasmPattern {
        if let Some(last) = self.inner.borrow_mut().steps.last_mut() {
            last.contiguity = Contiguity::Next;
        }
        self
    }

    /// Make the most recently added step a `repeated(min..=max)` quantifier: it
    /// matches its predicate between `min` and `max` times (capturing each), then
    /// advances. `min`/`max` are clamped to `1 <= min <= max`.
    pub fn times(self, min: u32, max: u32) -> WasmPattern {
        let min = (min.max(1)) as usize;
        let max = (max as usize).max(min);
        if let Some(last) = self.inner.borrow_mut().steps.last_mut() {
            last.repeat = Some(RepeatSpec { min, max });
        }
        self
    }

    /// Finalize the pattern. Throws a `JsError` if the builder state is
    /// invalid (no `when`, `not_then` without `within`, `not_then` not
    /// terminal).
    ///
    /// # Errors
    ///
    /// Throws a JS-side `Error` when: (a) no `.when(...)` step was added,
    /// (b) a `.notThen(...)` step has no paired `.within(...)`, or (c) a
    /// `.notThen(...)` step is followed by another step (it must be
    /// terminal).
    pub fn emit(self, f: Function) -> Result<WasmCompiledPattern, JsError> {
        let s = self.inner.borrow();
        if s.steps.is_empty() {
            return Err(JsError::new(
                "pattern is missing the initial .when(...) step",
            ));
        }
        let last_idx = s.steps.len().saturating_sub(1);
        for (i, step) in s.steps.iter().enumerate() {
            if step.is_negative {
                if step.within_ms.is_none() {
                    return Err(JsError::new(&format!(
                        "step `{}` is notThen but has no .within(...) bound",
                        step.name
                    )));
                }
                if i != last_idx {
                    return Err(JsError::new(&format!(
                        "step `{}` is notThen and must be the last step",
                        step.name
                    )));
                }
            }
        }

        let engine_steps: Vec<Step<JsValue, WasmPred>> = s
            .steps
            .iter()
            .map(|bs| {
                let within = bs.within_ms.map(i64::from);
                if bs.is_negative {
                    Step::negative(bs.name.clone(), bs.predicate.clone(), within)
                } else {
                    let mut step = Step::positive(bs.name.clone(), bs.predicate.clone(), within)
                        .with_contiguity(bs.contiguity);
                    if let Some(spec) = bs.repeat {
                        step = step.with_repeat(spec);
                    }
                    if let Some(guard) = bs.forbidden.clone() {
                        step = step.with_forbidden(guard);
                    }
                    step
                }
            })
            .collect();

        let emit = JsEmit(Rc::new(f));
        let ts_fn = s.timestamp_fn.as_ref().map(|t| JsTimestamp(t.0.clone()));
        let name = s.name.clone();
        drop(s);

        let compiled = Compiled::new(name, engine_steps, emit, ts_fn);
        Ok(WasmCompiledPattern {
            compiled: Some(compiled),
        })
    }
}

#[wasm_bindgen]
pub struct WasmCompiledPattern {
    compiled: Option<Compiled<JsValue, JsValue, WasmPred, JsEmit, JsTimestamp>>,
}

#[wasm_bindgen]
impl WasmCompiledPattern {
    #[wasm_bindgen(getter)]
    pub fn name(&self) -> String {
        self.compiled
            .as_ref()
            .map(|c| c.name.clone())
            .unwrap_or_default()
    }
}

// ── Runtime — thin wrapper around engine::Runtime ───────────────────

#[wasm_bindgen]
pub struct WasmPatternRuntime {
    inner: Runtime<JsValue, JsValue, WasmPred, JsEmit, JsTimestamp>,
}

#[wasm_bindgen]
impl WasmPatternRuntime {
    /// Wrap a finalized compiled pattern in a streaming runtime.
    ///
    /// # Errors
    ///
    /// Throws if the compiled pattern was already consumed (each
    /// `WasmCompiledPattern` produces exactly one runtime).
    /// Wrap a finalized compiled pattern in a streaming runtime. Pass
    /// `maxLatenessMs` to enable the event-time **reorder buffer**: events that
    /// arrive up to that many ms out of order are released to the matcher in
    /// event-time order (later-than-that drop as `"late"`). Omit it for the
    /// default in-order behaviour.
    #[wasm_bindgen(constructor)]
    pub fn new(
        mut pattern: WasmCompiledPattern,
        max_lateness_ms: Option<f64>,
    ) -> Result<WasmPatternRuntime, JsError> {
        let compiled = pattern
            .compiled
            .take()
            .ok_or_else(|| JsError::new("compiled pattern already consumed"))?;
        let mut inner = Runtime::new(compiled);
        if let Some(ms) = max_lateness_ms {
            inner = inner.with_reorder(ms as i64);
        }
        Ok(WasmPatternRuntime { inner })
    }

    /// Push one event through the runtime. Returns a JS array of any
    /// signals emitted by this event.
    pub fn push(&mut self, event: JsValue) -> Array {
        let emitted = self.inner.push(event);
        let arr = Array::new();
        for v in emitted {
            arr.push(&v);
        }
        arr
    }

    /// Signal end of input. Drains any pending negative-step matches.
    pub fn flush(&mut self) -> Array {
        let emitted = self.inner.flush();
        let arr = Array::new();
        for v in emitted {
            arr.push(&v);
        }
        arr
    }

    /// Advance logical time to `now` (ms) **without** consuming an event, firing
    /// any deadline-reached matches — e.g. "A then no B within T" fires on
    /// absence. Returns a JS array of emitted signals. The host drives this from
    /// its clock (the browser provider schedules it at [`nextDeadline`]); the
    /// engine reads no wall clock, so a tick-driven run stays byte-identical to an
    /// event-driven one. `now` is a JS `number` (ms), matching the timestamp
    /// convention.
    pub fn tick(&mut self, now: f64) -> Array {
        let emitted = self.inner.tick(now as i64);
        let arr = Array::new();
        for v in emitted {
            arr.push(&v);
        }
        arr
    }

    /// The earliest pending deadline (ms) across in-flight partials, or
    /// `undefined` if no timer is pending. The host schedules its next
    /// [`tick`](Self::tick) for this instant.
    #[wasm_bindgen(getter, js_name = nextDeadline)]
    pub fn next_deadline(&self) -> Option<f64> {
        self.inner.next_deadline().map(|d| d as f64)
    }

    /// Reset to the just-constructed state. The compiled pattern is kept;
    /// in-flight partial matches are dropped.
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    /// Register a JS callback invoked once for every partial match the
    /// runtime discards. The callback receives the drop reason as a string:
    /// `"max_in_flight"` (the bounded in-flight set overflowed) or
    /// `"deadline"` (a positive step's `within` window closed unmatched).
    /// Replaces any previously-set handler.
    #[wasm_bindgen(js_name = setDropHandler)]
    pub fn set_drop_handler(&mut self, handler: Function) {
        let cb = Rc::new(handler);
        self.inner.set_drop_handler(move |reason: DropReason| {
            let label = match reason {
                DropReason::MaxInFlight => "max_in_flight",
                DropReason::Deadline => "deadline",
                DropReason::Late => "late",
            };
            drop(cb.call1(&JsValue::NULL, &JsValue::from_str(label)));
        });
    }

    /// Total number of partial matches dropped over this runtime's lifetime
    /// (both `max_in_flight` and `deadline` reasons). Monotonic.
    #[wasm_bindgen(getter, js_name = droppedCount)]
    pub fn dropped_count(&self) -> usize {
        self.inner.dropped_count()
    }
}

/// Build a plain JS object for `Match<E>` to pass to the emit callback.
///
/// Surface: `{ patternName, length, first(), last(), all(), at(name) }`.
/// Methods are bound closures so the JS caller sees a method-style
/// interface.
fn build_match_object(m: &Match<JsValue>) -> JsValue {
    let obj = js_sys::Object::new();
    drop(js_sys::Reflect::set(
        &obj,
        &JsValue::from_str("patternName"),
        &JsValue::from_str(m.pattern_name()),
    ));
    drop(js_sys::Reflect::set(
        &obj,
        &JsValue::from_str("length"),
        &JsValue::from_f64(m.len() as f64),
    ));

    // Collect captures via the public Match API (first/last/all/at).
    let all_vec: Vec<JsValue> = m.all().into_iter().cloned().collect();
    let captures_rc = Rc::new(all_vec);

    let first_captures = captures_rc.clone();
    let first = Closure::<dyn Fn() -> JsValue>::new(move || {
        first_captures
            .first()
            .cloned()
            .unwrap_or(JsValue::UNDEFINED)
    });
    drop(js_sys::Reflect::set(
        &obj,
        &JsValue::from_str("first"),
        first.as_ref(),
    ));
    first.forget();

    let last_captures = captures_rc.clone();
    let last = Closure::<dyn Fn() -> JsValue>::new(move || {
        last_captures.last().cloned().unwrap_or(JsValue::UNDEFINED)
    });
    drop(js_sys::Reflect::set(
        &obj,
        &JsValue::from_str("last"),
        last.as_ref(),
    ));
    last.forget();

    let all_captures = captures_rc;
    let all = Closure::<dyn Fn() -> JsValue>::new(move || {
        let arr = Array::new();
        for v in all_captures.iter() {
            arr.push(v);
        }
        arr.into()
    });
    drop(js_sys::Reflect::set(
        &obj,
        &JsValue::from_str("all"),
        all.as_ref(),
    ));
    all.forget();

    // For step-name lookup, use Match::named() to iterate (name, event)
    // pairs directly.
    let pairs: Rc<Vec<(String, JsValue)>> =
        Rc::new(m.named().map(|(n, v)| (n.to_string(), v.clone())).collect());
    let at_pairs = pairs;
    let at = Closure::<dyn Fn(String) -> JsValue>::new(move |name: String| {
        at_pairs
            .iter()
            .find(|(n, _)| n == &name)
            .map(|(_, v)| v.clone())
            .unwrap_or(JsValue::UNDEFINED)
    });
    drop(js_sys::Reflect::set(
        &obj,
        &JsValue::from_str("at"),
        at.as_ref(),
    ));
    at.forget();

    obj.into()
}

/// Build a JS context object for the events captured so far in a partial
/// match — passed as the 2nd argument to `then`/`notThen` predicates so they
/// can correlate across steps. Surface: `{ length, first(), last(), all(),
/// at(name) }`.
fn build_context_object(captures: &[(String, JsValue)]) -> JsValue {
    let obj = js_sys::Object::new();
    drop(js_sys::Reflect::set(
        &obj,
        &JsValue::from_str("length"),
        &JsValue::from_f64(captures.len() as f64),
    ));

    let all_vec: Vec<JsValue> = captures.iter().map(|(_, e)| e.clone()).collect();
    let captures_rc = Rc::new(all_vec);

    let first_c = captures_rc.clone();
    let first = Closure::<dyn Fn() -> JsValue>::new(move || {
        first_c.first().cloned().unwrap_or(JsValue::UNDEFINED)
    });
    drop(js_sys::Reflect::set(
        &obj,
        &JsValue::from_str("first"),
        first.as_ref(),
    ));
    first.forget();

    let last_c = captures_rc.clone();
    let last = Closure::<dyn Fn() -> JsValue>::new(move || {
        last_c.last().cloned().unwrap_or(JsValue::UNDEFINED)
    });
    drop(js_sys::Reflect::set(
        &obj,
        &JsValue::from_str("last"),
        last.as_ref(),
    ));
    last.forget();

    let all_c = captures_rc;
    let all = Closure::<dyn Fn() -> JsValue>::new(move || {
        let arr = Array::new();
        for v in all_c.iter() {
            arr.push(v);
        }
        arr.into()
    });
    drop(js_sys::Reflect::set(
        &obj,
        &JsValue::from_str("all"),
        all.as_ref(),
    ));
    all.forget();

    let pairs: Rc<Vec<(String, JsValue)>> = Rc::new(captures.to_vec());
    let at = Closure::<dyn Fn(String) -> JsValue>::new(move |name: String| {
        pairs
            .iter()
            .find(|(n, _)| n == &name)
            .map(|(_, v)| v.clone())
            .unwrap_or(JsValue::UNDEFINED)
    });
    drop(js_sys::Reflect::set(
        &obj,
        &JsValue::from_str("at"),
        at.as_ref(),
    ));
    at.forget();

    obj.into()
}
