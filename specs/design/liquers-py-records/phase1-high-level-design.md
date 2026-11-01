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
  value). Only Orest can choose between them. Questions 3 and 8 change a scope or a crate format
  and are his to confirm; the others have recommendations.
- **Open questions:**
  1. **Open design - what a record view becomes in Python.** Recommended: an `lq.RecordView`
     wrapper over the `Arc<dyn RecordView>`, carrying the schema with its field roles, the chunk
     id and the origins, with `.to_arrow()` and `.to_pandas()` (the plan's exits) and
     `.to_polars()` (an addition: it reads the IPC bytes with `polars.read_ipc`, so it needs only
     polars), all copying until `records-arrow-c-data-export` lands. This is the guide's "wrappers
     first, Arrow as a documented exit", and the only form that keeps roles and provenance.
     Alternative (`py-record-values-to-python`): the value *is* a `pyarrow.Table`, so pyarrow is
     needed to read any record result, a source is refused, and roles survive only as metadata.
     Overlap: T2 decides (both answer "what does a record result become in Python"; T4 does not
     hold cleanly, because that design's AC-4 and AC-5 contradict this one); E1 (neither is
     eligible), E2 (`L` + `M` stays `L`), E3 (no PR), E4 and E5 do not exclude. If 1 is taken as
     recommended, this design absorbs `PY-RECORD-VALUES-CANNOT-CROSS-TO-PYTHON` (`merged:`), that
     design becomes `superseded` with `superseded_by: liquers-py-records`, its IPC transport is
     reused behind the exits, and its AC-6 (a state reports `RecordView`) becomes part of AC-1.
  2. **Proposed resolution - pyarrow is an extra.** Never a build or install dependency: imported
     when an exit needs it, `ConversionError` naming the extra when absent (as polars and Pillow
     today); `liquers-py[arrow]` installs it (`liquers-py-package`). PyPy has no pyarrow wheel, so
     those tests skip there.
  3. **Open design - Python data into records.** Recommended: a `lq.RecordView` returned by a
     command crosses back as the same `Arc` (no copy, RECORDS01), and a `pyarrow.Table` or
     `pyarrow.RecordBatch` crosses as a `RecordView` through IPC bytes (the plan's §3.2 row). What
     `liquers-records`' IPC reader refuses is normalized in Python first, when no value is lost:
     `large_*` and view types to 32-bit offsets, narrower integers to 64-bit, `float32` to
     `float64`, dictionaries decoded, `date64` to `date32`, timestamps to microseconds; anything
     else (an overflow, a lossy timestamp, nesting) is a `ConversionError` naming the column.
     Alternative: outbound only in M5.
  4. **Proposed resolution - what is exposed** (guide question 3). `RecordView`, `RecordSource`,
     `RecordColumn`, `RecordSchema`, `FieldSchema`, `FieldType`, `KeyRole`, `ChunkOrigin`; cells
     as Python scalars (`FieldValue` has no class); the view constructors `select_columns`,
     `slice`, `row`, `cell`. Hidden: the view structs, `Bitmap`, `Buffer<T>`, `AlignedBuffer`,
     the mutable table, resolvers and manifests' internals. Filtering stays in queries (`ns-rec`).
  5. **Proposed resolution - lent buffers.** A column of a `RecordBatch` is taken from the batch
     itself (`as_batch`), sharing its `Arc`ed buffers; any other view's column is read once
     (`column`, which copies) and then lent the same way. Fixed-width columns (`Int`, `UInt`,
     `Float`, `Date`, `Timestamp`, `Vector` as rows of `dim`) expose a read-only buffer
     (`memoryview(column)`); `Bool`, `Text` and `Binary` expose their parts (`offsets`, `data`);
     every column exposes `validity`, and `to_list()` is the always-correct copy. A memoryview
     holds its column object, so the allocation outlives every view of it (no `memory.grow`
     analogue on native: RECORDS05 holds, and is tested). The column class is immutable, so it is
     safe on free-threaded builds. The limited API has the buffer protocol only from 3.11, which
     sets `liquers-py-package`'s `abi3` floor.
  6. **Proposed resolution - traversal** (guide questions 4 and 5). `for view in source` pulls the
     source's own stream one chunk at a time (route 2, waiting with the interpreter lock
     released, safe inside a command), and `async for` awaits the same stream. This departs from
     the guide's preferred route 1 on purpose: evaluating each chunk id directly skips what the
     stream applies (a manifest's declared schema, a mapped source's projection, the row
     identity), and the stream already keeps one chunk resident. Route 1 stays available:
     `chunks()` lists the ids without evaluating (an in-memory source's placeholder ids are marked
     as not evaluable) and `source.chunk(id)` fetches one through the stream's rules, for resuming
     or out-of-order reads. `materialize(max_rows)` / `materialize_async(max_rows)` are bounded. A
     source resolves chunks in the environment that produced it (through a context when it is a
     command's argument, so the dependencies are recorded) and refuses after `shutdown()`.
     `RECORDS09` is `NA`: Python has an async model (reverses with a Pyodide target, where
     waiting is impossible, as `RUNTIME02`). `RECORDS11` is `NA`: Python does not implement the
     traits in M5 (reverses if Python-defined views are selected).
  7. **Proposed resolution - a single cell.** A one-row, one-column view reaches Python as its
     scalar, as in liquers-web (RECORDS10), so the result type depends on the data; this is
     documented, and `state.record_view()` returns the view whatever its shape.
     `view.item()` on any other view raises `ConversionError` naming its shape.
  8. **Open design - Arrow metadata** (guide question 2). Roles ride in `liquers.schema`; the chunk
     id rides in the batch message (`liquers.chunk_id`), which pyarrow's `read_all()` drops, and
     origins do not ride at all. Recommended: the IPC writer and reader gain a per-batch
     `liquers.origins` key beside `liquers.chunk_id` (re-based by `RecordBatch::concat` like the
     `Source` column, dropped when a stale `liquers.schema` is ignored), an additive change to
     `liquers-records`' format, and `.to_arrow()` copies roles, chunk id and origins into the
     table's schema metadata. Only `.to_arrow()` promises metadata; pandas and polars drop it.
     No producer in the tree fills origins yet (`RECORD_STREAMS.md` §ChunkOrigin), so the tests
     build their fixture from IPC bytes carrying them. Alternative: assert their loss.

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
  WHEN the problem example's `view` query is evaluated, directly and through `evaluate_state`
  THEN it is an `lq.RecordView` with the CSV's rows, column names and types, and the state's metadata reports `RecordView`
- **AC-2** A view round-trips with its identity
  WHEN a Python command returns an `lq.RecordView` it received (one with an `Id` field, a chunk id and origins), and the next command reads it
  THEN the same view arrives, roles, chunk id and origins intact (RECORDS01)
- **AC-3** Columns read the same lent and copied
  WHEN each column of a batch with every field type and some nulls is read through its buffers (values and `validity`) and through `to_list()`
  THEN both give the same values, and every buffer is read-only (RECORDS02, RECORDS04)
- **AC-4** Lent buffers stay valid
  WHEN a buffer is held while the view is dropped and many large allocations follow
  THEN it still reads the original values (RECORDS05)
- **AC-5** Wrappers are released
  WHEN every view, column and source object is dropped
  THEN a test-only live record-handle count (`debug-handles`) returns to its baseline (RECORDS06)
- **AC-6** A stored stream is walked one chunk at a time
  WHEN a manifest-backed source of three CSV chunks is iterated with `for`, the chunks read by a counting store
  THEN three views arrive in order with the manifest's schema, and each step has read one more chunk (RECORDS07)
- **AC-7** The async walk agrees
  WHEN the same source is iterated with `async for`
  THEN it yields the same rows (RECORDS08)
- **AC-8** Materializing is bounded
  WHEN `source.materialize(max_rows)` or `materialize_async` is called below and above the row count
  THEN the first returns one view of every row and the second raises an error naming the limit
- **AC-9** Arrow is an exit, with metadata
  WHEN `.to_arrow()`, `.to_pandas()` and `.to_polars()` are called on the AC-2 view
  THEN each holds the view's rows; the Arrow table's schema metadata carries roles, chunk id and origins; with pyarrow absent the first two raise `ConversionError` naming `liquers-py[arrow]` (RECORDS03)
- **AC-10** A single cell is a scalar
  WHEN a query ends in a one-row, one-column view
  THEN `evaluate` returns the Python scalar and `state.record_view()` the view (RECORDS10)
- **AC-11** pyarrow data enters records
  WHEN a Python command returns a `pyarrow.Table` with `large_string`, `int32`, dictionary and `timestamp[ns]` columns to `ns-rec/to_json`
  THEN the records arrive with the table's values, and a column that cannot be normalized raises `ConversionError` naming it (question 3)
- **AC-12** A larger view is not a scalar
  WHEN `item()` is called on a two-row view
  THEN it raises `ConversionError` naming the shape (RECORDS10)
- **AC-13** Chunks are listed without evaluation
  WHEN `chunks()` is called on the manifest source and on an in-memory source, then `source.chunk(id)` on the second manifest chunk
  THEN the manifest's ids arrive with no chunk read, the in-memory ids are marked not evaluable, and `chunk` returns that chunk's view as the stream gives it
- **AC-14** Views are narrowed without leaving Liquers
  WHEN `select_columns`, `slice`, `row` and `cell` are applied to a view
  THEN each returns the narrowed view (or cell value) the matching `ns-rec` command gives
- **AC-15** Shutdown ends traversal
  WHEN `shutdown()` is called while a source is held, and it is then iterated
  THEN iteration raises `lq.GeneralError`, while a held view's columns still read

Non-goals: zero-copy Arrow (`records-arrow-c-data-export`); Python-implemented views or sources
(`RECORDS11`); a Python filter predicate; the mutable table; writing manifests from Python.

## Core Interactions

Value types: `ext_to_py` / `py_to_value` gain the record variants, and the conversion learns which
environment (or context) it runs for. Records: wrappers over `RecordView`, `RecordSource` and their
streams with `EnvResolver` / `ContextResolver`; IPC writer and reader carry origins. Bindings: the
classes of question 4.

## Crate Placement

`liquers-py` (a new `src/records.rs`, as the plan's §3.6 named it); the additive `liquers.origins`
key in `liquers-records/src/formats/ipc.rs`. No `liquers-lib` change.

## Documentation Intent

- Reference: a §Records in `PYTHON_INTEGRATION.md` (the classes, conversion rows in both
  directions, buffers and lifetimes, traversal routes and why route 2 leads, threading, the
  copies); `RECORD_STREAMS.md` (the `liquers.origins` key).
- Guide: a "Records" chapter in `PYTHON_GUIDE.md` (reading a table, columns as buffers and lists,
  walking a stream, leaving for pyarrow, pandas or polars). API: docstrings on every new class, and
  the module doc of `liquers-records/src/formats/ipc.rs`.
- Other: `liquers-py/README.md` (a `VALUE`/RECORDS row with the two `NA`s);
  `LANGUAGE-INTEGRATION_GUIDE.md` §RECORDS (what liquers-py does, beside liquers-web; the route
  choice) and §6 Python.
- Documents to update: `PYTHON_INTEGRATION.md` §5 (the records refusal); the values table of
  `PYTHON_GUIDE.md` §7; `RECORD_STREAMS.md` §"Arrow IPC" ("`sources` are not written") and
  §ChunkOrigin; on absorption, `py-record-values-to-python`, its issue, and
  `records-arrow-c-data-export` (below); `specs/README.md`.
- Open problems documented: every Arrow exit copies; the IPC reader's refused types; Python cannot
  define a view; a one-cell result changes type; no producer fills origins.

## Design Dependencies

- `PY-RECORD-SOURCES-CANNOT-BE-TRAVERSED-FROM-PYTHON` — **implemented** by this design.
- `PY-RECORD-VALUES-CANNOT-CROSS-TO-PYTHON` / `py-record-values-to-python` — **absorbed** if
  question 1 is taken as recommended (otherwise this design narrows to everything but the value type).
- `records-arrow-c-data-export` — **required-by**: its Part B replaces the copy behind the exits;
  its question 3 and AC-1 (origins not carried) must then carry them, and its AC-7 and AC-8 must
  target `.to_arrow()` instead of `py-record-values-to-python`'s value.
- `ipc-reader-polars-text-columns` — **overlaps**: once it lands, part of question 3's
  normalization becomes unnecessary.
- `record-decimal-field-type`, `record-uuid-time-duration-types` — **overlap**: new `Column`
  variants extend question 5's buffer rules. `column-slice-shared-buffers`,
  `record-batch-private-fields` — **overlap**: they change how a batch's columns are reached.
- `liquers-py-stubs` — **required-by**: declares these classes. `liquers-py-package` —
  **required-by**: the `[arrow]` extra; it must keep the `abi3` floor at 3.11 or above (question 5).
- `py-api-reference` — **overlaps**: documents the new classes. `liquers-web-records-bridge`
  (complete) — precedent, E4.

## References

- `specs/guides/LANGUAGE-INTEGRATION_GUIDE.md` §"A third category", §RECORDS; `liquers-web/src/records.rs`
- `specs/reference/RECORD_STREAMS.md`; `liquers-records/src/batch.rs`, `value.rs`, `sources.rs`
