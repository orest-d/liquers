# Phase 1: High-Level Design — records-arrow-c-data-export

## Purpose

`liquers-records` lays its columns out in Arrow's buffer format so that a batch can be handed to
pyarrow, pandas, polars or DuckDB by pointer, through the Arrow C Data Interface. The export was
designed in `record-streams` and never built, so every route out of Liquers copies the data. This
design builds the C Data export and import for `RecordBatch`, and makes it the route a record view
takes into Python, as the pyarrow type `py-record-values-to-python` chooses (recommended there:
`pyarrow.Table`). It serves the data-science audience and the
"Arrow-compatible substrate" point in `reference/VISION.md`.

## Problem Example

Verified at HEAD (04667a8). A Rust caller has a 10-million-row `RecordBatch` and wants it in
pyarrow. `liquers-records` offers `write_table(.., TableFormat::Ipc, ..)` (feature `ipc`), which
serializes every buffer into a byte vector; pyarrow then reads and copies the bytes again. There is
no `ArrowArray` / `ArrowSchema` anywhere in the workspace (`grep -rn ArrowArray --include=*.rs`
finds nothing). From Python, `lq.evaluate("-R/data/table.csv/-/ns-rec/to_record")` raises
`ConversionError` today; `py-record-values-to-python` is designed to return a `pyarrow.Table` by
way of IPC bytes, which is the same double copy.

Wanted: `liquers_records::cdata::export_record_batch(&batch)` returns an `(ArrowSchema, ArrowArray)`
pair whose buffer pointers point at the batch's own buffers. The batch's memory stays alive until
the consumer calls `release`. liquers-py hands that pair to `pyarrow.record_batch` through the
PyCapsule protocol and wraps the batch in that design's type, so the Python result above carries no
copy of the column data.

## Scope and Acceptance Criteria

- **AC-1** Round trip
  WHEN a batch with one column of every `Column` variant, with and without nulls, is exported and
  then imported with `import_record_batch`
  THEN its schema (field roles included), columns, length and `chunk_id` equal the original's;
  `rows` is the default single run and `sources` is empty, as after an IPC round trip
- **AC-2** No data copied
  WHEN a batch is exported
  THEN every exported data, offsets and validity buffer pointer is the address of the batch's own
  buffer (`as_bytes().as_ptr()`)
- **AC-3** Lifetime
  WHEN the batch is dropped while its export is held, and the export is later released
  THEN the exported buffers stay readable until `release`, and after it the batch's storage is freed;
  a child moved out by the consumer before the parent is released stays valid until its own release
- **AC-4** Arrow types
  WHEN each `Column` variant is exported
  THEN its format string is the one in the Phase 2 mapping table (`tsu:UTC` for `Timestamp`, a
  `+w:<dim>` node with an `f` child for `Vector`, a `+s` root for the batch)
- **AC-5** Unsupported imports are refused
  WHEN an imported array has a type outside the subset (for example `i`, `U`, `vu`, a dictionary)
  THEN the import fails with an error naming the field and its format, and the foreign array is
  released
- **AC-6** Foreign offsets
  WHEN an imported array or child has a non-zero `offset`
  THEN the imported column holds exactly the rows from that offset on
- **AC-7** Python receives the table without IPC
  WHEN a record view is evaluated from Python with pyarrow 14 or newer
  THEN the result has the type `py-record-values-to-python` defines and equals what its IPC route
  produces, and its schema metadata carries `liquers.schema`
- **AC-8** Older pyarrow
  WHEN pyarrow is older than 14
  THEN the IPC route of `py-record-values-to-python` is used and the result is the same
- **AC-9** Feature-gated
  WHEN `liquers-records` is built without the `cdata` feature
  THEN no `cdata` item exists and the crate has no `unsafe` code of its own; with it, native and
  wasm32 builds pass
- **AC-10** An inconsistent batch is refused
  WHEN a batch whose public fields were changed after validation (a buffer shorter than `len`, a
  validity bitmap without storage for its bits, offsets past the data) is exported
  THEN the export fails with an error naming the field, and no pointer is handed out

Non-goals: zero-copy *import* (import copies, question 2); the stream interface
(`ArrowArrayStream`, `__arrow_c_stream__`) for a `RecordSource`; Python → records conversion; a
C Data route for the polars bridge (`liquers-lib/src/records/polars.rs`); Arrow types beyond the
documented subset.

## Core Interactions

Value types (`RecordBatch`, `Column`, `Buffer`, `Bitmap` in `liquers-records`); bindings (the
record-view arm of `liquers-py/src/value.rs`). No query, store, command or asset change.

## Crate Placement

Part A: `liquers-records`, behind a new feature `cdata`, because the export reads the buffers'
crate-private storage and must be usable by any crate above it. Part B: `liquers-py`, which turns
the pair into PyCapsules. The dependency flow is unchanged.

## Documentation Intent

- Reference: extend `specs/reference/RECORD_STREAMS.md` (crates table, Arrow interoperability,
  Limits) and `specs/reference/PYTHON_INTEGRATION.md` (record value transport).
- Guide: extend `specs/guides/LANGUAGE-INTEGRATION_GUIDE.md` §RECORDS with the built route.
- Other documents: `liquers-py/README.md` (pyarrow minimum for the zero-copy route).
- Documents to update: the source issue (closed at Phase 5 by the implementer).

## Design Readiness

- **Readiness:** needs-decision
- **Automatic fixing:** not-eligible — size `L` (rule 1); it also adds `pub` items, a feature flag
  and the crate's first `unsafe` code (rules 3 and 4).
- **Leading issue:** **Open design - where the `unsafe` lives.** `RECORD_STREAMS.md` states
  "`liquers-records` … **No `unsafe`.**" and C Data needs raw pointers and `extern "C"` release
  callbacks. Recommended (a): one module, `liquers-records/src/cdata.rs`, behind feature `cdata`,
  the documented single exception. (b) `liquers-py`, the placement `record-streams` Phase 2 named:
  records stays unsafe-free, but the export is Python-only, needs `RecordBatch`'s fields (which
  `record-batch-private-fields` proposes to make private) and a polars or DuckDB hand-off would need
  a second implementation. (c) A new crate `liquers-records-ffi`: one more crate for about 600
  lines. Written for (a); under (b) Part A moves to liquers-py unchanged.
- **Explanation:** the mechanism is fully specified. Arrow's buffers, our buffers and the release
  protocol line up, so a working plan exists under every answer. What is open is a crate rule the
  maintainer set.
- **Open questions:**
  1. **Open design - placement of `unsafe`** (above).
  2. **Proposed resolution - import copies.** `Buffer` cannot hold foreign memory without a new
     storage variant in `buffer.rs`. Import copies each buffer once, then releases the foreign array.
  3. **Proposed resolution - metadata.** The root schema carries `liquers.schema` and
     `liquers.chunk_id` exactly as the IPC writer does. `rows` and `sources` are not carried, as in
     IPC. Import trusts `liquers.schema` only while it still matches the fields (the IPC rule).
  4. **Proposed resolution - Python transport.** A private pyclass with `__arrow_c_array__` is
     passed to `pyarrow.record_batch` (pyarrow 14 or newer, AC-7), and the batch is wrapped
     without a copy in the type that design chooses (`pyarrow.Table.from_batches([batch])` under its
     recommendation). With an older pyarrow, the IPC route is used (AC-8). No new Python name.

## Design Dependencies

- `py-record-values-to-python` (issue `PY-RECORD-VALUES-CANNOT-CROSS-TO-PYTHON`) — **requires**,
  Part B only: Part B replaces the transport of the record-view arm that design adds and keeps its
  Python-facing type, which that design owns. Part A does not depend on it. That design currently
  records only `overlaps` toward this one; the coordinator aligns it to `required-by`.
- `record-streams` (complete, frozen) — origin: this design is its unbuilt remainder
  (§"Arrow interoperability"). Not extended (overlap rule E4).
- `column-slice-shared-buffers` — **overlaps**: once slices share storage, a window's pointer is
  still `as_bytes().as_ptr()` but only element-aligned. [CDATA] needs no more, so either order.
- `record-decimal-field-type`, `record-uuid-time-duration-types` — **overlaps**: each adds `Column`
  variants. The export's exhaustive `match` turns each new variant into a compile error, so the
  design landing second adds its mapping row (`d:<p>,<s>`, `w:16`, …).
- `record-batch-private-fields` — **overlaps**: under (a) the export sits inside the crate and is
  unaffected. Under (b) it would need the accessors that design adds.

## Consolidated Findings

- The crate's "no `unsafe`" rule is the only real decision; every other choice is fixed by [CDATA]
  or by the IPC writer (metadata keys, the `UTC` zone, the type subset).
- `RecordBatch`'s fields are `pub` and `Bitmap` deserializes `bits` and `len` independently, so
  the safe exporter re-validates every buffer length before handing out a pointer (AC-10).
- A non-batch view exports through `materialize()`, one copy (`Column::slice` copies today).
- Part B needs no `unsafe` in liquers-py: the pair is `Send`, releases itself on `Drop`, and sits in
  a `Mutex` inside the pyclass (PyO3 0.30 requires `Sync`). Part B waits until
  `py-record-values-to-python` is implemented and merged.
- Two PRs, one contract; no split, since Part B (about 80 lines) has no contract of its own.
