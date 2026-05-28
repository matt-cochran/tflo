#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]
#![deny(clippy::print_stdout)]
#![allow(clippy::use_self, clippy::missing_const_for_fn, clippy::needless_pass_by_value)]
#![allow(missing_docs)] // wasm-bindgen-exposed types document themselves via TS .d.ts emission

//! WebAssembly bindings for [`tflo-cep`].
//!
//! The bindings present three classes to JS:
//!
//! - [`WasmPattern`] — the builder. Mirrors the Rust `Pattern` surface but
//!   accepts JS `Function` callbacks for predicates and emit.
//! - [`WasmCompiledPattern`] — finalized pattern, ready to drive a runtime.
//! - [`WasmPatternRuntime`] — the streaming state machine. Push events one
//!   at a time, collect emitted signals.
//!
//! ## Why a parallel runtime
//!
//! The Rust `Pattern<E, M>` requires `Send + Sync` predicates because it
//! supports multi-threaded use upstream. WASM is single-threaded, and
//! `js_sys::Function` is not `Send`. The bindings replicate the small
//! state machine using `Rc<RefCell<...>>` and `js_sys::Function` directly —
//! ~150 lines that parallel `tflo-cep`'s `runtime.rs` without taking a
//! direct dependency on its internal types.

use std::cell::RefCell;
use std::rc::Rc;

use js_sys::{Array, Function};
use wasm_bindgen::prelude::*;

/// Initialize the panic hook for better error messages in the browser
/// console. Idempotent — safe to call multiple times.
#[wasm_bindgen(start)]
pub fn start() {
    console_error_panic_hook::set_once();
}

/// Maximum simultaneous in-flight partial matches per runtime. Mirrors
/// the Rust crate's `MAX_IN_FLIGHT`.
const MAX_IN_FLIGHT: usize = 1024;

// ── Internal step representation ─────────────────────────────────────

#[derive(Clone)]
struct Step {
    name: String,
    predicate: Function,
    /// Within-bound in milliseconds. Capped at `i32::MAX` (~24.8 days) so
    /// the JS-facing parameter is `number`, not `bigint`. Longer bounds
    /// are not the v0.1 sweet spot for browser analytics.
    within_ms: Option<i32>,
    is_negative: bool,
}

#[derive(Clone)]
struct PatternState {
    name: String,
    steps: Vec<Step>,
    timestamp_fn: Option<Function>,
    emit_fn: Option<Function>,
    auto_name_counter: u32,
}

impl PatternState {
    fn new(name: String) -> Self {
        Self {
            name,
            steps: Vec::new(),
            timestamp_fn: None,
            emit_fn: None,
            auto_name_counter: 0,
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
            inner: Rc::new(RefCell::new(PatternState::new(name))),
        }
    }

    /// Set the event-time extractor. Required for correct `within(...)`
    /// behavior; without it, every event is treated as ts=0.
    pub fn timestamp(self, f: Function) -> WasmPattern {
        self.inner.borrow_mut().timestamp_fn = Some(f);
        self
    }

    pub fn when(self, p: Function) -> WasmPattern {
        let name = self.inner.borrow_mut().next_name("when");
        let step = Step {
            name,
            predicate: p,
            within_ms: None,
            is_negative: false,
        };
        {
            let mut s = self.inner.borrow_mut();
            if s.steps.is_empty() {
                s.steps.push(step);
            } else {
                s.steps[0] = step;
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
        self.inner.borrow_mut().steps.push(Step {
            name,
            predicate: p,
            within_ms: None,
            is_negative: false,
        });
        self
    }

    #[wasm_bindgen(js_name = notThen)]
    pub fn not_then(self, p: Function) -> WasmPattern {
        let name = self.inner.borrow_mut().next_name("not_then");
        self.not_then_named(name, p)
    }

    #[wasm_bindgen(js_name = notThenNamed)]
    pub fn not_then_named(self, name: String, p: Function) -> WasmPattern {
        self.inner.borrow_mut().steps.push(Step {
            name,
            predicate: p,
            within_ms: None,
            is_negative: true,
        });
        self
    }

    /// Attach a within-bound (milliseconds) to the most recently added
    /// step. Saturating-cast to a 32-bit value (max ~24.8 days).
    pub fn within(self, ms: i32) -> WasmPattern {
        if let Some(last) = self.inner.borrow_mut().steps.last_mut() {
            last.within_ms = Some(ms);
        }
        self
    }

    /// Finalize the pattern. Throws a `JsError` if the builder state is
    /// invalid (no `when`, `not_then` without `within`, `not_then` not
    /// terminal).
    pub fn emit(self, f: Function) -> Result<WasmCompiledPattern, JsError> {
        let mut s = self.inner.borrow_mut();
        if s.steps.is_empty() {
            return Err(JsError::new("pattern is missing the initial .when(...) step"));
        }
        for (i, step) in s.steps.iter().enumerate() {
            if step.is_negative {
                if step.within_ms.is_none() {
                    return Err(JsError::new(&format!(
                        "step `{}` is notThen but has no .within(...) bound",
                        step.name
                    )));
                }
                if i != s.steps.len() - 1 {
                    return Err(JsError::new(&format!(
                        "step `{}` is notThen and must be the last step",
                        step.name
                    )));
                }
            }
        }
        s.emit_fn = Some(f);
        drop(s);
        Ok(WasmCompiledPattern {
            inner: self.inner.clone(),
        })
    }
}

#[wasm_bindgen]
pub struct WasmCompiledPattern {
    inner: Rc<RefCell<PatternState>>,
}

#[wasm_bindgen]
impl WasmCompiledPattern {
    #[wasm_bindgen(getter)]
    pub fn name(&self) -> String {
        self.inner.borrow().name.clone()
    }
}

// ── Runtime state machine ────────────────────────────────────────────

struct PartialMatch {
    captures: Vec<(String, JsValue)>,
    next_step: usize,
    deadline_ts: Option<i64>,
}

#[wasm_bindgen]
pub struct WasmPatternRuntime {
    state: Rc<RefCell<PatternState>>,
    in_flight: Vec<PartialMatch>,
    exhausted: bool,
    drained_negatives: bool,
}

#[wasm_bindgen]
impl WasmPatternRuntime {
    #[wasm_bindgen(constructor)]
    pub fn new(pattern: WasmCompiledPattern) -> WasmPatternRuntime {
        WasmPatternRuntime {
            state: pattern.inner,
            in_flight: Vec::new(),
            exhausted: false,
            drained_negatives: false,
        }
    }

    /// Push one event through the runtime. Returns a JS array of any
    /// signals emitted by this event.
    ///
    /// Throws `JsError` if a predicate or emit callback throws. JS
    /// predicate truthiness rules apply — anything other than `false`,
    /// `0`, `""`, `null`, `undefined`, or `NaN` is treated as match.
    pub fn push(&mut self, event: JsValue) -> Result<Array, JsError> {
        let signals = Array::new();
        let ts = self.timestamp_of(&event)?;
        self.advance_deadlines(ts, &signals)?;
        self.advance_partials(&event, ts, &signals)?;
        self.try_open_new(&event, ts, &signals)?;
        Ok(signals)
    }

    /// Signal end of input. Drains any pending negative-step matches whose
    /// deadlines have not yet been reached — semantically, a stream that
    /// ends is the same as a deadline that never closed. Returns the
    /// emitted signals.
    pub fn flush(&mut self) -> Result<Array, JsError> {
        let signals = Array::new();
        if !self.exhausted {
            self.exhausted = true;
        }
        if !self.drained_negatives {
            self.drain_negatives(&signals)?;
            self.drained_negatives = true;
        }
        Ok(signals)
    }

    /// Reset to the just-constructed state. The compiled pattern is kept;
    /// in-flight partial matches are dropped.
    pub fn reset(&mut self) {
        self.in_flight.clear();
        self.exhausted = false;
        self.drained_negatives = false;
    }

    // ── Internal helpers ──────────────────────────────────────────────

    fn timestamp_of(&self, e: &JsValue) -> Result<i64, JsError> {
        let s = self.state.borrow();
        let Some(f) = s.timestamp_fn.as_ref() else {
            return Ok(0);
        };
        let result = f
            .call1(&JsValue::NULL, e)
            .map_err(|e| JsError::new(&format!("timestamp callback threw: {e:?}")))?;
        result
            .as_f64()
            .map(|v| v as i64)
            .ok_or_else(|| JsError::new("timestamp callback did not return a number"))
    }

    fn predicate_holds(&self, predicate: &Function, event: &JsValue) -> Result<bool, JsError> {
        let result = predicate
            .call1(&JsValue::NULL, event)
            .map_err(|e| JsError::new(&format!("predicate threw: {e:?}")))?;
        Ok(result.is_truthy())
    }

    fn next_deadline_for(&self, step_idx: usize, ref_ts: i64) -> Option<i64> {
        let s = self.state.borrow();
        if step_idx >= s.steps.len() {
            return None;
        }
        s.steps[step_idx]
            .within_ms
            .map(|ms| ref_ts.saturating_add(i64::from(ms)))
    }

    fn advance_deadlines(&mut self, ts: i64, signals: &Array) -> Result<(), JsError> {
        let mut i = 0;
        while i < self.in_flight.len() {
            let expired = match self.in_flight[i].deadline_ts {
                Some(d) => ts > d,
                None => false,
            };
            if !expired {
                i = i.saturating_add(1);
                continue;
            }
            let step_idx = self.in_flight[i].next_step;
            let is_negative = {
                let s = self.state.borrow();
                s.steps[step_idx].is_negative
            };
            if is_negative {
                let pm = self.in_flight.remove(i);
                self.complete(pm, signals)?;
            } else {
                self.in_flight.remove(i);
            }
        }
        Ok(())
    }

    fn advance_partials(
        &mut self,
        event: &JsValue,
        ts: i64,
        signals: &Array,
    ) -> Result<(), JsError> {
        let mut to_remove: Vec<usize> = Vec::new();
        let mut completions: Vec<PartialMatch> = Vec::new();
        for idx in (0..self.in_flight.len()).rev() {
            let (step_idx, step_name, is_negative, predicate) = {
                let pm = &self.in_flight[idx];
                let s = self.state.borrow();
                let step = &s.steps[pm.next_step];
                (
                    pm.next_step,
                    step.name.clone(),
                    step.is_negative,
                    step.predicate.clone(),
                )
            };
            let matches = self.predicate_holds(&predicate, event)?;
            if !matches {
                continue;
            }
            if is_negative {
                to_remove.push(idx);
                continue;
            }
            let new_next_step = step_idx.saturating_add(1);
            let new_deadline = self.next_deadline_for(new_next_step, ts);
            {
                let pm = &mut self.in_flight[idx];
                pm.captures.push((step_name, event.clone()));
                pm.next_step = new_next_step;
                pm.deadline_ts = new_deadline;
            }
            let total_steps = self.state.borrow().steps.len();
            if new_next_step >= total_steps {
                completions.push(self.in_flight.remove(idx));
            }
        }
        for idx in to_remove {
            self.in_flight.remove(idx);
        }
        for pm in completions {
            self.complete(pm, signals)?;
        }
        Ok(())
    }

    fn try_open_new(
        &mut self,
        event: &JsValue,
        ts: i64,
        signals: &Array,
    ) -> Result<(), JsError> {
        let (when_pred, when_name, total_steps) = {
            let s = self.state.borrow();
            (
                s.steps[0].predicate.clone(),
                s.steps[0].name.clone(),
                s.steps.len(),
            )
        };
        if !self.predicate_holds(&when_pred, event)? {
            return Ok(());
        }
        let new_pm = PartialMatch {
            captures: vec![(when_name, event.clone())],
            next_step: 1,
            deadline_ts: self.next_deadline_for(1, ts),
        };
        if total_steps == 1 {
            self.complete(new_pm, signals)?;
        } else {
            self.in_flight.push(new_pm);
            if self.in_flight.len() > MAX_IN_FLIGHT {
                self.in_flight.remove(0);
            }
        }
        Ok(())
    }

    fn drain_negatives(&mut self, signals: &Array) -> Result<(), JsError> {
        let mut i = 0;
        while i < self.in_flight.len() {
            let is_negative = {
                let s = self.state.borrow();
                s.steps[self.in_flight[i].next_step].is_negative
            };
            if is_negative {
                let pm = self.in_flight.remove(i);
                self.complete(pm, signals)?;
            } else {
                i = i.saturating_add(1);
            }
        }
        Ok(())
    }

    fn complete(&self, pm: PartialMatch, signals: &Array) -> Result<(), JsError> {
        let m = build_match_object(&self.state.borrow().name, &pm.captures);
        let emit_fn = {
            let s = self.state.borrow();
            s.emit_fn
                .clone()
                .ok_or_else(|| JsError::new("compiled pattern is missing emit callback"))?
        };
        let signal = emit_fn
            .call1(&JsValue::NULL, &m)
            .map_err(|e| JsError::new(&format!("emit callback threw: {e:?}")))?;
        signals.push(&signal);
        Ok(())
    }
}

/// Build a plain JS object for `Match<E>` to pass to the emit callback.
///
/// Surface: `{ patternName, length, first(), last(), all(), at(name) }`.
/// Implemented as object methods rather than as a class to keep the
/// allocation-per-emit cost low and stay portable across JS runtimes.
fn build_match_object(pattern_name: &str, captures: &[(String, JsValue)]) -> JsValue {
    let obj = js_sys::Object::new();
    let _ = js_sys::Reflect::set(
        &obj,
        &JsValue::from_str("patternName"),
        &JsValue::from_str(pattern_name),
    );
    let _ = js_sys::Reflect::set(
        &obj,
        &JsValue::from_str("length"),
        &JsValue::from_f64(captures.len() as f64),
    );
    // `first` / `last` / `all` / `at` exposed as bound closures so the JS
    // caller sees a method-style interface.
    let captures_rc = Rc::new(captures.to_vec());
    let first_captures = captures_rc.clone();
    let first = Closure::<dyn Fn() -> JsValue>::new(move || {
        first_captures
            .first()
            .map(|(_, v)| v.clone())
            .unwrap_or(JsValue::UNDEFINED)
    });
    let _ = js_sys::Reflect::set(&obj, &JsValue::from_str("first"), first.as_ref());
    first.forget();

    let last_captures = captures_rc.clone();
    let last = Closure::<dyn Fn() -> JsValue>::new(move || {
        last_captures
            .last()
            .map(|(_, v)| v.clone())
            .unwrap_or(JsValue::UNDEFINED)
    });
    let _ = js_sys::Reflect::set(&obj, &JsValue::from_str("last"), last.as_ref());
    last.forget();

    let all_captures = captures_rc.clone();
    let all = Closure::<dyn Fn() -> JsValue>::new(move || {
        let arr = Array::new();
        for (_, v) in all_captures.iter() {
            arr.push(v);
        }
        arr.into()
    });
    let _ = js_sys::Reflect::set(&obj, &JsValue::from_str("all"), all.as_ref());
    all.forget();

    let at_captures = captures_rc;
    let at = Closure::<dyn Fn(String) -> JsValue>::new(move |name: String| {
        at_captures
            .iter()
            .find(|(n, _)| n == &name)
            .map(|(_, v)| v.clone())
            .unwrap_or(JsValue::UNDEFINED)
    });
    let _ = js_sys::Reflect::set(&obj, &JsValue::from_str("at"), at.as_ref());
    at.forget();

    obj.into()
}
