---
id: COLUMNMUT-VALIDITY-AND-VARIABLE-LENGTH-SET-ALWAYS-COPY
kind: issue
title: ColumnMut always copies validity, and rebuilds the whole column to overwrite one Text/Binary cell
status: draft
priority: P3
complexity: M
area: [records]
design: record-streams
created: 2026-09-26
github:
---
## Problem

`liquers-records/src/mutable.rs` (Step 2.5) gives `RecordBatch::into_mut`/`ColumnMut` a genuine
zero-copy path for the *value* buffer of `Int`/`UInt`/`Float`/`Date`/`Timestamp`/`Vector`/`Text`/
`Binary` columns: `AlignedBuffer::into_mut` uses `Arc::try_unwrap` to hand over the storage
directly when this batch is its only owner, copying only when it is shared. Two related paths do
not get the same treatment and always copy, regardless of ownership:

1. **Validity.** `column_into_mut`'s `validity_to_vec` always expands `Option<Bitmap>` into a
   fresh `Vec<bool>` bit by bit via `Bitmap::get`, and `ColumnMutInner::freeze`'s
   `validity_option` always rebuilds a `Bitmap` from that `Vec<bool>` via `Bitmap::from_bools` —
   in both directions, whether or not the original `Bitmap`'s `AlignedBuffer` was uniquely held.
   `Bitmap` has no growable counterpart the way `AlignedBuffer`/`AlignedBytesMut` now do, so there
   is no cheaper path available today.
2. **`ColumnMut::set` on `Text`/`Binary`.** Overwriting one variable-length cell
   (`set_bytes_cell`) rebuilds the column's entire `data`/`offsets` from scratch — O(row count +
   total byte size) — because replacing a cell whose new length differs from its old one shifts
   every later offset. There is no tested code path that calls `set` on a `Text`/`Binary`
   `ColumnMut` (Phase 3 §2.6's tests only exercise `Int`), so this is unverified in practice, not
   just unoptimized.

## Impact

Both are real costs, not correctness bugs — every Phase 3 §2.6 test and the additional coverage
added in Step 2.5 (`column_mut_round_trips_text_values`,
`column_mut_set_overwrites_a_text_cell_and_rebuilds_offsets`, etc.) passes. The validity copy is
one bit per row, negligible next to the value buffer `AlignedBuffer::into_mut` now moves for free,
so is unlikely to matter in practice. The `Text`/`Binary` `set` path could matter for a caller
doing many single-cell overwrites on a large text column (each one re-copies the whole column),
which is a plausible use of `set_value`/`ColumnMut::set` once `liquers-lib` commands start editing
tables in place.

## Expected behaviour

One of:
1. Accept both as documented, permanent trade-offs (validity is small; `set` on variable-length
   cells is expected to be rare relative to `append_row`/`push`), and note it at the call site
   instead of leaving it silent.
2. Give `Bitmap` a growable counterpart (mirroring `AlignedBytesMut`) so validity can move zero-copy
   too, and/or give `ColumnMut::set` on `Text`/`Binary` an in-place fast path when the new cell's
   byte length equals the old one (no offset shift needed) instead of always rebuilding.

## Discovery

Found while implementing Step 2.5 of the record-streams Phase 4 plan
(`liquers-records/src/mutable.rs`): building `RecordBatch::into_mut`'s zero-copy value-buffer path
made the asymmetry with validity handling and `Text`/`Binary`'s `set` visible by contrast — both
already existed as `Column`/`ColumnMut` design decisions (`Bitmap`'s lack of a growable form,
`Text`/`Binary`'s offset-shift-on-edit shape), not something Step 2.5 introduced.
