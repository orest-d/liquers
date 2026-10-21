---
id: SIMPLE-VALUE-I32-JSON-READS-BACK-AS-I64
kind: issue
title: liquers-lib's base value reads an I32 written as JSON back as I64
status: draft
priority: P3
complexity: S
area: [lib/value]
design:
created: 2026-10-20
github:
---

## Problem

`SimpleValue::as_bytes("json")` (`liquers-lib/src/value/simple.rs`) writes `SimpleValue::I32 { value: 42 }`
as the bare number `42`. `deserialize_from_bytes(b"42", "I32", "json")` has no `"I32"` arm in its
`"json"` match, so it falls to `Self::from_plain_json`, and `try_from_json_value` turns every
integer into `I64`. The value comes back as `I64 { value: 42 }` and reports identifier `I64`.

Two tests pin the defect as intended: `test_json_roundtrip_i32` expects `I64`, and
`every_declared_format_round_trips` maps `("json", I32)` to an `I64` expectation. Every other
scalar identifier round-trips (`I64`, `F64`, `Bool`, `Text`, `None`), and the structured ones are
dispatched explicitly (`Metadata`, `AssetInfo`, `Recipe`, `CommandMetadata`).

## Impact

An `I32` stored by `liquers_lib::value::Value` (default format `json`) is reloaded as an `I64`. A
command that matches on the variant, or compares identifiers, sees a different type; conversions
(`try_into_i32`) still work, so the visible effect is small. It breaks the rule
`VALUE_TYPE_SYSTEM.md` states and `vts7.2` checks for core: a value round-trips through its own
identifier.

## Expected behaviour

An `"I32"` arm (and, for symmetry, `"I64"`, `"Bool"`, `"Text"`, `"None"`) in the `"json"` match reads
the payload type directly, and the two tests expect `I32`. The core mirror of this defect (an `I64`
read back as `I32`, and structured variants read back as `Object`/`Array`) is
`CORE-VALUE-JSON-READ-IGNORES-STRUCTURED-IDENTIFIERS`, designed in
`specs/design/core-value-json-identifier-dispatch/`; the same pattern applies here.

## Discovery

Found 2026-10-20 in a bulk-design run while confirming `CORE-VALUE-JSON-READ-IGNORES-STRUCTURED-IDENTIFIERS`
at HEAD: `SimpleValue`'s `"json"` arm and its tests were read for comparison. Not run; the tests
named above assert the current behaviour.
