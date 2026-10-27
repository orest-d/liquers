---
id: WEB-NESTED-BYTES-REFUSED-NOT-DEGRADED
kind: issue
title: A Uint8Array nested in an object is refused, while the bridge documents it as degrading to numbers
status: draft
priority: P3
complexity: S
area: [web]
design:
created: 2026-10-26
github:
---
## Problem

`js_to_json` in `liquers-web/src/bridge.rs` carries the comment "a `Uint8Array` *nested* inside an
array or object degrades to an array of numbers, because JSON has no byte type". The code does not:
a nested `Uint8Array` is neither an array nor a plain object, so it reaches the final
`Cannot structurally convert the JavaScript value at {path}` error. Measured on a wasm build
(2026-10-26): a command returning `{ c: new Uint8Array([1]) }` fails with `conversion_error` at
`$.c`. No test in `tests/value_bridge_VALUE.rs` covers either behaviour.

## Impact

A command cannot return bytes inside a structure; the documentation in the code says otherwise.
Workaround: return the bytes on their own, or convert them to an array of numbers first.

## Expected behaviour

Decide which is intended, make the comment and the code agree, and pin it with a `VALUE` test.
Refusing is defensible (it is exact); degrading silently is what the comment promises.

## Discovery

Writing `specs/reference/JAVASCRIPT_INTEGRATION.md` and checking it against a build (2026-10-26).
