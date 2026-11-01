---
id: PY-NO-PUBLISHED-WHEELS
kind: feature
title: liquers-py has no published wheels, so installing it needs a Rust toolchain and a long build
status: draft
priority: P2
complexity: L
area: [py, build]
design: liquers-py-package
created: 2026-11-01
github:
---
## Problem

liquers-py is installed only from source: `maturin develop` or `pip install ./liquers-py`, which
needs Rust 1.95 or newer and builds `liquers-lib` with polars, a cold build of several minutes and
several gigabytes. No workflow builds wheels (`.github/workflows/` has the test loops only), the
distribution `liquers-py` is not on PyPI (checked 2026-11-01: `https://pypi.org/pypi/liquers-py/json`
answers 404), and nothing tests installing the package into a clean environment.
`PYTHON_INTEGRATION.md` §8 lists "no published wheel" as a limitation.

## Impact

A Python user who does not build Rust cannot use liquers at all, and nothing checks that a built
package installs, carries its metadata, or runs the README quick-start. This is the guide's
`PACKAGE` feature (`LANGUAGE-INTEGRATION_GUIDE.md` §PACKAGE), selected for M5.

## Expected behaviour

Wheels for the claimed platforms are built by GitHub Actions with `maturin`, installed into a
clean environment and exercised (`PACKAGE01`-`PACKAGE07`), with the version locked to the core and
optional extras for polars, Pillow and pyarrow. Design: `specs/design/liquers-py-package/`.

## Discovery

Filed 2026-11-01 when the M5 design folders were created. Weak overlap: `PY-PYPY-SUPPORT-UNVERIFIED`
(PyPy is a claimed interpreter; whether wheels are built for it is a question of this design).
