---
id: STORED-UNTYPED-FILE-OF-UNLISTED-FORMAT-CANNOT-BE-READ
kind: issue
title: A stored file with no type identifier, in a format the base value does not list (csv, png, parquet), cannot be loaded as a resource
status: closed
priority: P2
complexity: S
area: [lib/value, core/assets]
design: simple-value-untyped-and-scalar-reads
created: 2026-10-07
github:
---
# A stored file with no type identifier, in a format the base value does not list, cannot be loaded

## Problem

Take a file placed in a store by hand, or by any tool other than Liquers. Its metadata carries a
data format taken from the extension (`csv`), but either no type identifier or `Bytes`. A query
such as `-R/data/short.csv/-/ns-rec/to_record-csv` then fails with "No recipe found for key
data/short.csv".

The asset manager's fast track calls `CombinedValue::deserialize_from_bytes(b, "", "csv")`, and
both halves refuse:

- `SimpleValue::deserialize_from_bytes` (`liquers-lib/src/value/simple.rs` ≈670) matches on the
  format first. `csv` is not one of its formats, so it returns "Unsupported format in
  deserialize_from_bytes: csv". This happens even when the identifier is `""` or `Bytes`, which
  only the `bytes`/`b`/`bin` arm accepts.
- `ExtValue::deserialize_from_bytes` (`liquers-lib/src/value/mod.rs` ≈814) matches on the
  identifier, and `""` or `Bytes` falls through to "Unsupported type identifier".

The fast track treats the value as corrupted, the key has no recipe, and the read fails. A CSV
loads only when its metadata says `type_identifier: RecordView`, as Liquers writes it. In that
case it is deserialized by the schema-less reader before any command runs, and no context exists
to log a warning.

`specs/reference/RECORD_STREAMS.md` shows `-R/data/orders.csv/-/ns-rec/head-10` as a working
query. It works only for a CSV that Liquers wrote itself.

## Expected behaviour

When the identifier is `""` or `Bytes` and neither half knows the format, the file should load as
`Bytes`. Commands such as `ns-rec/to_record` already accept bytes and take the format from the
metadata.

## Discovery

Found 2026-10-07 while implementing `design/csv-physical-lines-short-rows/` Phase 3 T7. That test
was planned against a stored `data/short.csv` and now reads the CSV from a command instead
(`to_record_logs_padded_csv_rows`, `liquers-lib/tests/records_end_to_end.rs`).

## Resolution (2026-10-08)

Fixed by design `simple-value-untyped-and-scalar-reads` (commit 99af3b3).
`SimpleValue::deserialize_from_bytes` now returns `SimpleValue::Bytes` for identifier `""` or
`Bytes` in a format it does not parse (`csv`, `png`, `parquet`, …); any other identifier still
refuses, so `CombinedValue` keeps asking the extension. A hand-placed CSV loads as bytes and
`ns-rec/to_record` takes the format from the metadata. Core `Value`'s own `_` arm is unchanged
(out of scope: no core-only command consumes a CSV).

Evidence: `manifest_over_hand_placed_csv_files_materializes` (no longer ignored,
`liquers-lib/tests/records_manifest_over_csv_files.rs`); `untyped_unlisted_format_reads_as_bytes`
and `typed_unlisted_format_still_refuses` (`liquers-lib/src/value/simple.rs`).
