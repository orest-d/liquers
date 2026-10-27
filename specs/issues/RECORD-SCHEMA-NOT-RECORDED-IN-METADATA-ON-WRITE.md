---
id: RECORD-SCHEMA-NOT-RECORDED-IN-METADATA-ON-WRITE
kind: issue
title: A record view stored in a schema-less format does not record its schema in the metadata
status: draft
priority: P3
complexity: M
area: [lib/value, core/assets]
design: record-schema-attribute-on-write
created: 2026-10-20
github:
---

## Problem

When a `RecordView` is persisted as `csv`, `tsv`, `ndjson`, `json` or `md`, the bytes carry column
names and text only. Its field types, roles (`Id`, …), labels and nullability are lost, and nothing
puts them anywhere else: serialization (`DefaultValueSerializer::as_bytes`,
`liquers-core/src/value.rs`) returns bytes only, so a value has no way to contribute facts to the
metadata the asset layer stores beside them.

`specs/design/deserialization-with-metadata/` makes the *read* side honour a schema found in the
metadata attribute `records.schema`; without a producer that only helps records an application or
a command annotated by hand.

## Problem Example

A keyed recipe `data/t.csv` yields a view with `id: Int (Id role)`, `code: Text`, `amount: Float`
and rows `(1, "123", 2.0)`, `(2, "456", 3.0)`; it is persisted as `csv`. After a restart the fast
track reads it with an inferred schema: `code` and `amount` are `Int`, the `Id` role is gone.
Should be: the stored record carries `attributes["records.schema"]`, and the reload (with
`design/deserialization-with-metadata/`) has the computed schema.

## Impact

A computed table stored as CSV reloads (fast track, after a restart) with an inferred schema that
can differ from the one computed: a `Text` column of digits becomes `Int`, the `Id` role is lost.
The cached and the fresh value then disagree. Workaround: store tables as `ipc` or `parquet`, which
embed the schema, or set the attribute from the command with `context.set_attribute`
(`design/metadata-application-attributes/`).

## Expected behaviour

When a value is serialized for storage, it can add attributes to the metadata written with the
bytes; a `RecordView` written in a schema-less format adds `records.schema`. Options, not chosen
here: a defaulted `DefaultValueSerializer` method returning attributes for a format (called at the
same asset-layer sites as `serialization_warnings` in `design/serialization-lossy-write-warnings/`,
which suggests one combined "what serialization adds to the metadata" hook); or having the
`ns-rec` commands set the attribute themselves (covers only those commands).

## Discovery

Filed 2026-10-20 in a bulk-design run while designing `specs/design/deserialization-with-metadata/`,
which scopes the write side out (its question 1).
