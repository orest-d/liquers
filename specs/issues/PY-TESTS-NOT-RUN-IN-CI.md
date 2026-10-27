---
id: PY-TESTS-NOT-RUN-IN-CI
kind: issue
title: No CI workflow runs the liquers-py test suite, on any Python interpreter
status: draft
priority: P2
complexity: S
area: [py, build]
design:
created: 2026-10-26
github:
---
## Problem

`.github/workflows/` has `build-matrix.yml` and `docs-check.yml`. Neither runs
`scripts/test-liquers-py.sh` or `pytest liquers-py/tests`, so the Python suite (175 tests) and the
crate's Rust unit tests run only when someone runs them by hand. A change in `liquers-core` that
breaks the Python integration, or a regression on one interpreter (3.9, 3.13, free-threaded 3.14t,
PyPy), is not caught before merge.

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
