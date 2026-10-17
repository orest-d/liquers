---
id: PY-PYO3-0-21-CANNOT-TARGET-PYTHON-3-13
kind: feature
title: liquers-py pins PyO3 0.21, which cannot target Python 3.13 or newer
status: closed
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

## Resolution (2026-10-17)

`liquers-py/Cargo.toml` now depends on `pyo3 = "0.30.1"`, the newest release on crates.io (its MSRV,
1.85, is below the workspace's 1.95). `liquers-py/src/` moved off the GIL-ref API: the module
initializer takes `&Bound<'_, PyModule>`, `Python::with_gil` became `Python::attach`, `PyObject`
became `Py<PyAny>`, `to_object` / `into_py` became `IntoPyObjectExt::into_py_any`, and the getters
that returned `&'py PyDict` return `PyResult<Bound<'py, PyDict>>`. Three changes in PyO3's defaults
needed explicit opt-ins to keep the Python-visible behaviour:

- `#[pyclass]` types that are `Clone` no longer implement `FromPyObject` by default (0.30); each
  such class now says `#[pyclass(from_py_object)]`, so it can still be passed by value.
- Simple enums no longer compare equal by discriminant automatically; `EnumArgumentType`,
  `ErrorType` and `LogEntryKind` say `#[pyclass(eq, eq_int)]`.
- `Py<T>` is not `Clone` without the `py-clone` feature, so the bindings' `Value` implements
  `Clone` by hand, attaching to the interpreter for its `Py` variant.

One deliberate behaviour change: `Value::Bytes` now converts to Python `bytes` (PyO3 0.23 changed
`Vec<u8>`'s conversion), where it used to become a `list` of integers.

`pyproject.toml` declares `requires-python = ">=3.9,<3.16"` (PyO3 0.30 supports CPython 3.9–3.15
and PyPy 3.11+); the `PYO3_USE_ABI3_FORWARD_COMPATIBILITY` advice is gone from
`liquers-py/README.md` and `CLAUDE.md`.

Evidence, on CPython 3.13.16 with no override: `cargo check -p liquers-py --lib --tests` builds
with zero warnings; `cargo test -p liquers-py --no-default-features --lib` passes (7 tests);
`bash scripts/test-liquers-py.sh` builds the wheel with `maturin develop` and passes all 29
pytest tests in `liquers-py/tests/`.
