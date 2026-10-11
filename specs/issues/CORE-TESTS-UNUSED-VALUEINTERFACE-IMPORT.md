---
id: CORE-TESTS-UNUSED-VALUEINTERFACE-IMPORT
kind: issue
title: Two liquers-core integration tests import ValueInterface without using it
status: closed
priority: P3
complexity: S
area: [core/assets, build]
created: 2026-10-10
github:
---
## Problem

`cargo test -p liquers-core --tests` warns `unused import: ValueInterface` in two integration
tests:

- `liquers-core/tests/keyed_version_cascade.rs:25`
- `liquers-core/tests/metadata_only_entry_reload.rs:29`

## Impact

Noise only: standing warnings hide new ones in the same build. No behaviour is affected.

## Expected behaviour

The tests build without warnings.

## Discovery

Seen 2026-10-10 while fixing `STORE-CONFORMANCE-FEATURE-BUILD-WARNINGS` (PR #106), which did not
widen to cover it. Duplicate search in `specs/index.csv` found nothing.

## Resolution (2026-10-10)

Removed the unused import from both files; both suites pass with no warnings.
