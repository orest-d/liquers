# Phase 4: Implementation Plan — UUID, time-of-day and duration record field types

## Overview

Model first, then formats, then bridges, then documents. Record the decisions on Phase 1
questions 1-3 before step 1; under a different answer to question 1 (an `Interval` type) this
plan gains a variant and the sort/filter refusals it needs, and should be re-reviewed. If
`record-decimal-field-type` is mid-implementation, take its branch as the base so the match arms
merge once.

## Progress

- [ ] Step 1: text forms (`temporal.rs`)
- [ ] Step 2: the model — `FieldType`, `Column`, `FieldValue`, `ColumnMut`
- [ ] Step 3: text table formats
- [ ] Step 4: IPC
- [ ] Step 5: Parquet writer
- [ ] Step 6: polars bridge and the cell mapping in `liquers-lib`
- [ ] Step 7: browser bridge
- [ ] Step 8: documentation and final review

## Implementation Steps

### Step 1: text forms (`temporal.rs`)
- Files / symbols: new `liquers-records/src/temporal.rs` with the Phase 2 functions and their unit
  tests (test 4); `lib.rs` `mod temporal;`.
- Depends on: nothing.
- Proof: `cargo test -p liquers-records --lib temporal`.
- Rollback: delete the module.

### Step 2: the model
- Files / symbols: `schema.rs` (`FieldType` variants; test 11), `column.rs` (variants and every
  method's arms, `parse_text`, `compare`; test 10), `mutable.rs` (`ColumnMut`), then the arms the
  compiler names in `batch.rs`, `views.rs`, `sources.rs`, `search/*` (no `_ =>`).
- Depends on: 1.
- Proof: `cargo test -p liquers-records --all-features --lib --tests`.
- Rollback: revert the step; nothing outside records sees the variants yet.

### Step 3: text table formats
- Files / symbols: `formats/csv.rs` (`parse_scalar` and writing), `markdown.rs`, `html.rs`,
  `ndjson.rs`, `shapes.rs`; test 9.
- Depends on: 2.
- Proof: `cargo test -p liquers-records --lib --tests`.
- Rollback: revert.

### Step 4: IPC
- Files / symbols: `formats/ipc.rs` — `decode_arrow_field` arms for tags 9 (`Time`), 18
  (`Duration`), 15 (`FixedSizeBinary` with the `arrow.uuid` extension read from the Field's
  `custom_metadata`); writer type tables and field metadata; buffer read/write arms; `to_micros`
  use; tests 5-8; narrow the existing tag-15 refusal test.
- Depends on: 2.
- Proof: `cargo test -p liquers-records --features ipc --lib --tests`.
- Rollback: revert; the formats of step 3 stand alone.

### Step 5: Parquet writer
- Files / symbols: `formats/parquet.rs` — schema elements (`FIXED_LEN_BYTE_ARRAY(16)` + `UUID`;
  `INT64` + `TIME(MICROS, false)`; plain `INT64`) and value encoding.
- Depends on: 2.
- Proof: `cargo test -p liquers-records --features parquet --lib --tests` (write side); read side
  in step 6.
- Rollback: revert.

### Step 6: polars bridge and the cell mapping
- Files / symbols: `liquers-lib/Cargo.toml` (confirm `dtype-time` / `dtype-duration`, add if
  needed); `records/polars.rs` (`polars_dtype_to_field_type`, `append_series_values`, the reverse
  builder); `value/mod.rs` `record_view_cell_as_base`; arms in `mem/*`, `search/*`,
  `records/{commands,table_ops}.rs`; tests 12, 13, 15.
- Depends on: 2-5.
- Proof: `cargo test -p liquers-lib --lib --tests`; `bash scripts/check-build-matrix.sh`.
- Rollback: revert; records stay usable without the bridge.

### Step 7: browser bridge
- Files / symbols: `liquers-web/src/records.rs`, `typescript.rs`, `records_column.js`,
  `default_value.rs`; test 14.
- Depends on: 2.
- Proof: `cargo clean` then `cargo test -p liquers-web --target wasm32-unknown-unknown --features
  debug-handles,records`; `./liquers-web/scripts/check-stubs.sh --build --records`.
- Rollback: revert.

### Step 8: documentation and final review
- Files / symbols: `RECORD_STREAMS.md`, `RECORD_STREAM_GUIDE.md` (History rows, `reviewed:`),
  `liquers-web/README.md`; issue `RECORD-FIELD-TYPE-HAS-NO-UUID-TIME-OR-INTERVAL` status and a
  resolution note naming the documented non-types; a note in `relational-database-access`
  question 3 that `UUID` and `TIME` now map to `Uuid` and `Time`, while `INTERVAL` and
  `TIMESTAMP WITHOUT TIME ZONE` stay `Text`.
- Depends on: 1-7.
- Proof: `python3 scripts/docs_index.py --check`; diff review (no stdout printing, no unrelated edits).

## Testing Plan

Focused per step as above. Crate-level: `cargo test -p liquers-records --all-features --lib
--tests`, `cargo test -p liquers-records --lib --tests`, `cargo test -p liquers-lib --lib --tests`,
the web Node suite with `records`. Matrix: `bash scripts/check-build-matrix.sh` (feature-gated
bridge code, wasm32 rows).

## Rollback Plan

One commit per step. Steps 3-7 each revert independently onto step 2; reverting step 2 requires
reverting everything after it.

## Documentation Updates

`specs/reference/RECORD_STREAMS.md`, `specs/guides/RECORD_STREAM_GUIDE.md`, `liquers-web/README.md`
(step 8).

## Phase 5 Entry Criteria

Not applicable (no `workflow` marker). Done when questions 1-3 are decided, every proof passes and
`docs_index.py --check` is clean.
