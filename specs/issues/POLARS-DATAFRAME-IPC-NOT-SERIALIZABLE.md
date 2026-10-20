---
id: POLARS-DATAFRAME-IPC-NOT-SERIALIZABLE
kind: issue
title: A polars DataFrame cannot be written or read as Arrow IPC although polars is built with ipc
status: draft
priority: P3
complexity: S
area: [lib/polars]
created: 2026-10-18
design:
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
