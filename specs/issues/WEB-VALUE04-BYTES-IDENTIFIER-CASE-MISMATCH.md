---
id: WEB-VALUE04-BYTES-IDENTIFIER-CASE-MISMATCH
kind: issue
title: VALUE04 fails at HEAD — bytes identifier is `Bytes`, the test expects `bytes`
status: closed
priority: P2
complexity: S
area: [web, core/value]
design: web-value04-bytes-identifier
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

## Three further stale assertions, from the same cause

Found on 2026-08-26 while planning `foreign-value-type-registration` Phase 3. **Derived from
reading, not from a run** — `wasm32-unknown-unknown` is not installed in the environment where this
was checked, the same standing this issue's original report had.

| Location | Asserts | Should be | Why |
|---|---|---|---|
| `second_value_type.rs:324` | `"bytes"` | `"Bytes"` | The failure this issue reports |
| `second_value_type.rs:336` | `assert_ne!(…, "bytes")` | `"Bytes"` | **Passes vacuously**: it compares against a string nothing produces, so it would keep passing even if text did become bytes |
| `value_bridge_VALUE.rs:156` | `"bytes"` | `"Bytes"` | `SimpleValue::Bytes.identifier()` is `"Bytes"` (`liquers-lib/src/value/simple.rs:170`) |
| `value_bridge_VALUE.rs:343` | `"js"` | `"js.Value"` | `JsOpaque::identifier()` is `"js.Value"`; a **bare** `js` would violate the naming rule outright, since bare names are reserved for `liquers-core` and `liquers-lib` |

So the suite is redder than one assertion, and the count should be confirmed by a run before the
repair is called complete.

## Expected behaviour

**Answered by `foreign-value-type-registration`:** `Bytes` is the identifier. The one-to-one rule
between a type identifier and a value variant makes `SimpleValue::Bytes.identifier()` authoritative,
the registry registers `Bytes`, and the lowercase `bytes`/`binary`/`bin`/`b` spellings are read-side
accommodations for older stores that the write path deliberately refuses. The assertions are stale;
the code is right. Fixing them is in that design's scope, confirmed by the user on 2026-08-26.

Decide which spelling is canonical — `specs/reference/VALUE_TYPE_SYSTEM.md` governs type
identifiers — then fix whichever side is wrong, so the suite is green.

## Discovery

Found while running the `liquers-web` conformance loop to validate `CORE-ERROR-PAYLOAD-SIZE`
(2026-08-25). Verified pre-existing: the same failure reproduces with that change stashed, so it
is unrelated to error boxing.

## Resolution

**Closed 2026-08-26** by `foreign-value-type-registration` (PR
[#42](https://github.com/orest-d/liquers/pull/42)).

The code was right and the assertions were stale. A baseline run confirmed **three** real failures
rather than the one reported here — the original run aborted at the first failing binary and never
reached `value_bridge_VALUE` — plus the vacuous `assert_ne!` this issue's own analysis predicted.
All four are corrected: `second_value_type.rs:324` and `:336`, `value_bridge_VALUE.rs:156` and
`:343`.

`:343` was the interesting one: it expected a bare `js`, which the naming rule forbids outright,
since bare names are reserved for `liquers-core` and `liquers-lib`.

Evidence: `cargo test -p liquers-web --target wasm32-unknown-unknown --features debug-handles` —
16 targets, 141 tests, zero failures.
