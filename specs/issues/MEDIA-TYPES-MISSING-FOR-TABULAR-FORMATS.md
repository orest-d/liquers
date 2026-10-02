---
id: MEDIA-TYPES-MISSING-FOR-TABULAR-FORMATS
kind: issue
title: The extension-to-media-type table lacks or mislabels the common tabular formats
status: closed
priority: P3
complexity: S
area: [core/value, axum]
design: axum-assets-endpoints
created: 2026-09-24
github:
---
# The extension-to-media-type table lacks or mislabels the common tabular formats

## Problem

`liquers-core/src/media_type.rs` (`file_extension_to_media_type`) falls back to
`application/octet-stream` for anything it does not list (`:138`). For the table formats:

| Extension | Today | Commonly used |
|---|---|---|
| `ndjson` | *(absent)* → `application/octet-stream` | `application/x-ndjson` |
| `jsonl` | `application/jsonlines` (`:54`) | `application/jsonl` or `application/x-ndjson` — `application/jsonlines` is not in common use |
| `arrow`, `feather` | `application/octet-stream` (`:7`, `:29`) | `application/vnd.apache.arrow.file` |
| `ipc` | *(absent)* | `application/vnd.apache.arrow.file` (or `.stream` for the stream format) |
| `parquet` | `application/octet-stream` (`:74`) | `application/vnd.apache.parquet` |

`csv` (`text/csv`), `tsv` (`text/tab-separated-values`), `md` (`text/markdown`) and `html` are
already right.

## Impact

An NDJSON or Arrow response from `liquers-axum` is labelled as opaque bytes, so a browser downloads
it instead of letting a client dispatch on the type, and an Arrow-aware client cannot recognize the
format from the header. Minor, but it lands the moment `record-streams` serves these formats.

## Expected behaviour

Add or correct the entries above. The Apache media types are to be confirmed against the IANA
registry when the entries are added, rather than taken from this record.

## Discovery

Found 2026-09-24 while specifying the table formats of `specs/design/record-streams/` Phase 2.

## Update 2026-09-27 — after `record-streams` was implemented

Still open. `record-streams` added no media-type entries: `liquers-core/src/media_type.rs` is
unchanged on that branch. `ndjson`, `ipc` and `parquet` are now formats a `RecordView` really
writes, so this is no longer hypothetical.

## Resolution

`arrow`, `feather`, `ipc` → `application/vnd.apache.arrow.file`; `parquet` →
`application/vnd.apache.parquet`; `jsonl` → `application/jsonl`; `ndjson` →
`application/x-ndjson` (the last two unregistered, as the code comment says). Evidence:
`media_type::media_type_tests` MT01–MT10. The IANA registry could not be fetched from the build
environment; the values follow the registry as known when written, recorded in
`design/axum-assets-endpoints/phase4-implementation.md`.

Fixed on branch `claude/fervent-cori-ew4kvn` (design `axum-assets-endpoints`, 2026-09-28).
