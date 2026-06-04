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

Batches 1–3 are **complete** as of the hardening-pass branch; each lint is
now denied in `[workspace.lints.clippy]`. The remaining batches (4–5) are
deferred until after the Phase 1 breaking-change release.

| Batch | Lint | Group | ~Count | Notes |
|------:|------|-------|-------:|-------|
| 1 ✅ | `uninlined_format_args` | pedantic | 22 | `cargo clippy --fix` clean → `deny` |
| 1 ✅ | `redundant_closure_for_method_calls` | pedantic | 17 | mostly `--fix` clean → `deny` |
| 1 ✅ | `explicit_iter_loop` / misc style | pedantic | — | `--fix` clean → `deny` |
| 2 ✅ | `doc_markdown` (missing backticks) | pedantic | 216 | `--fix` + prose review → `deny` |
| 2 ✅ | `missing_errors_doc` | pedantic | 39 | hand-written `# Errors` sections → `deny` |
| 2 ✅ | `missing_panics_doc` | pedantic | 5 | absorbed by `--fix` → `deny` |
| 3 ✅ | `use_self` | pedantic | 251 | `--fix` clean → `deny`; `#[wasm_bindgen]` impls in `tflo-wasm/src/lib.rs` carry a module-level `allow` because the macro expansion needs the explicit struct name |
| 3 ✅ | `missing_const_for_fn` | nursery | 135 | `--fix` + three manual const-fns → `deny`; same `tflo-wasm` `allow` for `#[wasm_bindgen(constructor)]` impls |
| 4 ✅ | `type_complexity` | clippy::all | 14 | Permanent `allow` — boxed closures are intrinsic to the engine design. Rationale comment in `Cargo.toml`. |
| 4 ✅ | `trivially_copy_pass_by_ref` | pedantic | 24 | Permanent `allow` (Phase 5 decision). Case-by-case API-shape cleanup deferred. |
| 4 ✅ | `needless_collect` | nursery | 26 | Permanent `allow`. Many are intentional in tests / combinators. |
| 4 ✅ | `match_same_arms` | pedantic | 17 | Permanent `allow`. Some arms kept apart for clarity. |
| 5 ✅ | `cast_lossless` | pedantic | 26 | Permanent `allow` — engine casts integer counts into `f64` deliberately. |
| 5 ✅ | `cast_precision_loss` | pedantic | ~120 | Permanent `allow` — the engine's whole point is to compute in `f64` against `usize`/`i64` inputs. |
| 5 ✅ | `cast_possible_truncation` / `cast_possible_wrap` / `cast_sign_loss` | pedantic | misc | Permanent `allow` for the same reason. |
| 5 ✅ | `float_cmp` | pedantic | 69 | Permanent `allow` — detector ops compare against caller-supplied thresholds with `==`/`>`/`<` as documented contract. |
| 5 ✅ | `suboptimal_flops` | nursery | 82 | Permanent `allow` — `mul_add` rewriting would break the `tflo-fintech` golden-fixture bit-equality test. The golden suite is the actual safety net. |

> All batches resolved as of Phase 5 of the production roadmap. The
> Phase 5 decision was to **annotate** rather than rewrite for the
> numeric / nursery lints: they fire intentionally in a streaming
> numeric engine, and the `tflo-fintech` golden-fixture suite is what
> guards against numeric drift.

## Definition of done

✅ **Complete.** `[workspace.lints.clippy]` no longer blanket-suppresses
`pedantic`/`nursery`. Every lint that was in the original backlog has
either graduated to a per-lint `warn`/`deny` entry (batches 1–3) or
carries a documented permanent `allow` with rationale (batches 4–5).
The full workspace passes
`cargo clippy --workspace --all-targets -- -D warnings` clean.
