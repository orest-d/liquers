---
id: PY-RECORD-SOURCES-CANNOT-BE-TRAVERSED-FROM-PYTHON
kind: feature
title: A record source cannot be walked chunk by chunk from Python, and record columns cannot be read in place
status: draft
priority: P2
complexity: L
area: [py, records]
design: liquers-py-records
created: 2026-11-01
github:
---
## Problem

liquers-py has no Python class for any record type. `ext_to_py` (`liquers-py/src/value.rs`)
refuses `ExtValue::RecordView` and `ExtValue::RecordSource` with `ConversionError`, so nothing in
Python can list a source's chunks, walk a stored stream one chunk at a time (`for` or
`async for`), materialize it with a row bound, or read a column's values in place through the
buffer protocol. These are the guide's `RECORDS07`, `RECORDS08`, `RECORDS02`/`04`-`06`
(`LANGUAGE-INTEGRATION_GUIDE.md` §"RECORDS", §"Traversing a record stream").

## Impact

A Python user can run `ns-rec` commands but cannot consume a record stream except by
materializing it into a polars DataFrame inside the query, which defeats the chunked design for
anything large. The `VALUE`/RECORDS sub-feature selected for liquers-py (plan §4) cannot be
claimed.

## Expected behaviour

`lq.RecordView`, `lq.RecordSource` and `lq.RecordColumn` wrappers with chunk iteration, an async
iterator, a bounded materialize, read-only column buffers and an Arrow exit. Design:
`specs/design/liquers-py-records/`.

## Discovery

Filed 2026-11-01 when the M5 design folder was created. Overlap triage: the only strong
candidate is `PY-RECORD-VALUES-CANNOT-CROSS-TO-PYTHON` (design `py-record-values-to-python`),
decided by T2 (both answer what a record result becomes in Python; T4 fails because that design
refuses a source and requires pyarrow). No exclusion applies. Filed separately for now because
the shared contract is an open owner decision (`liquers-py-records` Phase 1 question 1); if it is
decided for wrappers, that issue is merged into `liquers-py-records` and its design superseded.
Runner-up: `RECORDS-ARROW-C-DATA-EXPORT-NOT-BUILT` (weak: zero-copy transport only).
