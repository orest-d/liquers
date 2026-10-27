---
id: POLARS-DATAFRAME-JSON-NOT-SERIALIZABLE
kind: issue
title: A polars DataFrame cannot be written or read as JSON or NDJSON
status: draft
priority: P3
complexity: S
area: [lib/polars]
design: polars-dataframe-json
created: 2026-10-20
github:
---

## Problem

`parse_polars_data_format` (`liquers-lib/src/polars/serde.rs`) maps `json` to
`PolarsDataFormat::Json` and `ndjson` / `jsonl` to `PolarsDataFormat::NdJson`, but both codec
functions refuse them ("Polars … for data_format 'json' is not implemented yet"). The `polars`
dependency in `liquers-lib/Cargo.toml` does not enable polars' `json` feature, so `JsonWriter` /
`JsonReader` are not available. `polars.DataFrame`'s `TypeInfo` (`liquers-lib/src/value/mod.rs`)
correctly leaves both out, with a comment saying why.

## Problem Example

`-R/data/sales.csv/-/ns-pl/head-5/data.json` (valid query): the result is a `polars.DataFrame`
declared as `json`; serving it fails with "Polars serialization for data_format 'json' is not
implemented yet", and `set_state` refuses the format as undeclared. Should be: an array of five
row objects, readable back into an equal frame. Verified at HEAD (2026-10-26).

## Impact

A DataFrame cannot be stored or served as JSON, the format every web client reads, without first
converting it to a record view (`ns-rec`), which writes `json` and `ndjson`. A query such as
`-R/data/sales.csv/-/ns-pl/head-5/data.json` is refused by the write path. Workaround: convert to a
record view, or use CSV.

## Expected behaviour

Either implement both through polars' `json` feature (a dependency feature change, so the build
matrix and the wasm rows need checking) and declare `json`, `ndjson`, `jsonl`; or remove the
unimplemented variants from `PolarsDataFormat` so the parser stops accepting formats nothing
writes. The choice of JSON orientation (records vs columns) should match `liquers-records`'
`JsonOrient::Records` default, so the two table types write the same JSON.

## Discovery

Found 2026-10-20 in a bulk-design run, while designing `specs/design/polars-dataframe-ipc/`, which
implements the IPC arms of the same functions and leaves these out of scope. `avro` is refused the
same way and is left out here because nothing in the workspace uses it.
