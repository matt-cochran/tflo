# Contributing to tflo

First off, thank you for considering contributing to tflo! We welcome contributions
of all kinds — bug reports, feature requests, documentation improvements, and code
changes.

## Code of Conduct

This project and everyone participating in it is governed by our
[Code of Conduct](CODE_OF_CONDUCT.md). By participating, you are expected to
uphold this code. Please report unacceptable behaviour to
`security@tflo.dev`.

## Table of Contents

- [Setting Up Locally](#setting-up-locally)
- [Running Benchmarks](#running-benchmarks)
- [How to Contribute](#how-to-contribute)
- [Code Style](#code-style)
- [Pull Request Checklist](#pull-request-checklist)
- [Getting Help](#getting-help)

---

## Setting Up Locally

1. **Clone the repository:**

   ```sh
   git clone https://github.com/matt-cochran/tflo.git
   cd tflo
   ```

2. **Run the full test suite across all crates:**

   ```sh
   cargo test --workspace
   ```

3. **Verify the code compiles with all features enabled:**

   ```sh
   cargo check --workspace --all-features
   ```

   > **Note:** tflo targets Rust edition 2024 and requires a recent Rust toolchain.
   > The minimum supported Rust version (MSRV) is specified in
   > [`rust-toolchain.toml`](rust-toolchain.toml).

4. **Optionally, build the full workspace in release mode:**

   ```sh
   cargo build --workspace --release
   ```

## Running Benchmarks

We use [`criterion`](https://docs.rs/criterion) for benchmarking.

```sh
cargo bench
```

To run benchmarks for a specific crate:

```sh
cargo bench -p tflo-core
cargo bench -p tflo-ta
```

Benchmark results are written to `target/criterion/`. You can open the HTML report
in your browser:

```sh
open target/criterion/report/index.html
```

## How to Contribute

1. **Fork** the repository on GitHub.
2. **Create a feature branch** from `main`:
   ```sh
   git checkout -b feat/my-feature
   ```
   Use a descriptive branch name prefixed with `feat/`, `fix/`, `docs/`, `chore/`,
   or `refactor/`.
3. **Make your changes**, following the [code style](#code-style) guidelines below.
4. **Write or update tests** as appropriate. We use `proptest` for property-based
   testing and golden vector tests in `tflo-ta-golden` for regression coverage.
5. **Ensure the full test suite passes**:
   ```sh
   cargo test --workspace
   ```
6. **Run clippy** and fix any warnings:
   ```sh
   cargo clippy --workspace --all-features -- -D warnings
   ```
7. **Commit your changes** with a clear and concise commit message. We follow
   [Conventional Commits](https://www.conventionalcommits.org/):
   ```
   feat(core): add sliding window median
   fix(ta): correct RSI initialisation for edge case
   docs: update README with new signal primitives
   ```
8. **Push your branch** and open a Pull Request against `main`.
9. **Ensure CI passes** on your PR before requesting a review.

## Code Style

- **Formatting:** All code must be formatted with `rustfmt`. Run the following
  before committing:
  ```sh
  cargo fmt --workspace
  ```
- **Clippy:** We enforce the `clippy::pedantic` and `clippy::nursery` lints at the
  workspace level. Run clippy with:
  ```sh
  cargo clippy --workspace --all-features
  ```
- **No `unsafe` code:** The workspace root sets `unsafe_code = "forbid"` in
  `[workspace.lints.rust]`. Crates must not use `unsafe` code. If you believe
  `unsafe` is necessary, please open an issue first to discuss it.
- **Documentation:** Public items must have doc comments. Use `//!` for crate-level
  docs and `///` for items. Include an example where appropriate.
- **Naming:** Follow Rust naming conventions: `snake_case` for functions and
  variables, `UpperCamelCase` for types, `SCREAMING_SNAKE_CASE` for constants.
- **Imports:** Group and order imports as:
  1. Standard library (`std::*`)
  2. External crates
  3. `crate::*` and `super::*`

## Pull Request Checklist

Before submitting your PR, ensure:

- [ ] Code is formatted with `cargo fmt --workspace`
- [ ] Clippy passes with `cargo clippy --workspace --all-features -- -D warnings`
- [ ] Full test suite passes: `cargo test --workspace`
- [ ] All benchmarks run without regressions (if performance-sensitive)
- [ ] New public items have doc comments
- [ ] Changes are covered by tests (unit, integration, or property-based)
- [ ] Commit messages follow [Conventional Commits](https://www.conventionalcommits.org/)
- [ ] PR title follows the same convention (e.g. `feat(core): ...`)
- [ ] You have read the [Code of Conduct](CODE_OF_CONDUCT.md)

## Getting Help

- Open a [Discussion](https://github.com/matt-cochran/tflo/discussions)
  for questions and ideas.
- File an [Issue](https://github.com/matt-cochran/tflo/issues) for bugs
  and feature requests.
- For security vulnerabilities, see [`SECURITY.md`](SECURITY.md).
