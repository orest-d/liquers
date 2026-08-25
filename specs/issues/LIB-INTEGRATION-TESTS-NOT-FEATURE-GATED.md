---
id: LIB-INTEGRATION-TESTS-NOT-FEATURE-GATED
kind: issue
title: liquers-lib integration tests do not compile without default features
status: draft
priority: P2
complexity: S
area: [lib/value, build]
design:
created: 2026-08-18
github:
---
## Problem

`liquers-lib`'s integration tests use optional dependencies unconditionally, so the crate's test
targets only compile with default features on:

- `tests/polars_commands.rs` and `tests/polars_value_serde.rs` — `use polars::prelude::*` with no
  `#[cfg(feature = "polars")]`;
- `tests/ui_shortcuts_integration.rs` and the other egui-facing suites — `egui::Key::S` and
  `KeyboardShortcut::to_egui` behind no gate.

```
$ cargo test -p liquers-lib --no-default-features --lib --tests
error[E0433]: failed to resolve: use of unresolved module or unlinked crate `polars`
$ cargo test -p liquers-lib --no-default-features --features polars --lib --tests
error[E0433]: failed to resolve: use of unresolved module or unlinked crate `egui`
```

The **library** is correctly gated and passes in every configuration — `--no-default-features
--lib` runs 215 tests green. Only the test targets are affected.

## Impact

The feature matrix cannot be exercised, which is exactly where `#[cfg]` mistakes hide: a cfg'd-out
enum variant leaving a `match` non-exhaustive compiles fine with default features and fails only in
the configuration nobody can run. A wasm or minimal build therefore has no test coverage at all,
and a regression in it is invisible until someone builds it by hand.

## Expected behaviour

Each integration test file that touches an optional dependency is gated — `#![cfg(feature = "…")]`
at the top of the file is the least intrusive form — so that `--no-default-features` and each
single-feature configuration compile and run whatever subset applies.

Worth doing alongside: a CI job, or a documented command list in `CLAUDE.md` beside the existing
build guidance, that runs the matrix rather than leaving it to be remembered.

## Discovery

Found on 2026-08-18 during `value-type-system` step 10, which called for running the feature
matrix. Verified pre-existing: at commit `dc762b3`, before any implementation in that project,
`tests/polars_commands.rs` contained zero `cfg(feature = "polars")` guards.
