---
id: PY-HAS-NO-PURE-PYTHON-FALLBACK
kind: feature
title: liquers has no pure-Python fallback when the native extension is unavailable
status: draft
priority: P3
complexity: XL
area: [py]
design: 
created: 2026-11-14
github:
---
## Problem

`import liquers` fails when the native extension cannot be loaded: on a platform with no wheel and
no Rust toolchain for the sdist, or under an interpreter PyO3 does not support. Every public name
is defined in Rust (`liquers-py/src/lib.rs`); there is no Python implementation of any of them.

## Impact

Such a user cannot use Liquers at all, not even to parse or build queries. Workaround: build from
the sdist with a Rust toolchain. No current user is blocked (no wheels are published yet,
`PY-NO-PUBLISHED-WHEELS`).

## Expected behaviour

When the native module `_liquers` is unavailable, the public `liquers` submodules import a
pure-Python implementation of a defined subset of the same names, with the same behaviour, and
say which names are unavailable. The subset needs its own decision: query parsing and encoding,
keys and command metadata are feasible; environments, stores and assets are a reimplementation of
`liquers-core`. The conformance suite would run against both backends for the subset.

## Discovery

Raised by Orest on 2026-11-14 at the Phase 1 gate of `py-package-submodules` ("In the future I'd
like to have a pure python fallback if _liquers is not available"). That design keeps one import
seam so the fallback can be added later without changing the public modules.
