---
id: SIMPLE-VALUE-WRITES-FEWER-FORMATS-THAN-DECLARED
kind: issue
title: liquers-lib's base value declares formats its serializer refuses
status: draft
priority: P3
complexity: S
area: [lib/value]
design:
created: 2026-09-25
github:
---
# `liquers-lib`'s base value declares formats its serializer refuses

## Problem

`SimpleValue::type_descriptions()` returns `liquers_core::value::Value::type_descriptions()`
unchanged (`liquers-lib/src/value/simple.rs`, "shares its identifiers and descriptions rather than
maintaining a second, drifting copy"). But `SimpleValue::as_bytes` writes fewer formats than core's
`Value::as_bytes`:

- it accepts only `txt` and `html` of core's textual set, and refuses `css`, `js`, `py` and `rs`,
  which core writes;
- it refuses `b` / `bin` / `bytes` for `Text` and `Bytes`;
- it refuses `txt` and `html` for `Query` and `Key`, which core writes as the encoded query or key.

That is 42 declared (type, format) pairs the writer refuses, so the declared write surface is
wrong. It matters because `supported_data_formats` is what the write path checks
(`CORE-METADATA-FORMAT-TYPE-CONSISTENCY`): a `Bytes` value requested as `data.bin` passes the check
and then fails in `as_bytes`.

The full list is recorded in the test
`every_declared_format_round_trips_or_is_recorded_as_unwritable` (`simple.rs`, the `UNWRITABLE`
constant). The test fails when the list changes in either direction.

## Expected behaviour

Either `SimpleValue::as_bytes` writes what core's `Value::as_bytes` writes, which removes the
duplication the comment says it avoids, or `SimpleValue` declares its own narrower `TypeInfo`s.
The first is the better fix: the two serializers mirror each other variant for variant, and the
divergence is an accident. `UNWRITABLE` then empties.

## Discovery

Found 2026-09-25, record-streams Phase 4 Step 0.2, by the TypeInfo-driven round-trip test the plan
asked for. The test replaced per-format tests that built their own bytes and so could not see
this. `DATA-FORMAT-CONSTANTS-AND-TOOLING` covers the missing shared vocabulary that lets such drift
happen, but not this instance.
