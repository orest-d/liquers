---
id: WEB-OBJECT06-EXPECTS-A-STALE-ERROR-TYPE-COUNT
kind: issue
title: OBJECT06 asserts 22 ErrorType variants, and its own list now has 23
status: draft
priority: P2
complexity: S
area: [web]
design: web-object06-error-type-exhaustiveness
created: 2026-09-30
github:
---

## Problem

`liquers-web/tests/objects_OBJECT.rs:133` asserts `ALL_ERROR_TYPES.len() == 22`. Commit
`1c4acaf` (2026-09-28, `ErrorType::StatusConflict`) added the new variant to `ALL_ERROR_TYPES` but
left the expected count at 22, so `object06_every_enum_variant_roundtrips` fails:
`left: 23, right: 22`.

## Impact

The routine `liquers-web` Node loop (`cargo test -p liquers-web --target wasm32-unknown-unknown
--features debug-handles`) is red on its base, so a real regression in `objects_OBJECT` would be
indistinguishable from this one.

## Expected behaviour

The assertion expects 23 — better, it is derived so that adding a variant fails only the
list's exhaustiveness check (a `match` over `ErrorType` without a default arm), not a hand-kept
number.

## Discovery

Found 2026-09-30 running the full `liquers-web` Node loop in step 7 of
`design/store-conformance-backlog/`. Unrelated to that design; it fails identically without its
changes.
