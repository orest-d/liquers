---
id: RECORD-TABLE-FORMAT-REFUSES-CSV-REFINEMENTS-POLARS-ACCEPTS
kind: issue
title: A record view refuses csv:semicolon and csv:pipe, which a DataFrame and the type registry accept
status: draft
priority: P3
complexity: S
area: [records]
design: record-table-csv-refinements
created: 2026-10-26
github:
---

## Problem

`TableFormat::from_data_format` (`liquers-records/src/formats/mod.rs`) accepts `csv`, `csv:comma`,
`tsv`, `csv:tab` and refuses every other refinement, while its doc comment says its names "match
the DataFrame serializer's". `parse_polars_data_format` (`liquers-lib/src/polars/serde.rs`) also
accepts `csv:semicolon`, `csv:pipe`, `csv_comma`, `csv_tab`, `csv_semicolon`, `csv_pipe` and any
single-character `csv:<c>`. `TypeInfo::supports_data_format` matches a refinement by its base, so
the registry says a `RecordView` supports `csv:semicolon`, and the write then fails in the codec.

## Problem Example

A record view with declared data format `csv:semicolon` (from a Rust caller today, or from
`…/data_format-csv-semicolon/…` once `design/query-data-format-command/` lands): `set_state`
passes the registry check, then serialization fails with "TableFormat::from_data_format: unknown
data format 'csv:semicolon'". The same data as a `polars.DataFrame` writes `;`-separated CSV.

## Impact

The two table types disagree on what a format name means, contrary to the documented intent; a
user who switches between `ns-pl` and `ns-rec` gets a late serialization error. Workaround: write a
DataFrame, or use `csv`/`tsv`.

## Expected behaviour

Either `TableFormat::from_data_format` accepts the same spellings as `parse_polars_data_format`
(`TableFormat::Csv { separator }` already carries any separator), or both share one parser. Which
crate owns the shared table is a choice for the fix (`liquers-records` cannot depend on
`liquers-lib`).

## Discovery

Bulk design of `QUERY-CANNOT-CHOOSE-A-DATA-FORMAT` (2026-10-26), comparing the two format parsers.
Found by reading; not run.
