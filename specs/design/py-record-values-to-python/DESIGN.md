---
id: PY-RECORD-VALUES-TO-PYTHON
kind: design
title: A record view evaluated from Python arrives as a pyarrow Table
form: compact
status: in_review
phase: implementation
readiness: needs-decision
autofix: not-eligible
area: [py, records]
issues: [PY-RECORD-VALUES-CANNOT-CROSS-TO-PYTHON]
created: 2026-10-27
---
# A record view evaluated from Python arrives as a pyarrow Table

Bulk design (`specs/guides/autonomous_bulk_design.md`), 2026-10-27: Phases 1-4 written, not
approved, nothing implemented.

## Phase 1: High-Level Design

### Purpose

Every `ns-rec` command runs from Python, but its result cannot be returned: `value_to_py` refuses
`ExtValue::RecordView` and `ExtValue::RecordSource`. This design converts a record view to a
`pyarrow.Table` through Arrow IPC bytes, as DataFrames already cross through Parquet bytes, and
makes the refusal of an unmaterialized source say how to materialize it. It serves `VISION.md`'s
"Python data science" use case.

### Problem Example

Verified at HEAD (`04667a80`): `ext_to_py` (`liquers-py/src/value.rs`) handles `Foreign`,
`PolarsDataFrame` and `Image`, and refuses every other variant with "this value type is not
exposed to Python"; `liquers-py` links `liquers-lib` with `records`, `records-ipc` and
`records-parquet`.

```python
@env.first_command
def table() -> bytes:
    return b"id,amount\na,1\nb,2"

env.evaluate("table/ns-rec/to_record-csv")
# today:  ConversionError ("RecordView" ... "this value type is not exposed to Python")
# should: a pyarrow.Table with columns ["id", "amount"] and 2 rows
env.evaluate("table/ns-rec/to_record-csv/to_record_source")
# today and after: ConversionError; after, its message names ns-rec/materialize
env.evaluate("table/ns-rec/to_record-csv/to_record_source/materialize-10")   # a pyarrow.Table
```

All three queries validate against the committed registry (`--command table`).
`liquers_records::write_table(view, TableFormat::Ipc, &WriteOptions::default())` (re-exported as
`liquers_lib::records::write_table`) already writes an Arrow IPC file; polars reads its output
(`liquers-lib/tests/records_ipc_polars.rs`), pyarrow has not been tried.

### Scope and Acceptance Criteria

- **AC-1** A view crosses as a table
  WHEN a query whose result is a `RecordView` is evaluated from Python with pyarrow installed
  THEN the value is a `pyarrow.Table` whose `column_names` and `num_rows` match the view
- **AC-2** The rows are the view's rows
  WHEN the same view is also evaluated through `…/ns-rec/to_json`
  THEN `table.to_pylist()` equals those records
- **AC-3** A materialized source crosses
  WHEN `…/to_record_source/materialize-N` is evaluated
  THEN the value is a `pyarrow.Table` as in AC-1
- **AC-4** A source is refused with a remedy
  WHEN a `RecordSource` is evaluated from Python
  THEN it raises `lq.ConversionError` whose message names `ns-rec/materialize`
- **AC-5** Missing pyarrow is a conversion error
  WHEN pyarrow cannot be imported
  THEN a `RecordView` result raises `lq.ConversionError` naming pyarrow, like the polars and
  Pillow paths
- **AC-6** The state path agrees
  WHEN `evaluate_state` returns a record view
  THEN `state.value` is the same kind of table, and the metadata is unchanged

Non-goals: Python → records (`pyarrow.Table` as a command input); zero-copy hand-off
(`records-arrow-c-data-export`); a source as a Python iterator or a `liquers` record class.

### Design Readiness

- **Readiness:** needs-decision
- **Automatic fixing:** not-eligible — a new conversion in the Python binding: `evaluate` returns a
  `pyarrow.Table` where `PYTHON_INTEGRATION.md` §5 documents `ConversionError` (rules 2 and 4); the
  test loop gains a pyarrow install (rule 4, new dependency); readiness is not `ready` (rule 5).
- **Leading issue:** question 1.
- **Explanation:** the transport (IPC bytes, `pyarrow.ipc`) and the error shape follow the
  existing DataFrame and image paths; the choices left are which Python type a view becomes and
  what happens to a source.
- **Open questions:**
  1. **Open design - Python type of a view:** `pyarrow.Table` (recommended): what pandas, polars
     (`polars.from_arrow`) and DuckDB take most directly, and the type a materialized multi-batch
     source would also produce, so views and sources look alike. `pyarrow.RecordBatch` matches a
     view one to one but is the less common currency. `polars.DataFrame` (through
     `liquers-lib/src/records/polars.rs`) needs no new Python package but makes a record view
     indistinguishable from a DataFrame value. The choice is a contract that
     `records-arrow-c-data-export` should keep when it replaces the transport.
  2. **Proposed resolution - a source:** keep refusing, with a message naming
     `ns-rec/materialize` (AC-4). Materializing in `value_to_py` needs a `ChunkResolver` (an
     environment context) and an async wait that the conversion does not have; an iterator needs a
     new Python class.
  3. **Implementation detail - pyarrow import:** imported at conversion time and never a build
     dependency, like polars and Pillow.

### Design Dependencies

- `records-arrow-c-data-export` (designed in this run under that slug) — **overlaps**: the same
  hand-off without copies. Not merged: no overlap test holds (different mechanism, crates and
  change sites; it needs no part of this design), and it is `L`. It may later swap the IPC bytes
  behind `ext_to_py` for the C Data Interface while keeping question 1's type. No order.
- `py-api-reference` — **overlaps**: documents `evaluate`'s return types; no order.
- `liquers-web-records-bridge` (complete) — precedent: JavaScript wraps views in its own handle
  class; Python uses the ecosystem's type instead. E4.

### Consolidated Findings

- `write_table` with `TableFormat::Ipc` materializes the view and writes one batch; the cost is
  two copies (materialize, IPC) plus pyarrow's read, comparable to the DataFrame path.
- The writer embeds `liquers.schema` in the schema metadata; pyarrow keeps it as
  `table.schema.metadata`, harmless and useful for a later Python → records route.
- pyarrow interop of the hand-written IPC writer is untested; AC-1/AC-2 are its first check.
- `ext_to_py` uses `if let` per variant (feature unification); the new branches follow that.

## Phase 2: Architecture

### Solution

In `liquers-py/src/value.rs` `ext_to_py`, before the final refusal:

1. `ExtValue::RecordView { value: view }`: `let bytes = liquers_lib::records::write_table(view.as_ref(),
   TableFormat::Ipc, &WriteOptions::default())?;` then, under the GIL,
   `pyarrow.ipc.open_file(pyarrow.py_buffer(bytes)).read_all()`; a Python failure maps through
   `py_conversion_error("pyarrow.Table (needs pyarrow)", e)`.
2. `ExtValue::RecordSource { .. }`: `Err(Error::conversion_error_with_message("RecordSource",
   "Python", "materialize it in the query first: …/ns-rec/materialize"))`.

No new type, trait or public item; `value_to_py`'s signature is unchanged. Sync, under the GIL, as
the existing branches; the IPC write is CPU-bound in-memory work.

Rejected: a `liquers` record class wrapping the `Arc` (new structure and API, the JavaScript
shape); Parquet bytes (`pyarrow.parquet` is a heavier optional part of pyarrow, and IPC is the
format `records-ipc` already builds); polars (question 1).

### Changes

- `liquers-py/src/value.rs` `ext_to_py` (two branches, `use liquers_lib::records::{write_table,
  TableFormat, WriteOptions}`), its doc comment.
- `scripts/test-liquers-py.sh`: `pyarrow` added to the pip install line and the header comment.
- Documents: `specs/reference/PYTHON_INTEGRATION.md` §5 (Liquers to Python: records row, source
  refusal); `specs/guides/PYTHON_GUIDE.md` §7 Values (the table type, the materialize remedy);
  `liquers-py/README.md` "Building and testing" (pyarrow among the installed packages). History
  rows and `reviewed:` bumps.

### Risks

| Risk | Mitigation | Certainty |
|---|---|---|
| pyarrow rejects the hand-written IPC file (alignment, flatbuffer detail) | AC-1/AC-2 tests; fall back to `pyarrow.ipc.open_stream` or the polars route; file an issue against `liquers-records` | medium |
| Column types differ from expectations (text as `large_string`, ints width) | tests compare values (`to_pylist`), not Arrow type names | high |
| A large view is copied twice | documented; zero copy is `records-arrow-c-data-export` | high |
| Feature unification changes `ExtValue` | `if let`, no `match`, as today | high |

Recovery: revert `value.rs`; the refusal returns. Existing tests: none change.

## Phase 3: Examples and Tests

### Examples

The Problem Example is the primary case. Secondary: `env.evaluate_state(…)` on the same query
gives `state.value` a `pyarrow.Table`; with `sys.modules["pyarrow"] = None` the first query raises
`lq.ConversionError` mentioning pyarrow.

### Tests

In `liquers-py/tests/test_VALUE_value_bridge.py`, with a `table` first command as above:

- `test_VALUE08_record_view_crosses_as_a_pyarrow_table` — `pytest.importorskip("pyarrow")`;
  type, `column_names == ["id", "amount"]`, `num_rows == 2`, and `to_pylist()` equals
  `env.evaluate("table/ns-rec/to_record-csv/to_json")`: AC-1, AC-2
- `test_VALUE08_materialized_record_source_crosses_as_a_table` — `…/materialize-10`: AC-3
- `test_VALUE08_record_source_is_refused_naming_materialize` — no pyarrow needed: AC-4
- `test_VALUE08_record_view_without_pyarrow_is_a_conversion_error` — `monkeypatch.setitem(sys.modules,
  "pyarrow", None)`: AC-5
- `test_VALUE08_record_view_state_value_is_a_table` — `evaluate_state`, metadata type identifier
  `RecordView`: AC-6

Run: `bash scripts/test-liquers-py.sh -k VALUE08`.

## Phase 4: Implementation Plan

### Steps

- [ ] 1. `scripts/test-liquers-py.sh` — add `pyarrow` to the pip line — proof: the script prints the install and `python3 -c "import pyarrow"` succeeds in the venv
- [ ] 2. `liquers-py/tests/test_VALUE_value_bridge.py` — the five tests, failing — proof: `bash scripts/test-liquers-py.sh -k VALUE08` fails on them only
- [ ] 3. `liquers-py/src/value.rs` `ext_to_py` — the `RecordView` branch — proof: `bash scripts/test-liquers-py.sh -k "record_view or materialized"` (AC-1, AC-2, AC-3, AC-5, AC-6)
- [ ] 4. Same function — the `RecordSource` refusal message — proof: `bash scripts/test-liquers-py.sh -k record_source_is_refused` (AC-4)
- [ ] 5. Documents listed in Phase 2 §Changes, History rows, `reviewed:` — proof: `python3 scripts/docs_index.py --check`
- [ ] 6. Issue `status: closed` with a resolution note (remaining: Python → records, zero copy); `python3 scripts/docs_index.py` — proof: `python3 scripts/docs_index.py --check`

Rollback: revert `value.rs`; the tests then fail visibly rather than silently.

### Validation

`bash scripts/test-liquers-py.sh` (whole suite and the crate's Rust unit tests);
`bash scripts/check-build-matrix.sh` is not needed (no `cfg` change in `liquers-lib`);
`python3 scripts/docs_index.py --check`.
