---
id: PY-ABI3T-WHEEL-NOT-USED
kind: issue
title: liquers-py builds a cp315t wheel because the abi3t wheel does not import
status: draft
priority: P3
complexity: S
area: [py, build]
design: 
created: 2026-11-01
github:
---
## Problem

PEP 803's stable ABI for free-threaded builds (`abi3t`) would let one free-threaded wheel serve
CPython 3.15t and every later free-threaded release, as the `abi3` wheel does for GIL builds.
liquers-py instead builds a version-specific `cp315t` wheel, and must add a `cp316t` wheel for
each new release.

Measured on 2026-11-01 (CPython 3.15.0b4t, maturin 1.16, PyO3 0.30.1, `pyo3/abi3t-py315`): the
build succeeds and is tagged `cp315-abi3.abi3t`, but the library is named
`_native.abi3t-x86_64-linux-gnu.so`, while the interpreter's `EXTENSION_SUFFIXES` accept only
`.abi3t.so`, so `import liquers` fails with `No module named 'liquers._native'`. An installer must
also know the `abi3t` tag.

## Expected behaviour

The release builds one `abi3t` wheel in place of `cp315t` once maturin names the library as
CPython expects and current pip installs the tag; checked on a final 3.15 release.

## Discovery

`liquers-py-package` Phase 2 (the ABI question).
