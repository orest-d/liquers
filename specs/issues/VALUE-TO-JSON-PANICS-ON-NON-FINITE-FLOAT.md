---
id: VALUE-TO-JSON-PANICS-ON-NON-FINITE-FLOAT
kind: issue
title: Value::try_into_json_value panics on a non-finite float
status: closed
priority: P0
complexity: M
area: [core/value]
design: value-to-json-non-finite-float
created: 2026-10-12
github:
---
## Problem

`Value::try_into_json_value` (`liquers-core/src/value.rs`, the `Value::F64` arm) converts a float
with `serde_json::Number::from_f64(*n).unwrap()`. `from_f64` returns `None` for NaN and ±infinity,
so converting such a value panics in library code — against the `CLAUDE.md` rule "no `unwrap()`
in library code" — instead of returning an `Error`.

## Impact

P0 under `DOCS_STRUCTURE_GUIDE.md` §4.4 — a panic on a supported path: `Value::from_f64` accepts a
non-finite value, and the documented `try_into_json_value` then panics. Any path that turns a value into JSON — serialization, the records → JSON bridge, a template data
model, an HTTP response — can crash on a computed NaN (0/0, a failed parse, an empty mean). The
signature already returns `Result`, so the fix needs no API change.

## Expected behaviour

Return a conversion error naming the value, or map non-finite floats to `null` deliberately and
document it; either way no panic. A unit test covers NaN, +∞ and −∞.

## Discovery

Templating research for `agent-memory-mvp`, 2026-10-12, reading the conversion a template engine's
data model would go through.

## Update 2026-10-13 — scope widened by the design

`design/value-to-json-non-finite-float/` covers this panic and also the JSON *byte* path, which
answers the same question the opposite way (overlap test T2). There, `serde_json` writes `null` for
every non-finite float. A core `F64` therefore reads back as `None`, and a `liquers-lib`
`SimpleValue::Array` holding a NaN reads back as an unrelated `Object`. Complexity raised from `S`
to `M` accordingly. The maintainer decided the policy, which is recorded in that design's Phase 1.

## Resolution 2026-10-13

Fixed by `design/value-to-json-non-finite-float/`. `try_into_json_value` refuses NaN and ±infinity
with a `ConversionError` in `liquers-core`, `liquers-lib` (`SimpleValue`, and so `ExtValue` and
record cells) and `liquers-py`, through `liquers_core::value::json_f64::to_number`. The JSON byte
path follows the policy in `specs/guides/SERIALIZATION_GUIDE.md`. Evidence: the tests citing AC-1 to
AC-11 in `liquers-core/src/value.rs`, `liquers-lib/src/value/simple.rs`,
`liquers-lib/tests/record_scalar_reading.rs` and `liquers-py/src/value.rs`, in commits `7718183`,
`4e22ff4`, `54f7b7e` and `ae04ac2`.
