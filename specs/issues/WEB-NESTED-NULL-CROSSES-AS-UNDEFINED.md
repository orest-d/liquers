---
id: WEB-NESTED-NULL-CROSSES-AS-UNDEFINED
kind: issue
title: A null nested in an array or object reaches JavaScript as undefined
status: closed
priority: P2
complexity: S
area: [web]
design:
created: 2026-10-26
github:
---
## Problem

`value_to_js` (`liquers-web/src/bridge.rs`) returns `null` for a top-level `None`, but converts every
other value through `serialize_to_js`, and `serde-wasm-bindgen` serializes a JSON `null` as
`undefined` (its default `serialize_missing_as_null(false)`; `error.rs` already works around it for
the validation report). Measured on a wasm build (2026-10-26): a command returning
`{ a: null, b: [null], c: { d: null } }` evaluates to `{ a: undefined, b: [undefined], c: { d:
undefined } }`.

## Impact

A structure does not round-trip: `{ a: null }` comes back with `a === undefined`, so
`JSON.stringify` drops the key, `"a" in result` is still true but `result.a === null` is false.
Workaround: compare with `== null`.

## Expected behaviour

A nested `null` arrives as `null`, as a top-level one does — `Serializer::new()
.serialize_maps_as_objects(true).serialize_missing_as_null(true)` in `serialize_to_js` is the likely
fix — with a `VALUE02` case for it.

## Discovery

Running the examples of `specs/guides/JAVASCRIPT_GUIDE.md` under Node (2026-10-26).

## Resolution (2026-10-26)

Fixed in `json_to_js` (`liquers-web/src/bridge.rs`), which now serializes a value's JSON form with
`serialize_missing_as_null(true)` beside `serialize_maps_as_objects(true)`. The change is confined to
value conversion: `serialize_to_js`, used for metadata and other Rust structs, is unchanged, so an
absent `Option` field there still crosses as `undefined`. `value02_nested_null_roundtrip` in
`liquers-web/tests/value_bridge_VALUE.rs` round-trips `{ a: null, b: [null], c: { d: null } }`; it
failed before the fix and passes after it.
