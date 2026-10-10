---
id: PY-PYO3-REJECTS-PYTHON-3-13
kind: issue
title: liquers-py does not build against Python 3.13 without an environment override
status: closed
priority: P3
complexity: S
area: [py, build]
design: pyo3-python-3-13-support
created: 2026-10-06
github:
---
## Problem

`liquers-py/Cargo.toml` pins `pyo3 = "0.21.2"`. PyO3 0.21 supports Python only up to 3.12, so on a
host whose `python3` is 3.13 (the cloud dev environment, 2026-10-06) `cargo check -p liquers-py
--lib` fails in `pyo3-ffi`'s build script. It builds with `PYO3_USE_ABI3_FORWARD_COMPATIBILITY=1`,
which relies on the stable ABI rather than real 3.13 support. Nothing in the repository mentions
the variable.

## Impact

Anyone checking or building `liquers-py` on a current Python hits a build-script failure with no
pointer to the workaround. Nothing in the default test loop builds `liquers-py`, so it goes
unnoticed.

## Expected behaviour

`liquers-py` builds against the supported current Python without an override: upgrade `pyo3` to a
release that supports 3.13 (an API migration — the `Bound` API from 0.21 onward), or document the
supported Python range and the override where `liquers-py` is built.

## Discovery

`cargo check -p liquers-py --lib` while validating `design/core-dead-code-hygiene/`, 2026-10-06.

## Resolution (2026-10-10)

Option B (maintainer decision, 2026-10-08): the supported range is written down.
`liquers-py/pyproject.toml` declares `requires-python = ">=3.8,<3.13"`; the new
`liquers-py/README.md` states the range, the build error and the
`PYO3_USE_ABI3_FORWARD_COMPATIBILITY=1` override; `CLAUDE.md` points to it. Checked on Python
3.13.16: `cargo check -p liquers-py --lib` fails with the documented error, and succeeds with the
override. The PyO3 upgrade (option A) is filed as `PY-PYO3-0-21-CANNOT-TARGET-PYTHON-3-13`.
Design: `design/pyo3-python-3-13-support/`.
