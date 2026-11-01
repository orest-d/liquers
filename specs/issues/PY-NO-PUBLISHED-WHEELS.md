---
id: PY-NO-PUBLISHED-WHEELS
kind: feature
title: liquers-py has no published wheels, so installing it needs a Rust toolchain and a long build
status: in_progress
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

## Progress

2026-11-01: `design/liquers-py-package/` (PR #240) built the release pipeline
(`.github/workflows/liquers-py-release.yml`): fifteen wheels and an sdist, each wheel tested in a
fresh venv and checked for linked libraries, then TestPyPI, then PyPI behind the protected `pypi`
environment. Nothing is published yet. This closes when the first release (`liquers-py-v0.1.0`) is
on PyPI, which needs Orest: the trusted publishers registered on TestPyPI and PyPI, the tag, and
his approval of the `pypi` job. Contract: `reference/PYTHON_INTEGRATION.md` §12.
