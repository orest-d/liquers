---
id: RECORD-FIELD-TYPE-HAS-NO-UUID-TIME-OR-INTERVAL
kind: feature
title: Record fields have no UUID, time-of-day, interval or zone-less timestamp type
status: draft
priority: P3
complexity: L
area: [records, lib/polars, web]
design: record-uuid-time-duration-types
created: 2026-10-26
github:
---

## Problem

`FieldType` (`liquers-records/src/schema.rs`) has no UUID, no time of day (Arrow `Time64`), no
duration or calendar interval (Arrow `Duration` / `Interval`), and no zone-less timestamp.
`Timestamp` is a UTC instant in every format (`record-timestamp-utc`), so a naive local date-time
is read as if it were UTC. The IPC reader refuses `Time`, `Duration`, `Interval` and
`FixedSizeBinary` (tags 9, 18, 11, 15), and the polars bridge refuses `Time` and `Duration`.

## Problem Example

```
-R/ops/jobs.arrow/-/ns-rec/head-5
```

where `ops/jobs.arrow` has `id: FixedSizeBinary(16)` (extension `arrow.uuid`), `starts:
Time64(µs)` and `took: Duration(µs)`. Today (`cd6d5da`): "Arrow IPC: field 'id' has unsupported
Arrow type FixedSizeBinary (tag 15)", and the same for `Time` and `Duration` once `id` is removed.
Should be: five typed rows that write back to IPC with the same Arrow types.

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

Complexity corrected from `M` to `L` by the bulk design (2026-10-26): new `FieldType` variants
reach every table format, the polars bridge and the browser bridge, across three crates — the
reach that made `record-decimal-field-type` `L` for one variant. The design proposes `Uuid`,
`Time` and `Duration` as types and documents interval and zone-less timestamp as read as `Text`.

## Discovery

Split out of `RECORD-FIELD-TYPE-HAS-NO-DECIMAL` by the 2026-10-26 bulk design, which narrowed
that issue to Decimal so it stays `L`. The refusals were checked in `decode_arrow_field`
(`liquers-records/src/formats/ipc.rs`) and `polars_dtype_to_field_type`
(`liquers-lib/src/records/polars.rs`) at cd6d5da.
