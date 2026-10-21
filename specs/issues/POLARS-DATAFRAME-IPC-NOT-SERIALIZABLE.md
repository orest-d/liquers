---
id: POLARS-DATAFRAME-IPC-NOT-SERIALIZABLE
kind: issue
title: A polars DataFrame cannot be written or read as Arrow IPC although polars is built with ipc
status: draft
priority: P3
complexity: S
area: [lib/polars]
created: 2026-10-18
design: polars-dataframe-ipc
github:
---

## Problem

`liquers-lib/src/polars/serde.rs` refuses `PolarsDataFormat::Ipc` in both directions ("not
implemented yet"), along with JSON, NDJSON and Avro. The `polars` dependency in
`liquers-lib/Cargo.toml` already enables the `ipc` feature.

## Impact

IPC is the cheapest interchange format with Python and Arrow consumers. Without it, `liquers-py`
crosses DataFrames as Parquet, which costs an encode and a decode per crossing, and a stored
DataFrame cannot be written as `.arrow`/`.feather`. Workaround: Parquet.

## Expected behaviour

`ipc` (and `arrow`/`feather` spellings, if adopted) serializes and deserializes through
`IpcWriter`/`IpcReader`, and `polars.DataFrame`'s `TypeInfo` lists it.

## Discovery

Choosing the DataFrame crossing for `specs/design/liquers-py/` (Phase 1, question 2), 2026-10-18.

## Update 2026-10-20 — confirmed at HEAD, designed

Still reproduces: `PolarsDataFormat::Ipc` sits in the refusing arms of both codec functions in
`liquers-lib/src/polars/serde.rs`, and `polars.DataFrame`'s `TypeInfo` declares only `csv`,
`csv:comma`, `csv_comma` and `parquet`, so even a working codec would be refused on the write path.
Example: `Value::from_polars_dataframe(df).as_bytes("feather")` returns "Polars serialization for
data_format 'feather' is not implemented yet"; `-R/data/sales.feather/-/ns-pl/head-5` fails the same
way. Design: `specs/design/polars-dataframe-ipc/` (also adopts the `arrow` spelling, as
`liquers-records` does). JSON and NDJSON are filed separately as `POLARS-DATAFRAME-JSON-NOT-SERIALIZABLE`.
