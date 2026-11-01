# Phase 1: High-Level Design — liquers-py-records

## Purpose

Let a Python program read the tables the `ns-rec` commands produce, walk a record stream a chunk
at a time, and leave Liquers for its own dataframe library when it wants to: the `RECORDS`
sub-feature of `VALUE` in `LANGUAGE-INTEGRATION_GUIDE.md` (§"RECORDS — the columnar record types"),
for `VISION.md`'s Python data science use case. Milestone M5 of the liquers-py plan, with
[`liquers-py-stubs`](../liquers-py-stubs/DESIGN.md) and [`liquers-py-package`](../liquers-py-package/DESIGN.md).

## Design Readiness

- **Readiness:** `needs-decision`
- **Automatic fixing:** `not-eligible`: an `L` feature with new public Python classes.
- **Leading issue:** question 1, which also decides the fate of the bulk design
  `py-record-values-to-python`.
- **Explanation:** the guide and the liquers-py plan (§3.2, §4) recommend wrappers with an Arrow
  exit; an unapproved bulk design for the same issue chose the opposite (a `pyarrow.Table` as the
  value). Only Orest can choose between them. The other questions have recommendations.
- **Open questions:**
  1. **Open design - what a record view becomes in Python.** Recommended: an `lq.RecordView`
     wrapper over the `Arc<dyn RecordView>`, reading columns in place, carrying the schema with its
     field roles, the chunk id and the origins, with `.to_arrow()`, `.to_pandas()` and
     `.to_polars()` as the exits (copying through Arrow IPC until
     `records-arrow-c-data-export` lands). This is the guide's "wrappers first, Arrow as a
     documented exit", and the only form that keeps roles and provenance. Alternative
     (`py-record-values-to-python`): the value *is* a `pyarrow.Table`, so pyarrow becomes
     necessary to read any record result and roles survive only as schema metadata. If 1 is
     taken as recommended, this design absorbs `PY-RECORD-VALUES-CANNOT-CROSS-TO-PYTHON` and that
     design becomes `superseded` (overlap T2 and T4; it is unapproved and has no PR, so E3 does not
     apply); its IPC transport and its materialize remedy carry over.
  2. **Proposed resolution - pyarrow is an extra.** Never a build or install dependency: imported
     when an Arrow exit is called, `ConversionError` naming the extra when absent (as polars and
     Pillow today); `liquers-py[arrow]` installs it (`liquers-py-package`).
  3. **Open design - Python data into records.** Recommended: a `lq.RecordView` returned by a
     command crosses back as the same `Arc` (no copy, RECORDS01), and a `pyarrow.Table` or
     `pyarrow.RecordBatch` crosses as a `RecordView` through IPC bytes (the plan's §3.2 row).
     `large_string`, `large_binary` and the view types, which `liquers-records`' IPC reader
     refuses, are cast to their 32-bit forms in Python first; a column that does not fit is a
     `ConversionError` naming it. Alternative: outbound only in M5.
  4. **Proposed resolution - what is exposed** (guide question 3). `RecordView`, `RecordSource`,
     `RecordColumn`, `RecordSchema`, `FieldSchema`, `FieldType`, `KeyRole`, `ChunkOrigin`; cells
     as Python scalars (`FieldValue` has no class); the view constructors `select_columns`,
     `slice`, `row`, `cell`. Hidden: the view structs, `Bitmap`, `Buffer<T>`, `AlignedBuffer`,
     the mutable table, resolvers and manifests' internals. Filtering stays in queries (`ns-rec`).
  5. **Proposed resolution - lent buffers.** A column of a fixed-width type (`Int`, `UInt`,
     `Float`, `Date`, `Timestamp`, `Vector` as rows by `dim`) exposes a read-only buffer
     (`memoryview(column)`), held alive by the column object, so the Rust allocation outlives every
     view of it (no `memory.grow` analogue on native: RECORDS05 holds trivially, and is tested).
     `Bool`, `Text` and `Binary` expose their parts (`offsets`, `data`, `validity`) as buffers
     too, and every column has `to_list()`, the always-correct copy.
  6. **Proposed resolution - traversal** (guide questions 4 and 5). `for view in source` takes
     route 1, evaluating one chunk at a time through the source's environment, and falls back to
     route 2 (blocking on the stream, GIL released) for an unbounded list or chunks that name no
     query (an in-memory source). `async for view in source` drives `stream()`. `chunks()` returns
     the ids (`lq.Query` / `lq.Key`) without evaluating; `materialize(max_rows)` and
     `materialize_async(max_rows)` are bounded. Python does not implement the traits in M5:
     `RECORDS11` is `NA` (reverses if Python-defined views are selected), `RECORDS09` is `NA`
     (Python has an async model).
  7. **Proposed resolution - a single cell.** A one-row, one-column view reaches Python as its
     scalar, as in liquers-web (RECORDS10); `view.item()` on any other view raises
     `ConversionError` naming its shape.
  8. **Proposed resolution - Arrow metadata** (guide question 2). Roles and the chunk id already
     ride in IPC metadata (`liquers.schema`, `liquers.chunk_id`); origins do not, so the IPC
     writer and reader gain a `liquers.origins` key (additive, `liquers-records`), and RECORDS03
     asserts all three survive `.to_arrow()` and an inbound round trip.

## Problem Example

```python
import liquers as lq
env = lq.Environment()

@env.first_command
def table() -> bytes:
    return b"id,amount\na,1\nb,2"

view = env.evaluate("table/ns-rec/to_record-csv")
source = env.evaluate("table/ns-rec/to_record-csv/to_record_source")
```

**Today** both raise `ConversionError` ("this value type is not exposed to Python",
`ext_to_py` in `liquers-py/src/value.rs`), so a record command's result can be computed but never
read. **After** `view` is an `lq.RecordView` (`len(view) == 2`, `view.schema.names == ["id",
"amount"]`, `view.column("amount").to_list() == [1, 2]`, `view.to_arrow()` a `pyarrow.Table`), and
`source` is an `lq.RecordSource` whose `for` and `async for` yield `lq.RecordView`s. All queries
here validate (`liquers-validate --command table`).

## Scope and Acceptance Criteria

- **AC-1** A view reaches Python
  WHEN the problem example's `view` query is evaluated
  THEN it is an `lq.RecordView` with the CSV's rows, column names and types
- **AC-2** A view round-trips with its identity
  WHEN a Python command returns an `lq.RecordView` it received, and the next command reads its schema
  THEN the same view arrives, roles, chunk id and origins intact (RECORDS01)
- **AC-3** Columns read the same in place and copied
  WHEN each column of a batch with every field type and some nulls is read through its buffer and through `to_list()`
  THEN both give the same values, and the buffer is read-only (RECORDS02, RECORDS04)
- **AC-4** Lent buffers stay valid
  WHEN a buffer is held while the view is dropped and many large allocations follow
  THEN it still reads the original values (RECORDS05)
- **AC-5** Wrappers are released
  WHEN every view, column and source object is dropped, or `shutdown()` is called
  THEN the live record-handle count (`debug-handles`) returns to its baseline (RECORDS06)
- **AC-6** A stored stream is walked one chunk at a time
  WHEN a manifest-backed source of three CSV chunks is iterated with `for`
  THEN three views arrive in order and only one chunk is evaluated per step (RECORDS07)
- **AC-7** The async walk agrees
  WHEN the same source is iterated with `async for`
  THEN it yields the same rows (RECORDS08)
- **AC-8** Materializing is bounded
  WHEN `source.materialize(max_rows)` or `materialize_async` is called below and above the row count
  THEN the first returns one view of every row and the second raises an error naming the limit
- **AC-9** Arrow is an exit, with metadata
  WHEN `.to_arrow()`, `.to_pandas()` and `.to_polars()` are called on a view with an `Id` field and origins
  THEN each holds the view's rows, the Arrow schema carries roles, chunk id and origins, and with pyarrow absent the call raises `ConversionError` naming `liquers-py[arrow]` (RECORDS03)
- **AC-10** A single cell is a scalar
  WHEN a query ends in a one-cell view, and `item()` is called on a larger one
  THEN the first is a Python scalar and the second raises `ConversionError` naming the shape (RECORDS10)
- **AC-11** pyarrow data enters records
  WHEN a Python command returns a `pyarrow.Table` (with `large_string` columns) to `ns-rec/to_json`
  THEN the records arrive with the table's rows (question 3)

Non-goals: zero-copy Arrow (`records-arrow-c-data-export`); Python-implemented views or sources
(`RECORDS11`); a Python filter predicate; the mutable table; writing manifests from Python.

## Core Interactions

Value types: `ext_to_py` / `py_to_value` gain the record variants. Records: wrappers over
`RecordView`, `RecordSource`, `RecordStream` with `EnvResolver`; IPC writer and reader carry
origins. EVAL: route 1 evaluates chunk queries. Bindings: the classes of question 4.

## Crate Placement

`liquers-py` (a new `src/records.rs`, as the plan's §3.6 named it); the additive `liquers.origins`
key in `liquers-records/src/formats/ipc.rs`. No `liquers-lib` change.

## Documentation Intent

- Reference: a §Records in `PYTHON_INTEGRATION.md` (the classes, conversion rows in both
  directions, buffers and lifetimes, traversal routes, threading, the copies); §5's records row
  replaced; `RECORD_STREAMS.md` (the `liquers.origins` key).
- Guide: a "Records" chapter in `PYTHON_GUIDE.md` (reading a table, columns as buffers and lists,
  walking a stream, leaving for pyarrow, pandas or polars). API: docstrings on every new class.
- Other: `liquers-py/README.md` (a `VALUE`/RECORDS row with the two `NA`s);
  `LANGUAGE-INTEGRATION_GUIDE.md` §RECORDS (what liquers-py does, beside liquers-web) and §6 Python.
- Open problems documented: every Arrow exit copies; the IPC reader's refused types; Python
  cannot define a view; the unbounded-list fallback blocks a thread.

## Design Dependencies

- `PY-RECORD-SOURCES-CANNOT-BE-TRAVERSED-FROM-PYTHON` — **implemented** by this design.
- `PY-RECORD-VALUES-CANNOT-CROSS-TO-PYTHON` / `py-record-values-to-python` — **absorbed** if
  question 1 is taken as recommended, otherwise this design narrows to everything but the value type.
- `records-arrow-c-data-export` — **required-by**: its Part B replaces the copy behind the Arrow
  exits and must re-target `.to_arrow()` instead of `ext_to_py`.
- `IPC-READER-CANNOT-READ-ANY-POLARS-STRING-COLUMN` — **overlaps** (question 3's casts sidestep it
  for pyarrow; not fixed here).
- `liquers-py-stubs` — **required-by**: declares these classes. `liquers-py-package` —
  **required-by**: the `[arrow]` extra.

## References

- `specs/guides/LANGUAGE-INTEGRATION_GUIDE.md` §"A third category", §RECORDS; `liquers-web/src/records.rs`
- `specs/reference/RECORD_STREAMS.md`; `liquers-records/src/batch.rs`, `value.rs`, `sources.rs`
