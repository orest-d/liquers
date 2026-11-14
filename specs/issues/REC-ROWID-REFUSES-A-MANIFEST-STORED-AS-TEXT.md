---
id: REC-ROWID-REFUSES-A-MANIFEST-STORED-AS-TEXT
kind: issue
title: ns-rec/rowid on a manifest key fails with unknown data format 'txt' where the other record commands read it
status: draft
priority: P3
complexity: S
area: [records]
design: 
created: 2026-11-14
github:
---
## Problem

A manifest written to the store as text carries the data format `txt` in its metadata (Python
`env.store_set("sales/monthly.manifest.yaml", text)` does this). The record commands read it as
a record source all the same, with one exception:

```text
-R/sales/monthly.manifest.yaml/-/ns-rec/nrows                          -> 2
-R/sales/monthly.manifest.yaml/-/ns-rec/rowid-0-1                      -> error
-R/sales/monthly.manifest.yaml/-/ns-rec/to_record_source/rowid-0-1     -> the row
```

The error is `TableFormat::from_data_format: unknown data format 'txt'`: `rowid` takes the
input for a table to parse instead of recognising the manifest, which the other commands do
through the shared input conversion in `liquers-lib/src/records/commands.rs`.

## Impact

Small: `rowid` is the documented way to address a row of a table without an `Id` field
(`docs/guides/_shared/records.md` §13.2), and on a manifest it needs an explicit
`to_record_source` first, which nothing says.

## Expected behaviour

`rowid` accepts what `nrows`, `head` and the filters accept, a manifest included.

## Discovery

Writing chapter 13 (*Records*) of the user guides, 2026-11-14. Whether `rec_id` has the same
gap was not checked.
