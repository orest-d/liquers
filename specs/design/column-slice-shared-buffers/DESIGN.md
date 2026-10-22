---
id: COLUMN-SLICE-SHARED-BUFFERS
kind: design
title: Column::slice shares its parent's buffers instead of copying them
form: compact
status: in_review
phase: implementation
readiness: needs-decision
autofix: not-eligible
area: [records]
issues: [COLUMN-SLICE-COPIES-INSTEAD-OF-SHARING-BUFFER-STORAGE]
created: 2026-10-21
---
# Column::slice shares its parent's buffers instead of copying them

Bulk design (`specs/guides/autonomous_bulk_design.md`): Phases 1-4 written, not approved, nothing
implemented. Origin: `record-streams` (complete), whose Phase 2 promised `// zero-copy` slices.

## Phase 1: High-Level Design

### Purpose

Give `AlignedBuffer` / `Buffer<T>` a window over their shared storage, so `Column::slice` — and
through it every `RecordView::column_range` on a `RecordBatch`, `RowRangeView`, `ns-rec/slice` and
`head` — returns columns that share the parent's `Arc` rather than fresh copies.

### Problem Example

A 10-million-row `Float` column (80 MB). A UI scrolls `batch.column_range(col, 1_000_000..1_000_050)`
or a query runs `-R/data/big.feather/-/ns-rec/to_record/slice-1000000-50` (validated: Ok). At HEAD
`Column::slice` (`liquers-records/src/column.rs`, `slice_buffer`, `slice_bytes`, `slice_vector`,
`slice_bitmap`) allocates and copies every value it returns: `Buffer::from_slice(&slice[..])`. That
is cheap for 50 rows, but `head-5000000` or `slice` of half a 2 GB text column copies 1 GB, and two
views over one batch hold two copies. Should: the returned `Column` holds clones of the parent's
`Arc`s with a `(start, len)` window; only bitmaps whose bit offset is not a multiple of 8 and the
`len + 1` rebased `Text`/`Binary` offsets are copied.

### Scope and Acceptance Criteria

- **AC-1** Fixed-width slice shares storage
  WHEN `Column::slice(offset, len)` is called on an `Int`, `UInt`, `Float`, `Date`, `Timestamp` or `Vector` column
  THEN the result's values are the requested range and its value buffer shares the parent's allocation (`Buffer::shares_storage_with` is true)
- **AC-2** Text and Binary data shared
  WHEN a `Text` or `Binary` column is sliced
  THEN its `data` shares the parent's allocation, windowed to the slice's bytes, and its offsets start at 0
- **AC-3** Bitmaps
  WHEN validity or `Bool` values are sliced at an offset that is a multiple of 8
  THEN the bitmap shares storage; at any other offset it is copied, and both read the same bits
- **AC-4** A window behaves as a buffer
  WHEN a sliced column is compared, serialized (serde, IPC, Parquet, CSV), validated, filtered, taken, sliced again or turned into a `ColumnMut`
  THEN every result equals what the copying implementation gives today; serialization writes only the window
- **AC-5** Element alignment
  WHEN a typed buffer is a window
  THEN `as_slice` is aligned to its element type, and an unsliced buffer stays 64-byte aligned
- **AC-6** Parent unaffected
  WHEN a `ColumnMut` built from a slice is edited, or a sliced `Bitmap` is `set`
  THEN the parent column is unchanged (copy-on-write)

Non-goals: an Arrow `offset` field on `Column` (public enum change); views other than slicing;
`take` / `filter` (they must copy).

### Design Readiness

- **Readiness:** needs-decision
- **Automatic fixing:** not-eligible — it weakens a documented layout guarantee (Q1, rule 5), and
  adds a public `shares_storage_with` method (rule 4).
- **Leading issue:** Q1, the alignment guarantee of a window.
- **Explanation:** the window is private to `buffer.rs`; every accessor already goes through
  `as_bytes` / `as_slice`, so callers see no change except where alignment and sharing are observed.
- **Open questions:**
  1. **Open design - alignment of a slice:** `RECORD_STREAMS.md` §"Memory layout" says an
     `AlignedBuffer`'s start is 64-byte aligned. Recommended: keep 64 for every buffer that is not a
     window and promise only element alignment for a window (what Arrow's C Data Interface and
     JavaScript typed arrays need; arrow-rs slices the same way). Alternative: share only when the
     window starts on a 64-byte boundary and copy otherwise (keeps the guarantee, shares 1 slice in
     8 for `i64`).
  2. **Proposed resolution - `Text` offsets:** copy and rebase the `len + 1` offsets so a slice
     serializes only its own bytes (recommended), rather than sharing offsets and the whole data
     buffer (fully zero-copy, but a 5-row slice of a 1 GB column would serialize 1 GB).

### Design Dependencies

- `column-mut-zero-copy-validity` — **overlaps**: both edit `buffer.rs`; that design's
  `Bitmap::into_mut` relies on `AlignedBuffer::into_mut`, which this design teaches about windows.
  Land this first; the T3 test does not hold because the edited functions differ.
- `RECORDS-ARROW-C-DATA-EXPORT-NOT-BUILT` (feature, design `record-streams`) — **required-by
  (soft)**: export does not need sharing (it can hand over whole buffers), but exporting a slice
  without a copy does, and the export's pointer must then be the window's start. Q1's answer is
  that export's alignment contract.
- `PY-RECORD-VALUES-CANNOT-CROSS-TO-PYTHON` — no dependency: its first slice copies through IPC.
- `ipc-reader-polars-text-columns` — **overlaps**: edits the IPC reader's offsets handling; no
  shared function.

### Consolidated Findings

- `AlignedBuffer` gains a private `offset`; `PartialEq` must compare `as_bytes()` (the derive would
  compare the whole storage), and `make_bytes_mut` / `into_mut` copy a window unless it is unique and
  starts at 0.
- `liquers-web`'s `column(i)` descriptors point at `as_bytes()`; a window pointer stays inside the
  `Arc` the handle keeps alive, and element alignment suffices for typed arrays (Q1).
- `RECORD_STREAMS.md` rows "Copies" (§ limitations), the `RecordBatch` row of the views table and
  §"Memory layout" change; the record-streams Phase 2 `// zero-copy` claim becomes true.

## Phase 2: Architecture

### Solution

`AlignedBuffer { chunks: Arc<Vec<AlignedChunk>>, offset: usize, len: usize }` — `as_bytes` returns
`flat[offset..offset + len]`. A crate-private `AlignedBuffer::slice(start, len) -> Result<_, Error>`
clones the `Arc` and narrows the window; `Buffer<T>::slice(start, len)` multiplies by
`size_of::<T>()`, so a window always starts on an element boundary and `bytemuck::cast_slice` cannot
fail on alignment. `Bitmap::slice(offset, len)` shares bytes when `offset % 8 == 0` (bits past `len`
are already ignored by every operation) and copies bit by bit otherwise. `Column::slice` calls these;
`slice_bytes` windows `data` to `[off[offset], off[offset + len])` and rebases the copied offsets.

Rejected: an `offset` per `Column` variant (public enum change across five crates); `bytes::Bytes`
(loses the 64-byte allocation alignment, adds a dependency).

Known-issue preflight: none blocks. No commands.

### Changes

`liquers-records/src/buffer.rs`:

```rust
impl AlignedBuffer {
    pub(crate) fn slice(&self, start: usize, len: usize) -> Result<AlignedBuffer, Error>;
    pub fn shares_storage_with(&self, other: &AlignedBuffer) -> bool; // Arc::ptr_eq
}
impl<T: bytemuck::Pod> Buffer<T> {
    pub(crate) fn slice(&self, start: usize, len: usize) -> Result<Buffer<T>, Error>;
    pub fn shares_storage_with(&self, other: &Buffer<T>) -> bool;
}
impl Bitmap { pub(crate) fn slice(&self, offset: usize, len: usize) -> Result<Bitmap, Error>; }
```

`AlignedBuffer`: hand-written `PartialEq` over `as_bytes`; `make_bytes_mut` and `into_mut` copy
unless `offset == 0` and the `Arc` is unique (then `into_mut` truncates to `len`); doc of
`alignment()` per Q1. `Serialize` unchanged (already `as_bytes`).

`liquers-records/src/column.rs`: `slice_buffer`, `slice_vector`, `slice_bitmap`, `slice_bytes` call
the new slices; error messages unchanged.

Documents: `RECORD_STREAMS.md` §"Memory layout" (window, alignment per Q1), views table row
`RecordBatch`, limitation "Copies" removed; History row, `reviewed:`.

### Risks

| Risk | Mitigation | Certainty |
|---|---|---|
| Derived equality or a direct `chunks` read ignores the window | grep `chunks` in `buffer.rs`; equality and serde tests over windows | high |
| A small slice keeps a huge parent alive in memory | documented; `ColumnMut`/`from_slice` give a compact copy; Arrow has the same trade-off | high |
| `Bitmap::set` on a shared window corrupts the parent | `make_bytes_mut` copies the window first (AC-6) | high |
| Slice offsets past the window | `slice` checks against the window's `len`, not the storage | high |

## Phase 3: Examples and Tests

### Examples

`batch.column_range(c, 1_000_000..1_000_050)` returns a column whose
`values.shares_storage_with(&parent_values)` is true. `-R/data/big.feather/-/ns-rec/to_record/slice-1000000-50/out.csv` (validated: Ok)
writes the same CSV as today.

### Tests

`liquers-records/src/buffer.rs` `mod tests`:

- `aligned_buffer_slice_shares_and_windows` — AC-1, AC-5
- `buffer_slice_is_element_aligned` — AC-5
- `aligned_buffer_window_equality_and_serde_use_the_window` — AC-4
- `aligned_buffer_window_into_mut_copies_and_leaves_parent` — AC-6
- `bitmap_slice_on_byte_boundary_shares_otherwise_copies` — AC-3
- `bitmap_set_on_a_shared_slice_leaves_the_parent` — AC-6

`liquers-records/src/column.rs` `mod tests`:

- `column_slice_shares_fixed_width_buffers` (every fixed-width variant and `Vector`) — AC-1
- `column_slice_text_shares_data_and_rebases_offsets` — AC-2
- `column_slice_of_a_slice_equals_the_copying_result` (slice, take, filter, validate, `serde_json`) — AC-4
- existing `column_slice_returns_the_requested_range` unchanged (comment updated) — AC-4

`liquers-records/tests/format_round_trip.rs`: `sliced_batch_round_trips_through_every_writer`
(IPC, CSV, NDJSON, Parquet over `batch.slice(…).materialize()`) — AC-4.

Run: `cargo test -p liquers-records --all-features --lib --tests`.

## Phase 4: Implementation Plan

### Steps

- [ ] 1. `buffer.rs` `AlignedBuffer` — `offset`, `slice`, `shares_storage_with`, manual `PartialEq`, window-aware `make_bytes_mut` / `into_mut` — buffer tests — `cargo test -p liquers-records --lib buffer`. Rollback: revert (private field).
- [ ] 2. `buffer.rs` `Buffer::slice`, `Buffer::shares_storage_with`, `Bitmap::slice` — remaining buffer tests. Depends on 1.
- [ ] 3. `column.rs` `slice_*` helpers — column tests — `cargo test -p liquers-records --lib column`. Depends on 2.
- [ ] 4. `tests/format_round_trip.rs` — `sliced_batch_round_trips_through_every_writer` — `cargo test -p liquers-records --all-features --test format_round_trip`.
- [ ] 5. `cargo test -p liquers-lib --lib --tests` (records glue, polars bridge over slices); after `cargo clean`, `cargo test -p liquers-web --target wasm32-unknown-unknown --features debug-handles,records` (descriptors over windows).
- [ ] 6. `RECORD_STREAMS.md` per Q1, History, `reviewed:`; close `COLUMN-SLICE-COPIES-INSTEAD-OF-SHARING-BUFFER-STORAGE` — `python3 scripts/docs_index.py --check`.

### Validation

`cargo test -p liquers-records --all-features --lib --tests`; `cargo test -p liquers-records --lib --tests`;
`bash scripts/check-build-matrix.sh` (records rows); diff review limited to `buffer.rs`, `column.rs`,
one test file and the reference.
