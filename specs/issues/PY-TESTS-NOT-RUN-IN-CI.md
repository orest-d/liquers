---
id: PY-TESTS-NOT-RUN-IN-CI
kind: issue
title: No CI workflow runs the liquers-py test suite, on any Python interpreter
status: closed
priority: P2
complexity: M
area: [py, build]
design: language-binding-tests-in-ci
created: 2026-10-26
github:
---
## Resolution

Fixed by `specs/design/language-binding-tests-in-ci/` (PR #226).
`.github/workflows/liquers-py-tests.yml` runs `scripts/test-liquers-py.sh` on CPython 3.10, 3.14 and
3.14t. `.github/workflows/liquers-web-tests.yml` runs the liquers-web Node conformance suites with
`debug-handles` and with `debug-handles,records`. Evidence: all four jobs green on the PR. A
temporary commit that broke one assertion in `test_ERROR_errors.py` and one in
`liquers-web/tests/objects_OBJECT.rs` turned all four jobs `failure` (none `cancelled`), and was
reverted. The first CI runs on 3.10 found two defects, both fixed in the same PR: tracebacks with
no frames, and an abort at interpreter exit. PyPy in CI remains open as `py-pypy-ci-job`.

## Problem

`.github/workflows/` has `build-matrix.yml` and `docs-check.yml`. Neither runs
`scripts/test-liquers-py.sh` or `pytest liquers-py/tests`, so the Python suite (175 tests) and the
crate's Rust unit tests run only when someone runs them by hand. A change in `liquers-core` that
breaks the Python integration, or a regression on one interpreter (3.9, 3.13, free-threaded 3.14t,
PyPy), is not caught before merge.

## Problem Example

A PR changes `liquers-core`'s error mapping so that `ErrorType` gains a variant. `build-matrix`
passes, because `liquers-py` compiles, and the PR merges. `test_ERROR01_every_error_type_maps` would
have failed on every interpreter, and a free-threaded-only race in `test_FREETHREAD_concurrency.py`
would have failed on 3.14t only, but neither runs until someone runs
`PYTHON=… bash scripts/test-liquers-py.sh` by hand.

## Impact

Every claim in `liquers-py/README.md`'s conformance matrix, and the free-threaded support verified by
`design/liquers-py-async/` AC-8, holds only as of the last manual run.

## Expected behaviour

A workflow, triggered on changes under `liquers-py/**` and the crates it depends on, runs
`scripts/test-liquers-py.sh` on at least the oldest and newest supported CPython and on a
free-threaded build.

## Discovery

Found during the `design/liquers-py-async/` Phase 2 preflight (2026-10-26), while checking how the
free-threaded audit (AC-8) would be proved.

Designed 2026-10-27 in `design/language-binding-tests-in-ci/` (bulk design), merged into the design
led by `LANGUAGE-BINDING-TESTS-NOT-RUN-IN-CI`. Overlap tests that held: **T4** (that issue's Python
half is this issue on one interpreter; this issue adds the interpreter matrix) and **T3** (the same
new workflow). Runner-up: none. Complexity raised from `S` to `M` with the merged scope.
