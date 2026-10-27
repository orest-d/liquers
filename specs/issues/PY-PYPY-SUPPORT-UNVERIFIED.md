---
id: PY-PYPY-SUPPORT-UNVERIFIED
kind: issue
title: liquers-py claims PyPy 3.11 support that has never been built or tested
status: draft
priority: P3
complexity: S
area: [py, build]
design: py-pypy-ci-job
created: 2026-10-27
github:
---
## Problem

`liquers-py/README.md` "Supported Python", `specs/guides/PYTHON_GUIDE.md` §1, `CLAUDE.md` and
`liquers-py/pyproject.toml` (a `Programming Language :: Python :: Implementation :: PyPy`
classifier) all say PyPy 3.11 or newer is supported. The claim rests on PyO3 0.30's support list
alone: no record shows the package built or its suite run on PyPy, `scripts/test-liquers-py.sh`
has only been run on CPython 3.13 and 3.14t, and `design/language-binding-tests-in-ci/` leaves
PyPy out of its CI matrix ("supported by PyO3 but never yet verified here").

What is known about the test loop on PyPy 3.11:

- `pyo3-ffi` 0.30.1 supports PyPy 3.11 or newer (verified).
- polars cannot be installed binary-only there (`polars-runtime-32` has no compatible wheel), and
  `scripts/test-liquers-py.sh` installs it unconditionally, so pip would build it from source;
  pillow has a `pp311` wheel (verified).
- The Rust unit tests (`cargo test -p liquers-py --no-default-features --lib`) call
  `Python::initialize()` in `liquers-py/src/value.rs`, `environment.rs`, `error.rs` and `aio.rs`.
  PyO3 0.30.1 declares it `#[cfg(not(any(PyPy, GraalPy)))]` (`src/marker.rs`), so the test target
  does not compile on PyPy (verified from the source).
- Unverified: whether `actions/setup-python` resolves `pypy3.11`, whether `liquers-py` compiles
  and imports on PyPy, and how GC-timing tests such as `RUNTIME05` behave there.

## Problem Example

```bash
PYTHON=pypy3.11 bash scripts/test-liquers-py.sh
# today:    never run; expected to stall at `pip install polars`, and the Rust half does not compile
# expected: the package builds with maturin, the Python suite passes (polars tests skipped),
#           and the check runs in CI so the claim stays true
```

## Impact

A PyPy user relies on a support statement nobody has checked. Tests that count released handles
(`RUNTIME05`) or assume prompt finalization may behave differently under PyPy's garbage collector,
so the suite may fail there even if the package works. Workaround: none needed for CPython users.

## Expected behaviour

The PyPy claim is backed by a CI job that runs the Python suite on PyPy 3.11, or, if that job
shows real failures, the claim is narrowed until they are fixed.

## Discovery

Found during the 2026-10-27 bulk design of the Python backlog, while reviewing
`design/language-binding-tests-in-ci/`'s non-goals against `CLAUDE.md`'s supported-versions line.
Compare `PY-FREE-THREADED-SUPPORT-UNVERIFIED` (closed), the same kind of gap for 3.14t.
