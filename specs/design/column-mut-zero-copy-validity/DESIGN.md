---
id: COLUMN-MUT-ZERO-COPY-VALIDITY
kind: design
title: ColumnMut moves validity, Bool values and offsets, and edits a Text/Binary cell in place
form: compact
status: in_review
phase: implementation
readiness: ready
autofix: not-eligible
area: [records]
issues: [COLUMNMUT-VALIDITY-AND-VARIABLE-LENGTH-SET-ALWAYS-COPY]
created: 2026-10-21
---
# ColumnMut moves validity, Bool values and offsets, and edits a Text/Binary cell in place

Bulk design (`specs/guides/autonomous_bulk_design.md`): Phases 1-4 written, not approved, nothing
implemented. Origin: `record-streams` (complete), Step 2.5.

## Phase 1: High-Level Design

### Purpose

Finish the zero-copy round trip `RecordBatch::into_mut` → edit → `freeze` that `record-streams`
built for value buffers: validity, `Bool` values and `Text`/`Binary` offsets move too when the batch
holds them alone, and overwriting one variable-length cell costs the tail it shifts, not a rebuild of
the whole column.

### Problem Example

A 5-million-row `Text` column with validity, edited one cell at a time
(`batch.into_mut()`, then `set_value(row, col, &FieldValue::Text("x".into()))` 100 times, then
`freeze()`). At HEAD (`liquers-records/src/mutable.rs`): `column_into_mut` expands validity into a
`Vec<bool>` (5 MB from 625 KB) and copies the offsets (`offsets.as_slice().to_vec()`);
each `set` calls `set_bytes_cell`, which copies all data to `old_bytes`, rebuilds `new_data` and every
offset — 100 full-column copies; `freeze` packs the `Vec<bool>` back (`Bitmap::from_bools`) and
copies the offsets again (`Buffer::from_slice`). `Bool` columns go through `Vec<bool>` both ways.
Should: validity and `Bool` values stay packed and move with the batch; a same-length cell is
overwritten in place; a different-length cell moves only the bytes after it and adds the length
difference to the later offsets.

Partly changed since filing: `set` on `Text` is now tested
(`column_mut_set_overwrites_a_text_cell_and_rebuilds_offsets`), so it is verified, only slow.

### Scope and Acceptance Criteria

- **AC-1** Validity moves
  WHEN `into_mut` and `freeze` run on a column whose validity bitmap the batch holds alone
  THEN the bitmap's storage is reused (no allocation of a new bitmap) and the frozen validity equals the original
- **AC-2** No validity until a null
  WHEN a `ColumnMut` receives only non-null values
  THEN it allocates no validity and freezes with `validity: None`, as today
- **AC-3** Bool values move
  WHEN a `Bool` column round-trips through `into_mut` / `freeze`
  THEN its values bitmap is reused when held alone and the values are equal
- **AC-4** Same-length cell in place
  WHEN `set` overwrites a `Text`/`Binary` cell with one of the same byte length
  THEN only that cell's bytes change and the offsets are untouched
- **AC-5** Different-length cell shifts the tail
  WHEN `set` changes a cell's length (longer, shorter, to or from null)
  THEN the bytes after it move once, later offsets change by the difference, and every other cell reads as before
- **AC-6** Offsets move
  WHEN a `Text`/`Binary` column round-trips through `into_mut` / `freeze`
  THEN its offsets buffer is reused when held alone
- **AC-7** Shared buffers are never written
  WHEN the source batch is still shared (cloned `Arc`) and its `ColumnMut` is edited
  THEN the original batch is unchanged

Non-goals: public API changes (`ColumnMut` keeps its methods); `RecordBatchMut::append_row`
atomicity rules (unchanged); sharing for slices (design `column-slice-shared-buffers`).

### Design Readiness

- **Readiness:** ready
- **Automatic fixing:** not-eligible — adds a crate-private struct `BitmapMut` (rule 3).
- **Leading issue:** None.
- **Explanation:** every change is behind `ColumnMut`'s unchanged public surface; existing tests
  pin behaviour and the new tests observe reuse through a crate-private pointer check.
- **Open questions:** None. The issue's alternative (document the copies as permanent) is rejected:
  the `Text` `set` cost is O(column) per call and grows with the in-place editing commands
  `liquers-lib` is expected to add.

### Design Dependencies

- `column-slice-shared-buffers` — **overlaps**: both edit `buffer.rs`; this design's
  `Bitmap::into_mut` calls `AlignedBuffer::into_mut`, which that design makes window-aware. Either
  order compiles; land the slice design first so `into_mut` is changed once.
- None else.

### Consolidated Findings

- Offsets were also copied both ways — not in the issue, same cause; included (AC-6).
- `BitmapMut` is `pub(crate)` over `AlignedBytesMut`, mirroring `AlignedBuffer` / `AlignedBytesMut`.
- Validity becomes "absent until the first null": the first null allocates and fills `len` set bits,
  so AC-2 keeps today's `None` result without scanning on `freeze`. A frozen all-valid bitmap that
  came in through `into_mut` is kept as is (Arrow allows it; `Column::validate` accepts it); only the
  `Vector` `dim == 0` rule still forces a bitmap.
- Reference: `RECORD_STREAMS.md` ("validity always copied", "`set` … rebuilds the tail", the
  batch ↔ mutable row) changes.

## Phase 2: Architecture

### Solution

`buffer.rs` gains `pub(crate) struct BitmapMut { bytes: AlignedBytesMut, len: usize }` with `new`,
`with_capacity`, `push(bool)`, `set(i, bool)`, `get(i)`, `len`, `truncate`, `freeze() -> Bitmap`
(moves storage), and `Bitmap::into_mut(self) -> BitmapMut` (via `AlignedBuffer::into_mut`).
`AlignedBytesMut` gains `splice(range, bytes)` (replace a byte range, moving the tail once).

`mutable.rs` `ColumnMutInner`: every `validity: Vec<bool>` becomes `validity: Option<BitmapMut>`;
`Bool.values` becomes `BitmapMut`; `Text`/`Binary` `offsets: Vec<i32>` becomes `AlignedBytesMut`
read through `bytemuck`. Helpers: `validity_set(&mut Option<BitmapMut>, len, row, valid)` allocates
on the first `false`. `set_bytes_cell` reads `old = offsets[row]..offsets[row + 1]`; equal length →
`data.set`; otherwise `data.splice(old, new)` and add `delta` to `offsets[row + 1..]` with checked
`i32` arithmetic (overflow → error, as `offset_from_len` today). `ColumnMark` / `restore` truncate the
bitmaps. `column_into_mut` / `freeze` use `Bitmap::into_mut` / `BitmapMut::freeze` and
`Buffer::into_aligned().into_mut()` for offsets.

Rejected: a `Vec<u64>`-word bitmap (another layout to convert); keeping `Vec<bool>` validity and only
fixing `set` (leaves AC-1/AC-3).

Known-issue preflight: none blocks. No commands.

### Changes

```rust
// liquers-records/src/buffer.rs
pub(crate) struct BitmapMut { bytes: AlignedBytesMut, len: usize }
impl Bitmap { pub(crate) fn into_mut(self) -> BitmapMut; }
impl AlignedBytesMut { pub(crate) fn splice(&mut self, range: Range<usize>, bytes: &[u8]); }
```

`mutable.rs`: `ColumnMutInner` fields, `validity_option` / `validity_to_vec` replaced,
`set_bytes_cell`, `push`, `set`, `mark` / `restore`, `freeze`, `column_into_mut`. Errors unchanged.

### Risks

| Risk | Mitigation | Certainty |
|---|---|---|
| `restore` after a refused `append_row` leaves a stray bit | `BitmapMut::truncate`; existing atomicity tests | high |
| Offset overflow on a growing cell | checked add, error as today | high |
| Behaviour drift in `freeze` (validity `None` vs all-set) | AC-2 test; `Column` equality ignores nothing else | medium |
| Writing a shared storage | all moves go through `into_mut`, which copies when shared (AC-7) | high |

## Phase 3: Examples and Tests

### Examples

Problem Example: 100 `set` calls cost 100 tail moves; a same-length rename costs one cell write.

### Tests

`liquers-records/src/buffer.rs` `mod tests`:

- `bitmap_into_mut_and_freeze_reuse_unshared_storage` — AC-1, AC-3
- `bitmap_mut_push_set_truncate_match_bitmap` — AC-1
- `aligned_bytes_mut_splice_moves_the_tail` — AC-5

`liquers-records/src/mutable.rs` `mod tests` (reuse checked with a crate-private
`AlignedBuffer::storage_ptr` test helper):

- `into_mut_freeze_reuses_validity_held_alone` — AC-1
- `all_valid_column_mut_freezes_without_validity` — AC-2
- `bool_column_round_trip_reuses_values` — AC-3
- `text_set_same_length_writes_in_place` — AC-4
- `text_and_binary_set_longer_shorter_and_null_shift_the_tail` — AC-5
- `text_round_trip_reuses_offsets` — AC-6
- `editing_a_shared_batch_leaves_the_original` — AC-7
- existing `column_mut_set_overwrites_a_text_cell_and_rebuilds_offsets`, the `append_row_is_atomic_*`
  and round-trip tests unchanged — AC-2, AC-5

Run: `cargo test -p liquers-records --lib -- mutable buffer`.

## Phase 4: Implementation Plan

### Steps

- [ ] 1. `buffer.rs` — `BitmapMut`, `Bitmap::into_mut`, `AlignedBytesMut::splice`, test helper `storage_ptr` — buffer tests — `cargo test -p liquers-records --lib buffer`. Rollback: revert (crate-private).
- [ ] 2. `mutable.rs` — validity and `Bool` values as `BitmapMut`; `mark` / `restore` — AC-1-3, AC-7 tests and existing tests — `cargo test -p liquers-records --lib mutable`. Depends on 1.
- [ ] 3. `mutable.rs` — offsets as `AlignedBytesMut`, `set_bytes_cell` in place / splice — AC-4-6 tests — same command. Depends on 2.
- [ ] 4. `cargo test -p liquers-records --all-features --lib --tests`; `cargo test -p liquers-lib --lib --tests` (record editing through the glue).
- [ ] 5. `RECORD_STREAMS.md` batch ↔ mutable row and `ColumnMut` paragraph, History, `reviewed:`; close `COLUMNMUT-VALIDITY-AND-VARIABLE-LENGTH-SET-ALWAYS-COPY` — `python3 scripts/docs_index.py --check`.

### Validation

`cargo test -p liquers-records --lib --tests`; `bash scripts/check-build-matrix.sh` (records rows);
diff review limited to `buffer.rs`, `mutable.rs` and the reference.
