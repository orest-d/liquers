---
id: PY-FREE-THREADED-SUPPORT-UNVERIFIED
kind: issue
title: liquers-py declares free-threaded support by PyO3 default without being audited or tested on it
status: draft
priority: P2
complexity: M
area: [py]
design: liquers-py-async
created: 2026-10-26
github:
---
## Problem

`liquers-py` builds on PyO3 0.30.1, and since PyO3 0.28 a `#[pymodule]` without a `gil_used`
option declares that it does not need the GIL (`pyo3-macros-backend` `module.rs`:
`options.gil_used.is_some_and(...)`, so the default is `false`). The `_native` module in
`liquers-py/src/lib.rs` has no option, so on a free-threaded CPython (3.13t, 3.14t; PEP 703,
PEP 779) importing it keeps the GIL disabled. That claim has never been checked: no build or test
runs on a free-threaded interpreter, and `README.md`, `pyproject.toml` and
`specs/reference/PYTHON_INTEGRATION.md` say nothing about it.

## Impact

Sync Python commands each run on their own blocking thread (`command.rs`, `call_py_command`), so on
a free-threaded interpreter they already run in parallel. Any data race the GIL used to prevent,
in a `Py<PyAny>` held across threads or a shared Rust structure reached from Python, is live there
today.

## Expected behaviour

Either liquers-py is audited, built and tested on a free-threaded interpreter, with the result
recorded in the conformance matrix, or the module sets `#[pymodule(gil_used = true)]` until it is,
and the README says so.

## Discovery

Raised by Orest (2026-10-26) while discussing where `async def` commands run in
`design/liquers-py-async/` (Phase 1, Decision 2). Reframed after review of PR #181 showed PyO3
0.28+ already declares the support. Moved into `liquers-py-async` (AC-8) by Orest on
2026-10-26.
