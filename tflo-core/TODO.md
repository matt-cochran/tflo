# tflow Technical Debt

This document tracks allowed clippy pedantic lints that should be addressed in future refactoring efforts.

## Allowed Pedantic Lints

The following lints are currently allowed in the workspace `Cargo.toml`. Each represents an opportunity for code quality improvement.

### High Priority (Production Quality)

| Lint | Reason Allowed | Remediation |
|------|----------------|-------------|
| `cognitive_complexity` | Complex match statements in `compile.rs::eval_node` | Break down large match into smaller functions |
| `too_many_lines` | `eval_node` has 637+ lines | Split into node-type-specific evaluation functions |
| `type_complexity` | Closure types with `Arc<dyn Fn... + Send + Sync>` | Create type aliases for common closure patterns |

### Medium Priority (Code Clarity)

| Lint | Reason Allowed | Remediation |
|------|----------------|-------------|
| `missing_const_for_fn` | Many functions could be `const` | Audit and add `const` where appropriate |
| `cast_precision_loss` | `i64`/`usize` to `f64` casts | Document where precision loss is acceptable |
| `cast_possible_truncation` | Numeric computation casts | Add explicit bounds checking where needed |
| `significant_drop_tightening` | Mutex guard drop timing | Review guard lifetimes for optimization |
| `manual_let_else` | `if let Some` patterns | Convert to `let ... else` where clearer |

### Low Priority (Style Preferences)

| Lint | Reason Allowed | Remediation |
|------|----------------|-------------|
| `module_name_repetitions` | Style preference | Review for cleaner naming |
| `must_use_candidate` | Style preference | Add `#[must_use]` where appropriate |
| `map_unwrap_or` | `map().unwrap_or()` patterns | Convert to `map_or()` |
| `unnecessary_map_or` | `map_or(true/false, ...)` patterns | Use `is_none_or`/`is_some_and` |
| `suboptimal_flops` | `a + b * c` patterns | Consider `mul_add()` for hot paths |
| `doc_markdown` | Math formulas in docs | Add backticks to code in docs |
| `option_if_let_else` | `if let Some` vs `map_or` | Evaluate case-by-case |
| `match_same_arms` | Explicit matching | Consolidate where appropriate |
| `redundant_closure_for_method_calls` | `\|a, b\| a.cmp(b)` | Use method reference syntax |
| `if_not_else` | `if !condition` patterns | Invert where clearer |
| `let_and_return` | `let result = x; result` | Return directly where clearer |
| `needless_continue` | Explicit `continue` | Remove redundant continues |
| `many_single_char_names` | Math variable names | Use descriptive names where possible |
| `unnested_or_patterns` | `A \| B \| C` patterns | Use nested patterns |
| `use_self` | `TypeName` vs `Self` | Use `Self` consistently |
| `derive_partial_eq_without_eq` | `PartialEq` without `Eq` | Add `Eq` where appropriate |
| `missing_fields_in_debug` | Partial Debug impls | Include all fields or use `finish_non_exhaustive()` |
| `single_match` / `single_match_else` | Match vs if let | Use if let where appropriate |
| `elidable_lifetime_names` | Explicit lifetimes | Elide where possible |
| `wrong_self_convention` | `into_*` with `&self` | Review naming conventions |
| `implicit_hasher` | HashMap default hasher | Generalize over `BuildHasher` |
| `arc_with_non_send_sync` | Arc with non-Send types | Use Rc or add Send+Sync |
| `needless_borrow` | Unnecessary borrows | Remove redundant `&` |
| `trivially_copy_pass_by_ref` | `&NodeId` parameters | Pass by value for Copy types |
| `coerce_container_to_any` | `Box<dyn Any>` coercion | Use explicit dereference |

## Refactoring Roadmap

### Phase 1: Critical Path Optimization
1. **Split `eval_node`** into node-type-specific functions
   - Create `eval_primitive_node()`, `eval_aggregate_node()`, `eval_trigger_node()`, etc.
   - This addresses `cognitive_complexity` and `too_many_lines`

2. **Create type aliases** for common closure patterns
   ```rust
   type MapperFn = Arc<dyn Fn(&ValueStore) -> Option<Box<dyn Any + Send + Sync>> + Send + Sync>;
   type FolderFn = Arc<dyn Fn(&ValueStore, &Mutex<Box<dyn Any + Send + Sync>>) -> Option<Box<dyn Any + Send + Sync>> + Send + Sync>;
   ```

### Phase 2: Numeric Safety Audit
1. Document all intentional precision loss locations
2. Add bounds checking where truncation could cause issues
3. Consider using `checked_*` operations for critical paths

### Phase 3: Style Consistency
1. Run `cargo clippy --fix` for auto-fixable lints
2. Review remaining style lints case-by-case
3. Update code style guide with decisions

## Benchmarks to Monitor

After each refactoring phase, run benchmarks to ensure no performance regressions:

```bash
cd tflow
cargo bench
```

Key metrics to track:
- `step` throughput (items/sec)
- `compile` latency
- Memory allocation per step
- Cache hit rates for composed graphs

## Notes

- The `cargo` lint group is disabled because these are internal crates that don't need full crates.io metadata
- `disallowed_methods` from `clippy.toml` bans `.unwrap()` and `.expect()` - exceptions require explicit `#[allow]` with justification

