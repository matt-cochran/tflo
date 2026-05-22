# Clippy lint backlog

The pre-open-source hardening pass enabled **panic-freedom** lints
(`unwrap_used`, `expect_used`, `panic`, `unreachable`, `todo` at `deny`) and,
to land that change without an unrelated 1500-line diff, **temporarily
suppressed** the `clippy::pedantic` and `clippy::nursery` groups — plus the
individual `clippy::type_complexity` lint — in `[workspace.lints.clippy]`
(`Cargo.toml`).

This document is the backlog for re-enabling them, one lint at a time, so the
suppression is a deliberate staged migration rather than a silent loss of
coverage.

## How to re-enable a lint

1. Pick the next lint from the table below (top of the list = do first).
2. Promote it from the suppressed group to its own entry in
   `[workspace.lints.clippy]`, e.g.:
   ```toml
   uninlined_format_args = "warn"
   ```
   (specific lints have higher priority than the `pedantic`/`nursery`
   group entries, so this opts a single lint back in).
3. `cargo clippy --fix` for the mechanical ones, then hand-fix the remainder.
4. Once a lint is clean, bump it to `"deny"` so it cannot regress.
5. When every lint in a group has its own entry, flip the group itself back
   to `"warn"` and delete the now-redundant per-lint entries.

## Backlog — ordered easiest first

Counts are workspace-wide warning totals observed at the start of the
hardening pass; they shrink as code changes.

| Batch | Lint | Group | ~Count | Notes |
|------:|------|-------|-------:|-------|
| 1 | `uninlined_format_args` | pedantic | 22 | `cargo clippy --fix` clean |
| 1 | `redundant_closure_for_method_calls` | pedantic | 17 | mostly `--fix` clean |
| 1 | `explicit_iter_loop` / misc style | pedantic | — | `--fix` clean |
| 2 | `doc_markdown` (missing backticks) | pedantic | 216 | `--fix` handles most; review prose |
| 2 | `missing_errors_doc` | pedantic | 39 | add `# Errors` sections by hand |
| 2 | `missing_panics_doc` | pedantic | 5 | add `# Panics` sections by hand |
| 3 | `use_self` | pedantic | 251 | `--fix` clean but large diff — own PR |
| 3 | `missing_const_for_fn` | nursery | 135 | `--fix` clean; verify no MSRV regressions |
| 4 | `type_complexity` | clippy::all | 14 | factor the `Arc<dyn Fn(...)>` node-closure types into `type` aliases — or keep permanently allowed, as boxed closures are intrinsic to the engine design |
| 4 | `trivially_copy_pass_by_ref` | pedantic | 24 | small API-shape changes |
| 4 | `needless_collect` | nursery | 26 | review each — some are intentional |
| 4 | `match_same_arms` | pedantic | 17 | review — some arms kept apart for clarity |
| 5 | `cast_lossless` | pedantic | 26 | `i32 -> f64` via `From` — `--fix` clean |
| 5 | `cast_precision_loss` | pedantic | ~120 | **judgement call** — a numeric engine casts `usize/i64 -> f64` deliberately; likely a permanent per-crate `allow` |
| 5 | `float_cmp` | pedantic | 69 | **judgement call** — exact `f64` compares are often intentional here; audit, then likely permanent `allow` |
| 5 | `suboptimal_flops` | nursery | 82 | **judgement call** — `mul_add` changes rounding; must NOT be applied where golden-test bit-exactness depends on it |

> Batches 1–4 are mechanical or near-mechanical. Batch 5 needs domain
> judgement: the float lints interact with the golden-suite bit-exactness
> guarantee (`tflo-fintech`), so `suboptimal_flops`/`float_cmp` fixes must be
> verified against the golden tests, and the `cast_*`/`float_cmp` lints may
> stay permanently `allow`ed with a documented rationale.

## Definition of done

The backlog is complete when `[workspace.lints.clippy]` no longer contains a
blanket `pedantic`/`nursery` suppression — either each lint has graduated to
its own `warn`/`deny` entry, or it carries a documented permanent `allow`.
