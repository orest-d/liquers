---
id: PY-RECORD-VALUES-CANNOT-CROSS-TO-PYTHON
kind: feature
title: A record view or source evaluated from Python raises ConversionError instead of reaching Python as a table
status: draft
priority: P3
complexity: M
area: [py, records]
design: py-record-values-to-python
created: 2026-10-20
github:
---

## Problem

`liquers-py` links `liquers-lib` with `records`, `records-ipc` and `records-parquet`, so every
`ns-rec` command runs from Python, but `value_to_py` (`liquers-py/src/value.rs`) maps only
DataFrames (through `polars.read_parquet`) and images; `ExtValue::RecordView` and
`ExtValue::RecordSource` raise `ConversionError` with their type identifier
(`reference/PYTHON_INTEGRATION.md` §5).

```python
lq.evaluate("-R/data/table.csv/-/ns-rec/to_record")   # ConversionError
```

## Problem Example

```python
@env.first_command
def table() -> bytes:
    return b"id,amount\na,1\nb,2"

env.evaluate("table/ns-rec/to_record-csv")
# today:  ConversionError naming "RecordView": "this value type is not exposed to Python"
#         (ext_to_py, liquers-py/src/value.rs)
# should: a table Python can use (design/py-record-values-to-python/: a pyarrow.Table)
```

Re-checked at HEAD (2026-10-27, `04667a80`): unchanged.

## Impact

A Python user can run record commands but not see their result, except by converting to a polars
DataFrame inside the query first. There is no Python → records direction either.

## Expected behaviour

A record view crosses as a `pyarrow.RecordBatch` (or `pyarrow.Table`), a source as an iterator of
them or after `materialize` with its row limit. The first slice may copy, through Arrow IPC bytes
read with `pyarrow.ipc` (already built with `records-ipc`), raising `ConversionError` when pyarrow
is absent, like the polars and Pillow paths. The zero-copy route is
`RECORDS-ARROW-C-DATA-EXPORT-NOT-BUILT`, which this slice does not need and which can later replace
its transport without changing the Python-facing type.

## Discovery

Found 2026-10-20 during the bulk-design pass while assessing `RECORDS-ARROW-C-DATA-EXPORT-NOT-BUILT`,
whose "no `liquers-py` path at all" is in fact two gaps: no zero-copy export, and no conversion of
any kind.
