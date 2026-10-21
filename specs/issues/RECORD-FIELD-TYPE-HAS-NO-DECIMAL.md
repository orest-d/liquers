---
id: RECORD-FIELD-TYPE-HAS-NO-DECIMAL
kind: feature
title: Record fields have no Decimal, UUID, interval or time-zone-aware time type
status: draft
priority: P3
complexity: L
area: [records, lib/polars, web]
design:
created: 2026-10-21
github:
---

## Problem

`FieldType` (`liquers-records/src/schema.rs`; `RECORD_STREAMS.md` §Schema) has `Bool`, `Int`,
`UInt`, `Float`, `Text`, `Binary`, `Date`, `Timestamp` and `Vector`. There is no exact decimal,
so a `NUMERIC`/`DECIMAL` value — money — can only become `Float` (a silent precision loss) or
`Text` (a type loss). `uuid`, intervals and a `TIME` / time-zone-aware distinction are also
absent. `record-streams` deferred Decimal deliberately for data Liquers produces; it stops being
deferrable for data read from a database (`record-streams/engine-survey.md` §3, gap 2).

## Impact

Blocks a correct relational access layer (`design/relational-database-access/`), and limits
Parquet/IPC files with decimal columns today. Workaround: read such columns as text.

## Expected behaviour

A `Decimal { precision, scale }` field type (Arrow `Decimal128`) carried through every table
format, the polars bridge and the browser bridge, or an explicit, documented rule that decimals
are read as text. Other types as needed by the first database backend.

## Discovery

Split out of `NO-RELATIONAL-DATABASE-ACCESS-LAYER` by the 2026-10-21 bulk design
(`design/relational-database-access/`); confirmed against `RECORD_STREAMS.md` §Schema at 18732dd.
