---
id: PY-FREE-THREADED-PYTHON-NOT-SUPPORTED
kind: issue
title: liquers-py does not declare support for free-threaded Python, so its import may re-enable the GIL
status: draft
priority: P3
complexity: M
area: [py]
design:
created: 2026-10-26
github:
---
## Problem

`liquers-py` makes no statement about free-threaded CPython (3.13t, 3.14t; PEP 703, PEP 779).
Its `#[pymodule]` does not set `gil_used = false`, and `README.md`, `pyproject.toml` and
`specs/reference/PYTHON_INTEGRATION.md` say nothing about the free-threaded builds. A module that
does not declare free-threaded support makes the interpreter re-enable the GIL when it is imported
(PyO3's default; not yet checked on a 3.14t build of this crate). Python commands then serialize
on the GIL even on a free-threaded interpreter.

## Impact

Sync Python commands already run each on their own blocking thread (`command.rs`,
`call_py_command`), so on a free-threaded interpreter they could run in parallel with no other
change. Without the declaration, that parallelism is lost. Declaring it is not free: every
`Py<PyAny>` held across threads and every shared Rust structure reached from Python has to be
checked for data races that the GIL used to prevent.

## Expected behaviour

Either liquers-py is built and tested on a free-threaded interpreter and declares
`gil_used = false`, with the result recorded in the conformance matrix, or the README states that
free-threaded builds are not supported and run with the GIL.

## Discovery

Raised by Orest (2026-10-26) while discussing where `async def` commands run in
`design/liquers-py-async/` (Phase 1, Decision 2).
