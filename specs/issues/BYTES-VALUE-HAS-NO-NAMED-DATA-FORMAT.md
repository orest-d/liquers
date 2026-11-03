---
id: BYTES-VALUE-HAS-NO-NAMED-DATA-FORMAT
kind: issue
title: A bytes value cannot be serialized in a named data format such as csv, so raw CSV bytes cannot be a record chunk
status: draft
priority: P3
complexity: S
area: [core/value, records]
created: 2026-11-01
github:
---

## Problem

`DefaultValueSerializer::as_bytes` for `Value` (`liquers-core/src/value.rs`) writes a
`Value::Bytes` only for the formats `bytes` / `b` / `bin` and the text formats (`txt`, `html`,
`rs`, `py`, `css`, `js`). Any other format, `csv` among them, fails with "Unsupported format csv".

A command that returns raw bytes already in a format, such as CSV text it built itself, therefore
cannot declare that format. Without a filename its data format is `bin`; with one (`part.csv`, or a
query ending in `/part.csv`) serialization fails.

Found in M5 (`liquers-py-records`, Step 7). A manifest chunk is read by `classify_state`
(`liquers-records/src/sources.rs`): a value that is not a view or a source is serialized in its own
data format and parsed as a table. So a Python command `def part() -> bytes: return b"id,n\n..."`
used as a chunk fails either way:
- `TableFormat::from_data_format: unknown data format 'bin'`;
- `Unsupported format csv`.

The tests return `RecordView`s instead.

## Impact

Chunks must be views, or stored resources whose key carries the extension. Commands written in
any language that produce CSV, NDJSON or another table format as bytes cannot feed a manifest
directly. The same refusal applies when such a value is stored under a key with that extension.

## Possible direction

Treat `Value::Bytes` as already serialized: write it unchanged in any data format, the way the
text arm already does. A narrower option is to do this only for formats the bytes cannot be
converted into (any non-JSON format). Either way, decide what `json` means for bytes, which
currently writes a JSON array of numbers.
