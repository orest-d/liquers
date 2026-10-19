---
id: WEB-OUTBOUND-INT64-BEYOND-DOUBLE-UNTESTED
kind: issue
title: An i64 beyond 2^53 leaving for JavaScript has unconfirmed, untested behaviour
status: closed
priority: P3
complexity: S
area: [web]
design: web-outbound-int64-precision
created: 2026-10-17
github:
---

## Problem

`VALUE03` requires integer boundaries to round trip exactly or raise `ConversionError`, never
losing precision silently. The inbound direction is handled (`liquers-web/src/bridge.rs:234-265`:
`BigInt` is the lossless path; a plain number beyond 2^53 was already imprecise in JavaScript) and
tested. The outbound direction is not: integers leave through `try_into_json_value` and
`serialize_to_js` (`bridge.rs:389-394`), which does not enable
`serialize_large_number_types_as_bigints`, so output never uses `BigInt`. Whether an i64 beyond
2^53 raises (serde-wasm-bindgen's probable default) or silently degrades to an imprecise `number`
is unconfirmed, and no test covers it.

## Impact

If the path silently degrades, a Rust command returning a large i64 (an id, a hash) hands
JavaScript a wrong value with no error — exactly what `VALUE03` exists to prevent. If it raises,
the behaviour is fine but unpinned.

## Expected behaviour

Establish what the path does, decide the policy (raise, or serialize as `BigInt`), and pin it
with a `value03`-series test for the outbound direction.

## Discovery

Bidirectional conformance audit (2026-10-17); the fact-check could not confirm
serde-wasm-bindgen's behaviour from this tree and found no test.

## Resolution (2026-10-18)

Pinned, not changed. `value03_outbound_integer_boundaries` in
`liquers-web/tests/value_bridge_VALUE.rs` asserts that integers inside ±(2^53 − 1) reach JavaScript
as exact numbers and that anything outside, scalar or nested, is refused with `ConversionError`
(serde-wasm-bindgen's default). The policy is stated in `serialize_to_js`'s doc comment. Design:
`specs/design/web-outbound-int64-precision/`.
