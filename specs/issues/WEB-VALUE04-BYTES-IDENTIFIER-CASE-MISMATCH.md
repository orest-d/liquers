---
id: WEB-VALUE04-BYTES-IDENTIFIER-CASE-MISMATCH
kind: issue
title: VALUE04 fails at HEAD — bytes identifier is `Bytes`, the test expects `bytes`
status: draft
priority: P2
complexity: S
area: [web, core/value]
design: 
created: 2026-08-25
github:
---
## Problem

`liquers-web`'s wasm conformance suite has one failing test at HEAD:

```
---- value04_bytes_are_not_confused_with_text_second_value_type ----
panicked at liquers-web/tests/second_value_type.rs:324:5:
assertion `left == right` failed: bytes must not become text
  left: "Bytes"
 right: "bytes"
```

The conversion itself is correct — a `Uint8Array` does become a bytes value, not text. Only the
*identifier casing* disagrees: the second value type reports `Bytes`, and the test asserts
`bytes`.

Reproduce with:

```bash
cargo test -p liquers-web --target wasm32-unknown-unknown --features debug-handles \
    --test second_value_type
```

All other suites in that loop pass (10 suites, 84 tests).

## Impact

The documented `liquers-web` test loop is red, so a genuine regression in it is easy to miss.
If `Bytes` is the intended identifier, this is only a stale assertion; if `bytes` is intended,
the type identifier is wrong and the mismatch is visible to JavaScript through
`value.identifier()`.

## Expected behaviour

Decide which spelling is canonical — `specs/reference/VALUE_TYPE_SYSTEM.md` governs type
identifiers — then fix whichever side is wrong, so the suite is green.

## Discovery

Found while running the `liquers-web` conformance loop to validate `CORE-ERROR-PAYLOAD-SIZE`
(2026-08-25). Verified pre-existing: the same failure reproduces with that change stashed, so it
is unrelated to error boxing.
