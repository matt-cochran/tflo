<!-- Thanks for contributing to tflo! Please fill in the sections below. -->

## Summary

<!-- One or two sentences describing what this PR does and why. -->

## Scope and non-goals check

<!--
Before opening, please skim docs/non-goals.md. It records what tflo
deliberately does NOT do (NFA pattern matching in core, streaming SQL,
distributed runtime, exactly-once via 2PC, etc.).

- If this PR does NOT touch a non-goal, write "n/a" below.
- If it DOES, briefly explain why the change is compatible with the
  stated reasoning, or argue why the reasoning should change.
-->

n/a

## Type of change

<!-- Check all that apply. -->

- [ ] Bug fix
- [ ] New feature (additive, non-breaking)
- [ ] Breaking change (requires a major version bump)
- [ ] Refactor (no behavioural change)
- [ ] Documentation
- [ ] Tests / benchmarks / CI

## Verification

<!-- Replace the placeholders with the commands you actually ran. -->

- [ ] `cargo fmt --workspace --check`
- [ ] `cargo clippy --workspace --all-features --all-targets -- -D warnings`
- [ ] `cargo test --workspace`
- [ ] `cargo build --target wasm32-unknown-unknown -p tflo-core -p tflo-ops`
- [ ] `cargo build --no-default-features -p tflo-core`
- [ ] `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps` (warnings = errors)
- [ ] For performance-sensitive changes: ran relevant `cargo bench`
- [ ] For changes to `tflo-fintech`: golden tests in
      `tflo-fintech/tests/golden` still pass

## Related issues / discussions

<!-- Links to issues, discussions, or design docs (interop-backlog,
non-goals, semantics, deployment-shapes). -->

## Notes for reviewers

<!-- Anything reviewers should pay particular attention to. -->
