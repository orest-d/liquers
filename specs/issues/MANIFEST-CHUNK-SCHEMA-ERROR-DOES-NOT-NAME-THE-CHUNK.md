---
id: MANIFEST-CHUNK-SCHEMA-ERROR-DOES-NOT-NAME-THE-CHUNK
kind: issue
title: A chunk that violates a manifest's uniform_schema is refused without naming which chunk
status: closed
priority: P3
complexity: S
area: [records]
design: manifest-chunk-error-identity
created: 2026-10-07
github:
---
# A chunk that violates `uniform_schema` is refused without naming which chunk

## Problem

`check_view_matches_schema` (`liquers-records/src/sources.rs` ≈100) refuses a chunk whose view
does not match the manifest's declared `uniform_schema`. Its messages name the field and the
violated constraint, but not the chunk:

```text
ManifestSource: chunk field 'amount' holds 1 null(s), but uniform_schema declares it not null
```

Neither `read_chunk` nor `view_from_chunk_value` adds the chunk's identity, which is its index and
its key or query. In a manifest over a directory of files, the reader has to bisect to find which
file is bad.

## Expected behaviour

The error names the chunk by its global index and by its key, or by its query when it is
unkeyed. For example: `chunk 2 (-R/data/raw/bad.csv/-/ns-rec/to_record-csv): field 'amount'
holds 1 null(s) …`. The same applies to the field-count and field-type mismatch messages.

## Discovery

Found 2026-10-07 by `design/manifest-over-stored-csv-test/`. Its test
`manifest_csv_chunk_schema_error_names_the_chunk`
(`liquers-lib/tests/records_manifest_over_csv_files.rs`) is `#[ignore]`d with this ID.

## Resolution

Fixed 2026-10-08 by `design/manifest-chunk-error-identity/` on branch
`claude/manifest-chunk-error-identity`. `ManifestSource::advance` (`liquers-records/src/sources.rs`)
now passes a chunk read error through a private `name_chunk`, which prefixes the message with
`chunk <global index> (<key>): `, or the encoded query for an unkeyed chunk, and keeps the error
type and the other payload fields. The example above now reads
`chunk 2 (data/raw/bad.csv): ManifestSource: chunk field 'amount' holds 1 null(s), …`.

A single chunk read by `ns-rec/rowid` through the `pub` `view_from_chunk_value` is unchanged: that
caller already names the chunk it asked for.

Evidence: `manifest_csv_chunk_schema_error_names_the_chunk`
(`liquers-lib/tests/records_manifest_over_csv_files.rs`) is no longer ignored and passes; new unit
tests `chunk_error_names_unkeyed_chunk_by_query` and `chunk_error_keeps_error_type` in
`liquers-records/src/sources.rs`.
