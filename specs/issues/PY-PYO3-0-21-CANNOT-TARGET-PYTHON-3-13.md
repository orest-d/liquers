---
id: PY-PYO3-0-21-CANNOT-TARGET-PYTHON-3-13
kind: feature
title: liquers-py pins PyO3 0.21, which cannot target Python 3.13 or newer
status: draft
priority: P3
complexity: M
area: [py, build]
design:
created: 2026-10-10
github:
---

## Problem

`liquers-py/Cargo.toml` pins `pyo3 = "0.21.2"`, whose maximum supported Python is 3.12.
`liquers-py/pyproject.toml` now says so (`requires-python = ">=3.8,<3.13"`), and building against
3.13 needs `PYO3_USE_ABI3_FORWARD_COMPATIBILITY=1` (`liquers-py/README.md`).

## Impact

Python 3.13 (the cloud dev environment's `python3`, and current CPython) is not a supported target;
the override builds against the stable ABI without support from PyO3.

## Expected behaviour

Raise `pyo3` to the oldest release that supports the current CPython (0.22 or later), migrate
`liquers-py/src/` from the GIL-ref API (`&PyAny`) to `Bound<'py, PyAny>`, re-run `liquers-py/tests/`,
then lift the `requires-python` upper bound and remove the override from `README.md` and
`CLAUDE.md`.

## Discovery

Option A of `design/pyo3-python-3-13-support/`, deferred by maintainer decision (2026-10-08,
backlog compaction D4) to its own `M` feature, filed when that design (option B) was implemented on
2026-10-10.
