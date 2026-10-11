---
id: STORE-CONFORMANCE-FEATURE-BUILD-WARNINGS
kind: issue
title: The store-conformance feature build emits unused-import and dead-code warnings
status: closed
priority: P3
complexity: S
area: [core/store, build]
design: store-conformance-warning-cleanup
created: 2026-10-10
github:
---
## Problem

`cargo check -p liquers-store --features store-conformance --lib --tests` (a row of
`scripts/check-build-matrix.sh`) builds `liquers-core` with three warnings, all in code behind the
`store-conformance` feature:

- `liquers-core/src/store_conformance/rules/explicit.rs:12` — unused `use crate::query::Key;`
- `liquers-core/src/store_conformance/rules/mod.rs:51` — unused `pub(crate) use rule;`
- `liquers-core/src/store_conformance/mod.rs:323` — `error_type_of` is never used

## Impact

Noise only: the matrix still passes, but standing warnings hide new ones in the same row. No
behaviour is affected.

## Expected behaviour

The feature builds without warnings: remove the unused import and re-export, and either use
`error_type_of` or delete it (or gate it to the configuration that uses it).

## Discovery

Seen 2026-10-10 in the build-matrix log while validating `design/sync-store-removal/`. Pre-existing:
the files were last changed in `b10774e`, and that design did not touch them. Duplicate search
found only closed, unrelated items. Eligible for automatic fixing (size `S`, no interface change);
not fixed in that branch, which must not widen.

## Resolution (2026-10-10)

Removed the unused `Key` import and `error_type_of`; the `rule` re-export is needed by the harness
tests, so it is now `#[cfg(test)]`, and `ErrorType` is imported in the `tests` module only.
`cargo check -p liquers-store --features store-conformance --lib --tests` and
`cargo check -p liquers-core --features store-conformance --lib --tests` report no warning in
`store_conformance/`; `cargo test -p liquers-core --features store-conformance --lib store_conformance`
(23 passed) and both `store_conformance_CONF` suites pass. Design:
`design/store-conformance-warning-cleanup/`.
