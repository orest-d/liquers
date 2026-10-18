---
id: FOREIGN-VALUE-CANNOT-BE-DESERIALIZED
kind: issue
title: A foreign value written as bytes can never be read back
status: draft
priority: P2
complexity: M
area: [lib/value, core/value, py]
design: liquers-py
created: 2026-10-18
github:
---

## Problem

`ForeignValue::as_bytes` lets a foreign value (`ExtValue::Foreign`) write a byte form, but nothing
can rebuild one. `ExtValue::deserialize_from_bytes` (`liquers-lib/src/value/mod.rs`) has arms for
`Image`, `polars.DataFrame`, `RecordView` and `RecordSource` only. A foreign identifier such as
`py.Object` falls through to the unknown-identifier error. Its `TypeInfo` can declare a format, and
the write path then stores bytes that every later read refuses.

The read path also cannot be gated per environment. `deserialize_stored_value`
(`liquers-core/src/assets.rs`) checks only that the registry *contains* the identifier, not that it
lists a format. A Python environment with the pickle codec off would therefore unpickle whatever a
shared store held, as soon as a decoder exists.

## Expected

A foreign value with a byte form round-trips through a store, and an environment whose registry
lists no format for the type never decodes it.

## Resolution plan

`specs/design/liquers-py/` Phase 2: a `register_foreign_decoder` hook in `liquers-lib`, plus a
degraded read for registered types that declare no data formats.
