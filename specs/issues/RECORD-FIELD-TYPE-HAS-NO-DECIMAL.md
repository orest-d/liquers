---
id: RECORD-FIELD-TYPE-HAS-NO-DECIMAL
kind: feature
title: Record fields have no exact Decimal type, so decimal columns are refused or lose precision
status: draft
priority: P3
complexity: L
area: [records, lib/polars, web]
design: record-decimal-field-type
created: 2026-10-21
github:
---

## Problem

`FieldType` (`liquers-records/src/schema.rs`; `RECORD_STREAMS.md` §Schema) has `Bool`, `Int`,
`UInt`, `Float`, `Text`, `Binary`, `Date`, `Timestamp` and `Vector`, and no exact decimal. The
IPC reader refuses an Arrow `Decimal` column (`decode_arrow_field`: "unsupported Arrow type
Decimal (tag 7)"). The polars bridge refuses a polars `Decimal` dtype
(`polars_dtype_to_field_type`), and polars is built without `dtype-decimal`. So a
`NUMERIC`/`DECIMAL` value, such as money, can only become `Float` (silent precision loss) or `Text`
(type loss). `record-streams` deferred Decimal deliberately for data Liquers produces. It stops
being deferrable for data read from a database (`record-streams/engine-survey.md` §3, gap 2).

## Problem Example

`finance/ledger.arrow` holds `amount: Decimal128(10, 2)`.
`-R/finance/ledger.arrow/-/ns-rec/head-5` validates, then fails to evaluate with "Arrow IPC: field
'amount' has unsupported Arrow type Decimal (tag 7)". Should be: five rows with an exact `amount`
column, written as `12.30` in CSV and `"12.30"` in JSON, and back to IPC and Parquet as
`Decimal128(10, 2)`.

## Impact

This blocks a correct relational access layer (`design/relational-database-access/`, question 3)
and makes IPC/Parquet files with decimal columns unreadable today. Workaround: convert the
columns to text before Liquers reads them.

## Expected behaviour

A `Decimal { precision, scale }` field type (Arrow `Decimal128`) carried through every table
format, the polars bridge and the browser bridge, with no silent rounding.

The issue as first filed also listed UUID, intervals and a time-of-day / time-zone distinction.
Since `record-timestamp-utc` (complete), `Timestamp` is a UTC instant in every format. The other
types are split off as `RECORD-FIELD-TYPE-HAS-NO-UUID-TIME-OR-INTERVAL`.

## Discovery

Split out of `NO-RELATIONAL-DATABASE-ACCESS-LAYER` by the 2026-10-21 bulk design
(`design/relational-database-access/`); confirmed against `RECORD_STREAMS.md` §Schema at 18732dd.
Narrowed to Decimal, and the refusals re-verified at cd6d5da, by the 2026-10-26 bulk design
(`specs/design/record-decimal-field-type/`).
