# Phase 2: Solution & Architecture — records-arrow-c-data-export

## Overview

Part A adds `liquers-records/src/cdata.rs`, compiled with the new feature `cdata`. It defines the
two [CDATA] structs as `#[repr(C)]` types, an exporter that builds a `+s` root array whose children
point at the batch's buffers, and a copying importer. Each exported node owns a clone of its
`Column` (`Arc`-shared buffers) in `private_data`, and its `release` callback drops it. Part B
wraps an export in a private liquers-py pyclass implementing the PyCapsule protocol and calls
`pyarrow.record_batch(obj)`. Written for question 1 (a).

Rejected: the `arrow-rs` or `polars-arrow` FFI (a dependency, or ties to polars); zero-copy import
(question 2); one top-level array per column (pyarrow builds a batch from one struct array).

## Known-Issue Preflight

| Issue | Status | Priority | Impact on this design | Fix first? | Blocking? | Action |
|---|---|---|---|---|---|---|
| `PY-RECORD-VALUES-CANNOT-CROSS-TO-PYTHON` | draft | P3 | Part B edits the arm it adds | Part B only | No (Part A) | land `py-record-values-to-python` before Part B |
| `RECORD-FIELD-TYPE-HAS-NO-DECIMAL`, `…-NO-UUID-TIME-OR-INTERVAL` | draft | P3 | new `Column` variants need a mapping row | No | No | second to land adds the row |
| `IPC-READER-CANNOT-READ-ANY-POLARS-STRING-COLUMN` | draft | P2 | the same `U` / `vu` subset limit applies to import | No | No | refuse with a named error (AC-5) |

## Interfaces

All in `liquers-records/src/cdata.rs`, `#[cfg(feature = "cdata")] pub mod cdata;` in `lib.rs`.
Field names and order are [CDATA] *Structure definitions*; the fields are private.

```rust
#[repr(C)]
pub struct ArrowSchema {
    format: *const c_char, name: *const c_char, metadata: *const c_char, flags: i64,
    n_children: i64, children: *mut *mut ArrowSchema, dictionary: *mut ArrowSchema,
    release: Option<unsafe extern "C" fn(*mut ArrowSchema)>, private_data: *mut c_void,
}
#[repr(C)]
pub struct ArrowArray {
    length: i64, null_count: i64, offset: i64, n_buffers: i64, n_children: i64,
    buffers: *mut *const c_void, children: *mut *mut ArrowArray, dictionary: *mut ArrowArray,
    release: Option<unsafe extern "C" fn(*mut ArrowArray)>, private_data: *mut c_void,
}
// Sound: [CDATA] lets `release` be called from any thread, so a struct we exported (Arc-shared,
// immutable buffers, boxed children), one imported via `from_raw`, and one a consumer filled
// after `empty()` may all be moved across threads and released there.
unsafe impl Send for ArrowSchema {}
unsafe impl Send for ArrowArray {}
impl Drop for ArrowSchema { /* calls release when it is Some */ }
impl Drop for ArrowArray  { /* calls release when it is Some */ }

// Both types: `empty()` (released, for a consumer to fill), `is_released()`, `as_mut_ptr()`, and
// `unsafe fn from_raw(ptr)` (Safety: valid and unreleased; moved out, source marked released).

/// Zero-copy: the buffers are the batch's own; the batch may be dropped first.
pub fn export_record_batch(batch: &RecordBatch) -> Result<(ArrowSchema, ArrowArray), Error>;
/// Copies the foreign buffers into new `Buffer`s and releases `array`. `schema` is only read.
pub fn import_record_batch(schema: &ArrowSchema, array: ArrowArray) -> Result<RecordBatch, Error>;
```

Private `SchemaPrivate` (format, name and metadata as owned C strings, children) and
`ArrayPrivate` (`_owner: Option<Column>`, the buffer-pointer array, children; children are
`Box::into_raw`). `release_schema` / `release_array` release each child whose `release` is still
set, free the boxes, drop the private struct and set `release` to `None` ([CDATA] *Release callback
semantics*). Mapping (`RECORD_STREAMS.md` §"Arrow interoperability"):

| `Column` | format | buffers (in order) | child |
|---|---|---|---|
| `Bool` | `b` | validity, values bitmap | — |
| `Int` / `UInt` / `Float` | `l` / `L` / `g` | validity, values | — |
| `Date` | `tdD` | validity, `i32` values | — |
| `Timestamp` | `tsu:UTC` (the IPC writer's zone) | validity, `i64` values | — |
| `Text` / `Binary` | `u` / `z` | validity, `i32` offsets (`len + 1`), data | — |
| `Vector` | `+w:<dim>` | validity | `f`, length `len × dim`, values `data` |
| batch root | `+s` | (null validity) | one per column |

Before taking any pointer the exporter re-checks the batch (its fields are `pub`; `Bitmap`
deserializes `bits` and `len` apart): column count, `Column::validate()` (offsets `len + 1` within
the data, `Vector` data `dim × len`, `Bool` storage), `column.len() == batch.len`, and
`has_storage_for_len` on every validity. A failure names the field (AC-10).

A missing validity is a null pointer with `null_count` 0; otherwise `null_count` is
`len - count_ones()`. `offset` is always 0 on export. `flags` is `ARROW_FLAG_NULLABLE` when
`FieldSchema::nullable`. Root metadata is [CDATA]'s binary key-value encoding of `liquers.schema`
(the `RecordSchema` as JSON) and, when present, `liquers.chunk_id`.

Import: a `+s` root; each child maps back by the table (`tsu:` with any zone reads as UTC epoch
microseconds, as the IPC reader does); anything else is refused. `n_buffers`, `length + offset` and
the `+w` child length are checked, then the data is copied: typed slices from `offset`, `Text` /
`Binary` bytes between `offsets[offset]` and `offsets[offset + length]` with rebased offsets,
bitmaps bit by bit when `offset % 8 != 0`. The schema is `liquers.schema` while it matches the
fields by count, name and type (`formats/ipc.rs` `check_embedded_schema`'s rule), else derived
(`FieldSchema::new`, `.not_null()` without the nullable flag). `RecordBatch::new` validates.

**Part B** (`liquers-py`): a direct dependency `liquers-records = { path = "../liquers-records",
features = ["cdata"] }`, and in `liquers-py/src/value.rs` a private `#[pyclass] struct ArrowExport
{ export: Mutex<Option<(ArrowSchema, ArrowArray)>> }`, not added to the module. PyO3 0.30
requires a pyclass to be `Sync` (free-threaded builds are supported), which the `Mutex` gives
because the pair is `Send`. `__arrow_c_array__(requested_schema)` takes the pair out and returns
capsules named `arrow_schema` / `arrow_array`
(`PyCapsule::new_with_destructor`); a second call raises `ValueError`; `requested_schema` is
ignored, as the protocol permits. The record-view arm runs `materialize()`, `export_record_batch`
and `pyarrow.record_batch(export)` when `pyarrow.__version__`'s major is 14 or more, then wraps the
batch in the type `py-record-values-to-python` returns (`pyarrow.Table.from_batches([batch])` for its
recommended `pyarrow.Table`; no copy), else the IPC route. All of it is synchronous and in memory.

## Integration Points

- `liquers-records/Cargo.toml`: feature `cdata = []` (no new dependency: `std::ffi` only).
- `liquers-records/src/lib.rs`: the gated module and re-exports.
- `liquers-records/src/buffer.rs`: a `#[cfg(test)] pub(crate) fn strong_count(&self) -> usize` on
  `AlignedBuffer`, for AC-3. Nothing else changes.
- `scripts/check-build-matrix.sh` `RECORDS_CONFIGS`: `--features cdata --tests` and
  `--target wasm32-unknown-unknown --features cdata`.
- Part B: `liquers-py/Cargo.toml`, `liquers-py/src/value.rs` (the record-view arm of `ext_to_py`),
  `scripts/test-liquers-py.sh` (installs `pyarrow>=14` unless the py-record design already does).

## Error Handling

- Export: a field name with an interior NUL, or a buffer inconsistent with `len` (AC-10) →
  `Error::general_error` naming the field.
- Import: a released input, a wrong root, a child count or buffer count mismatch, a short buffer or
  an unsupported format → `Error::not_supported` (format) or `Error::general_error` (shape), naming
  the field. The array is released on every path, because it is owned and dropped.
- Python: a missing or failing pyarrow → `py_conversion_error`, as the existing paths.

## Relevant Commands

None. No command, namespace or `specs/command_registry.yaml` change.

## Documentation Architecture

- `specs/reference/RECORD_STREAMS.md` (reference, records): "No `unsafe` outside `cdata`"; C Data
  route built, with the mapping and metadata keys; Limits drops the export.
- `specs/reference/PYTHON_INTEGRATION.md` (reference, py): views cross through C Data (pyarrow 14+).
- `specs/guides/LANGUAGE-INTEGRATION_GUIDE.md` §RECORDS (guide): Python as the worked case.
- `liquers-py/README.md`: the pyarrow minimum for the zero-copy route.
- Proposed `affects_docs`: the three under `specs/` (`liquers-py/README.md` is outside it). `specs/README.md`: link this design.

## Risks

| Assessment | Finding |
|---|---|
| Files likely to change | `liquers-records/src/{cdata.rs (new), lib.rs, buffer.rs}`, `liquers-records/Cargo.toml`, `scripts/check-build-matrix.sh`; Part B `liquers-py/src/value.rs`, `liquers-py/Cargo.toml`, `scripts/test-liquers-py.sh` |
| Crates and workflows affected | `liquers-records` (opt-in feature), `liquers-py`; nothing else builds `cdata` |
| Existing tests likely to change | none |
| New validation | Phase 3 tests, the two matrix rows; Miri where a nightly toolchain exists |
| Compatibility / data / performance | additive, off by default; Python output type unchanged. Export is O(columns) small allocations, import one copy |
| Security / soundness | a wrong release order, a double free or a short buffer is undefined behaviour: `Arc`-count and moved-child tests (AC-3), re-validation before export (AC-10). Import trusts the producer's lengths, as [CDATA] requires |
| Recovery | revert the module and feature; reverting Part B's arm restores IPC |
| Certainty | high; question 1 decides only the crate. `PyCapsule::new_with_destructor` (pyo3 0.30.1) and `pyarrow.record_batch` taking `__arrow_c_array__` (pyarrow 14) are checked at Step 6 |
