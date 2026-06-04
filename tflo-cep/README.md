# tflo-cep

Closure-based event-pattern matching for the
[`tflo`](https://github.com/matt-cochran/tflo) temporal event processing
engine. Composes typed signals into multi-event domain signals like
"user added to cart but did not purchase within 5 minutes" — and runs in
browser WASM, on edge gateways, and inside any Rust service the rest of
`tflo` already deploys to.

> Pre-1.0, experimental. The API will change. Not yet published to
> crates.io.

## What it is

A small iterator-adapter API on top of the existing `tflo` engine
primitives. Patterns are built from closures, the same way `tflo-cel`,
`tflo-rhai`, and `tflo-rego` work — predicates are `Fn(&E) -> bool`,
emit is `Fn(&Match<E>) -> M`. The crate is `wasm32`-clean by
construction: no I/O, no async runtime, no system deps.

## Quick start

```rust
use tflo_cep::prelude::*;
use std::time::Duration;

#[derive(Clone)]
struct Event { ts: i64, action: &'static str }

let abandoned_cart = Pattern::<Event>::new("abandoned_cart")
    .timestamp(|e| e.ts)
    .when(|e| e.action == "add_to_cart")
    .not_then(|e| e.action == "purchase")
    .within(Duration::from_secs(300))
    .emit(|m| format!("abandoned cart added at ts={}", m.first().ts))
    .expect("pattern is well-formed");

let signals: Vec<String> = events.into_iter()
    .match_pattern(abandoned_cart)
    .collect();
```

## The v0.1 surface

Six methods, all closure-based:

| Method | Purpose |
|---|---|
| `Pattern::new(name)` | Begin a pattern |
| `.timestamp(\|e\| e.ts)` | Event-time extractor (required) |
| `.when(\|e\| ...)` | Initial match — opens a partial match |
| `.then(\|e\| ...)` | Positive sequential — advances a partial match |
| `.not_then(\|e\| ...)` | Negative terminal — succeeds when no matching event arrives |
| `.within(Duration)` | Time bound modifier on the previous step |
| `.emit(\|m\| ...)` | Output transformer; returns `Result<Pattern, PatternError>` |

Plus `.then_named(...)` / `.not_then_named(...)` for explicit step
names (used by `Match::at("name")`).

## Use cases from the design discussion

The integration suite covers the shapes a real browser-analytics SDK needs:

- **`abandoned_cart`** — `when add_to_cart, not_then purchase within 5 min`
- **`engaged_with_product`** — `when product_view, then deep_scroll within 30 s`
- **`rage_click`** — three same-target clicks within one second, expressed as `when → then within 1 s → then within 1 s` (the `repeated(n..=m, ...)` quantifier sugar lands in v0.2)

End-of-stream resolves any pending negative matches, so streams that
terminate before a deadline still emit their cart-abandonment signals.

## Bounded by construction

Partial matches are bounded by event rate within the `within` window and
hard-capped at `MAX_IN_FLIGHT = 1024` per stream — when exceeded, the
oldest partial match is dropped. This is the
[bounded-vs-event-rate](https://github.com/matt-cochran/tflo/blob/main/docs/non-goals.md)
guarantee that keeps `tflo-cep` honest about edge memory.

## What's not in v0.1 (deferred to v0.2)

- `repeated(n..=m, predicate)` — bounded quantifier sugar
- CEL string predicates (`when("event.action == 'foo'")`)
  via `IntoPredicate` trait — closures only today
- `MatchContext` for iterative conditions (`B > A.value`)
- Graph-DSL integration (`Operator` impl) — iterator-only today
- Strict contiguity (`next` vs `eventually`) — relaxed only today

Each is one focused addition; the runtime substrate is shared.

## License

MIT OR Apache-2.0.
