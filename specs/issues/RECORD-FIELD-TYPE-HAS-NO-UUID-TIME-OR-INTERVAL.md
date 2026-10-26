---
id: RECORD-FIELD-TYPE-HAS-NO-UUID-TIME-OR-INTERVAL
kind: feature
title: Record fields have no UUID, time-of-day, interval or zone-less timestamp type
status: draft
priority: P3
complexity: M
area: [records, lib/polars, web]
design:
created: 2026-10-26
github:
---

## Problem

`FieldType` (`liquers-records/src/schema.rs`) has no UUID, no time of day (Arrow `Time64`), no
duration or calendar interval (Arrow `Duration` / `Interval`), and no zone-less timestamp.
`Timestamp` is a UTC instant in every format (`record-timestamp-utc`), so a naive local date-time
is read as if it were UTC. The IPC reader refuses `Time`, `Duration`, `Interval` and
`FixedSizeBinary` (tags 9, 18, 11, 15), and the polars bridge refuses `Time` and `Duration`.

## Impact

Database columns of type `UUID`, `TIME`, `INTERVAL` and `TIMESTAMP WITHOUT TIME ZONE`, and Arrow or
Parquet files containing them, are either refused or must be read as `Text`
(`relational-database-access` question 3 proposes that as the interim rule). Workaround: cast to
text at the source.

## Expected behaviour

Decide, per type, whether it becomes a `FieldType` (with its Arrow, Parquet, polars, text and
browser mapping) or is documented as read as `Text`. UUID (`FixedSizeBinary(16)` with the Parquet
`UUID` logical type) and `Time` are the likeliest first candidates. Follow the pattern of
`specs/design/record-decimal-field-type/`.

## Discovery

Split out of `RECORD-FIELD-TYPE-HAS-NO-DECIMAL` by the 2026-10-26 bulk design, which narrowed
that issue to Decimal so it stays `L`. The refusals were checked in `decode_arrow_field`
(`liquers-records/src/formats/ipc.rs`) and `polars_dtype_to_field_type`
(`liquers-lib/src/records/polars.rs`) at cd6d5da.
