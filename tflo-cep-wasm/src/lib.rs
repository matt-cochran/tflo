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

use tflo_cep::engine::{
    Compiled, EmitCallback, Predicate, Runtime, Step, TimestampCallback,
};
use tflo_cep::Match;

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
}

/// `EmitCallback` adapter for a JS `Function`.
///
/// The closure receives a JS-side `Match` object — see [`build_match_object`].
struct JsEmit(Rc<Function>);

impl EmitCallback<JsValue, JsValue> for JsEmit {
    fn emit(&self, m: &Match<JsValue>) -> JsValue {
        let match_obj = build_match_object(m);
        self.0.call1(&JsValue::NULL, &match_obj).unwrap_or(JsValue::UNDEFINED)
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
    predicate: JsPredicate,
    within_ms: Option<i32>,
    is_negative: bool,
}

#[derive(Clone, Default)]
struct PatternState {
    name: String,
    steps: Vec<BuilderStep>,
    timestamp_fn: Option<JsTimestamp>,
    auto_name_counter: u32,
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
        let step = BuilderStep {
            name,
            predicate: JsPredicate(Rc::new(p)),
            within_ms: None,
            is_negative: false,
        };
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
        self.inner.borrow_mut().steps.push(BuilderStep {
            name,
            predicate: JsPredicate(Rc::new(p)),
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
        self.inner.borrow_mut().steps.push(BuilderStep {
            name,
            predicate: JsPredicate(Rc::new(p)),
            within_ms: None,
            is_negative: true,
        });
        self
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
            return Err(JsError::new("pattern is missing the initial .when(...) step"));
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

        let engine_steps: Vec<Step<JsValue, JsPredicate>> = s
            .steps
            .iter()
            .map(|bs| {
                let within = bs.within_ms.map(i64::from);
                if bs.is_negative {
                    Step::negative(bs.name.clone(), bs.predicate.clone(), within)
                } else {
                    Step::positive(bs.name.clone(), bs.predicate.clone(), within)
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
    compiled: Option<Compiled<JsValue, JsValue, JsPredicate, JsEmit, JsTimestamp>>,
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
    inner: Runtime<JsValue, JsValue, JsPredicate, JsEmit, JsTimestamp>,
}

#[wasm_bindgen]
impl WasmPatternRuntime {
    /// Wrap a finalized compiled pattern in a streaming runtime.
    ///
    /// # Errors
    ///
    /// Throws if the compiled pattern was already consumed (each
    /// `WasmCompiledPattern` produces exactly one runtime).
    #[wasm_bindgen(constructor)]
    pub fn new(mut pattern: WasmCompiledPattern) -> Result<WasmPatternRuntime, JsError> {
        let compiled = pattern
            .compiled
            .take()
            .ok_or_else(|| JsError::new("compiled pattern already consumed"))?;
        Ok(WasmPatternRuntime {
            inner: Runtime::new(compiled),
        })
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

    /// Reset to the just-constructed state. The compiled pattern is kept;
    /// in-flight partial matches are dropped.
    pub fn reset(&mut self) {
        self.inner.reset();
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
    drop(js_sys::Reflect::set(&obj, &JsValue::from_str("first"), first.as_ref()));
    first.forget();

    let last_captures = captures_rc.clone();
    let last = Closure::<dyn Fn() -> JsValue>::new(move || {
        last_captures
            .last()
            .cloned()
            .unwrap_or(JsValue::UNDEFINED)
    });
    drop(js_sys::Reflect::set(&obj, &JsValue::from_str("last"), last.as_ref()));
    last.forget();

    let all_captures = captures_rc;
    let all = Closure::<dyn Fn() -> JsValue>::new(move || {
        let arr = Array::new();
        for v in all_captures.iter() {
            arr.push(v);
        }
        arr.into()
    });
    drop(js_sys::Reflect::set(&obj, &JsValue::from_str("all"), all.as_ref()));
    all.forget();

    // For step-name lookup, use Match::named() to iterate (name, event)
    // pairs directly.
    let pairs: Rc<Vec<(String, JsValue)>> = Rc::new(
        m.named()
            .map(|(n, v)| (n.to_string(), v.clone()))
            .collect(),
    );
    let at_pairs = pairs;
    let at = Closure::<dyn Fn(String) -> JsValue>::new(move |name: String| {
        at_pairs
            .iter()
            .find(|(n, _)| n == &name)
            .map(|(_, v)| v.clone())
            .unwrap_or(JsValue::UNDEFINED)
    });
    drop(js_sys::Reflect::set(&obj, &JsValue::from_str("at"), at.as_ref()));
    at.forget();

    obj.into()
}
