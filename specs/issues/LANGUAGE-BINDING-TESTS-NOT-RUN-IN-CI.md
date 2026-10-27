---
id: LANGUAGE-BINDING-TESTS-NOT-RUN-IN-CI
kind: issue
title: CI never runs liquers-py's Python tests or liquers-web's Node conformance suites
status: draft
priority: P2
complexity: M
area: [build, py, web]
design: language-binding-tests-in-ci
created: 2026-10-26
github:
---

## Problem

The only CI that builds Rust is `.github/workflows/build-matrix.yml`, which runs
`scripts/check-build-matrix.sh` — `cargo check` over feature and target rows. Nothing runs:

- `bash scripts/test-liquers-py.sh` (maturin develop, pytest over `liquers-py/tests/`, then the
  crate's Rust unit tests), which is where the Python conformance evidence in
  `liquers-py/README.md` comes from;
- `cargo test -p liquers-web --target wasm32-unknown-unknown --features debug-handles` (with and
  without `records`), the Node loop behind `liquers-web/README.md`'s conformance table.

Both bindings sit downstream of core changes that alter their behaviour without touching their
files (an `ErrorType` variant, an asset-lifecycle change, a value-type change), and the
`build-matrix` row only proves they still compile.

## Problem Example

A core PR adds an `ErrorType` variant and an `ErrorCategory`. `build-matrix` passes, because
`liquers-py` and `liquers-web` compile. `test_ERROR_*` (Python) and `OBJECT06` (web, an exhaustive
error-type round trip) would fail, but nothing runs them until someone works on a binding locally.

## Impact

Conformance tables marked `COMPLETE` / `CONFORMANT` can silently stop being true after an
unrelated merge. Workaround: run both loops by hand before merging any core change that touches
errors, values, assets or command registration.

## Expected behaviour

A workflow (or two jobs) that runs `scripts/test-liquers-py.sh` on one CPython version and the
`liquers-web` Node loop (both feature sets) on pull requests touching `liquers-core`,
`liquers-lib`, `liquers-py` or `liquers-web`, after `cargo clean` or in a separate job, given the
disk notes in `CLAUDE.md`. The browser-only and Playwright suites may stay manual.

## Discovery

Recorded on 2026-10-26 while writing the project-groups plan for the bulk-design pass; the
2026-10-20 backlog plan (§10.1) had noted "No CI job runs the py test loop" without filing it.

Designed 2026-10-27 in `design/language-binding-tests-in-ci/` (bulk design), together with
`PY-TESTS-NOT-RUN-IN-CI` (overlap tests T4 and T3; this issue leads). Complexity raised from `S` to
`M`: the fix is two workflow files, one per binding so that each has its own path filter, and one line of `scripts/test-liquers-py.sh` (polars has no free-threaded wheel).
