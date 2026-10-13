---
id: VALUE-TO-JSON-PANICS-ON-NON-FINITE-FLOAT
kind: issue
title: Value::try_into_json_value panics on a non-finite float
status: draft
priority: P0
complexity: S
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
