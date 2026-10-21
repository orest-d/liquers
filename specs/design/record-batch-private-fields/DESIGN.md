---
id: RECORD-BATCH-PRIVATE-FIELDS
kind: design
title: RecordBatch keeps its validated fields private, and materialized batches carry no placeholder chunk id
form: compact
status: in_review
phase: implementation
readiness: needs-decision
autofix: not-eligible
area: [records]
issues: [RECORD-BATCH-FIELDS-CAN-BE-MUTATED-PAST-VALIDATION]
created: 2026-10-21
---
# RecordBatch keeps its validated fields private, and materialized batches carry no placeholder chunk id

Bulk design (`specs/guides/autonomous_bulk_design.md`): Phases 1-4 written, not approved, nothing
implemented. Origin: `record-streams` (complete), implementation review.

## Phase 1: High-Level Design

### Purpose

Make `RecordBatch::new`'s validation an invariant rather than a convention: no code outside
`liquers-records` can change `columns`, `len`, `rows` or `schema` after construction. And keep the
synthetic `in_memory_view-<n>` ids of `InMemorySource` inside the source's bookkeeping, out of every
materialized and serialized batch.

### Problem Example

Outside the crate, at HEAD (`liquers-records/src/batch.rs`, all six fields `pub`):

```rust
let mut batch = RecordBatch::new(schema, columns, None, None, vec![])?; // validated: 3 rows
batch.len = 10;                     // compiles; batch.value(9, 0) now errors in a kernel
batch.columns.pop();                // compiles; column count ≠ field count
```

Second leak: `InMemorySource::new(vec![view]).materialize(resolver, 1000)` (`sources.rs`) places the
view with `Some(in_memory_chunk_id(0))`, so the batch's `chunk_id` is the placeholder query
`in_memory_view-0`, which is serialized with the batch (IPC `chunk_metadata`, serde). Found here too:
a one-chunk stream materialized by `RecordStreamExt::materialize` keeps that id through
`RecordBatch::concat`'s single-input rule, so `ns-rec/head` or a filter over a single-view source
leaks it as well. Should: field writes outside the crate do not compile; a materialized batch's
`chunk_id` is the view's own (`None` for a fresh batch), never a placeholder.

### Scope and Acceptance Criteria

- **AC-1** Fields are read-only outside the crate
  WHEN code outside `liquers-records` assigns to a `RecordBatch` field
  THEN it does not compile; reads go through `columns()`, `rows()`, `sources()`, `schema()`, `len()`, `chunk_id()`
- **AC-2** Safe chunk id change
  WHEN a caller needs a batch with another chunk id (the guide's provenance example)
  THEN `with_chunk_id(Option<ChunkId>)` returns it, every other field unchanged
- **AC-3** Re-stamp stays valid
  WHEN `place_chunk` or `PlacedView::materialize` re-stamps `rows` and `chunk_id`
  THEN they use a crate-private method that keeps the run-length invariant (one run of `len` rows)
- **AC-4** Deserialization unchanged
  WHEN a batch is serialized and read back (serde, IPC)
  THEN the format is byte-identical to today and validation still runs
- **AC-5** No placeholder in a single-view materialize
  WHEN `InMemorySource` with one view is materialized
  THEN the batch's `chunk_id` is the view's own `chunk_id()`
- **AC-6** No placeholder through a wrapper
  WHEN a `FilteredSource` / `MappedSource` over a one-view `InMemorySource` is materialized
  THEN the result's `chunk_id` is not a placeholder (`InMemorySource::is_placeholder_chunk_id` is false)
- **AC-7** Streams still carry ids
  WHEN an `InMemorySource` is streamed
  THEN each streamed view still reports its placeholder `chunk_id()` and `RowId`s as today

Non-goals: `Column`'s variant fields (an enum's fields cannot be private; a private `columns` field
already makes them reachable only by shared reference); `RecordBatchMut`.

### Design Readiness

- **Readiness:** needs-decision
- **Automatic fixing:** not-eligible — changes `pub` fields of a `liquers-records` type (rule 4) and
  edits `liquers-lib` and `liquers-web` (rule 6).
- **Leading issue:** Q1, private fields versus a documented caller invariant.
- **Explanation:** fields become `pub(crate)`, so the ~110 reads inside the crate compile unchanged;
  six read sites in other crates and two test assignments move to accessors.
- **Open questions:**
  1. **Open design - privacy:** make the fields `pub(crate)` with accessors (recommended: the
     invariant is enforced, reads stay free) or keep them `pub` and document that a caller who
     writes one owns the invariant (no migration, nothing enforced). The issue offers both.
  2. **Proposed resolution - materialized chunk id:** the view's own `chunk_id()` (recommended:
     truthful, `None` for a fresh batch as before the placeholder ids existed) rather than always
     `None`.

### Design Dependencies

- `record-streams` (complete) — origin; not extended.
- `RECORDS-ARROW-C-DATA-EXPORT-NOT-BUILT` — **overlaps**: an export inside the crate reads the
  fields directly; nothing to order.
- `liquers-web-records-bridge` (complete) — its `LiquersRecordBatch` reads `inner.columns` and
  `chunk_id`/`sources` by field; migrated here (E4: not extended, its code is edited).

### Consolidated Findings

- `pub(crate)` keeps the change small: outside the crate only `liquers-lib/src/records/polars.rs`
  (`batch.schema`, `batch.columns`, `batch.len`), `liquers-web/src/records.rs` (`columns`,
  `chunk_id`, `sources`), `liquers-web/tests/records_RECORDS.rs`,
  `liquers-records/tests/records_guide_counterparts.rs` (one assignment → `with_chunk_id`) and
  `format_round_trip.rs` read fields.
- Accessor names match the `RecordView` trait (`schema`, `len`, `chunk_id`), so callers that import
  the trait already compile; `origins()` is the trait name for `sources`, so `sources()` is an
  inherent alias kept for the reference's field name.
- The placeholder leak is wider than filed (AC-6); fixing it in `RecordStreamExt::materialize` covers
  every wrapper.
- `RECORD_STREAMS.md`: the `RecordBatch { … }` line and the "Values" limitation bullet;
  `RECORD_STREAM_GUIDE.md` provenance example (`with_chunk_id`).

## Phase 2: Architecture

### Solution

`batch.rs`: fields `pub(crate)`. Inherent accessors `columns(&self) -> &[Column]`,
`rows(&self) -> &[RowRun]`, `sources(&self) -> &[ChunkOrigin]`, plus `with_chunk_id`; `schema`,
`len`, `chunk_id` come from the `RecordView` impl and also get inherent forwarding methods so no
trait import is needed. Crate-private `restamp(&mut self, run_chunk, first_number, chunk_id)` used by
`views.rs` `place_chunk` and `PlacedView::materialize` (one run of `self.len` rows, so it cannot break
the invariant).

`sources.rs` `InMemorySource::materialize`: `place_chunk(view, 0, view.chunk_id().cloned(), Some(0))`.
`batch.rs` `RecordStreamExt::materialize`: after `concat`, if the result's `chunk_id` satisfies
`InMemorySource::is_placeholder_chunk_id`, set it to `None` (crate-internal call; `concat` itself is
unchanged).

Rejected: removing the placeholder ids from streams (AC-7; `describe_chunk` and row ids need them);
a `RecordBatchBuilder` (more surface than the problem needs).

Known-issue preflight: none blocks. No commands.

### Changes

```rust
impl RecordBatch {
    pub fn schema(&self) -> &Arc<RecordSchema>;
    pub fn columns(&self) -> &[Column];
    pub fn len(&self) -> usize;
    pub fn chunk_id(&self) -> Option<&ChunkId>;
    pub fn rows(&self) -> &[RowRun];
    pub fn sources(&self) -> &[ChunkOrigin];
    pub fn with_chunk_id(self, chunk_id: Option<ChunkId>) -> RecordBatch;
    pub(crate) fn restamp(&mut self, chunk: u64, first_number: Option<u64>, chunk_id: Option<ChunkId>);
}
```

Callers: `liquers-lib/src/records/polars.rs` `record_batch_to_dataframe`; `liquers-web/src/records.rs`
(`LiquersRecordBatch` column count, `chunkIdJson`, `sourcesJson`, `column` descriptor lookup);
tests listed above.

### Risks

| Risk | Mitigation | Certainty |
|---|---|---|
| Inherent `len()` shadows the trait's and triggers `clippy::len_without_is_empty` | add inherent `is_empty` | high |
| A missed external read site | `cargo test -p liquers-lib`, the matrix and the web Node loop compile every caller | high |
| A consumer relied on the placeholder in a materialized batch | none in the workspace (`is_placeholder_chunk_id` is used on stream ids only) | medium |

## Phase 3: Examples and Tests

### Examples

`batch.with_chunk_id(Some(ChunkId::Key(parse_key("data/sales/daily_0000.csv")?)))` replaces the
guide's field assignment.

### Tests

`liquers-records/tests/record_batch_api.rs` (new, integration so it is outside the crate):

- `fields_are_read_through_accessors` — AC-1 (a `compile_fail` doctest on `RecordBatch` shows the
  assignment refused) 
- `with_chunk_id_changes_only_the_chunk_id` — AC-2
- `serde_round_trip_is_unchanged` (against a stored JSON fixture of today's output) — AC-4

`liquers-records/src/views.rs` `mod tests`: `place_chunk_restamps_one_run_of_len` — AC-3.
`liquers-records/src/sources.rs` `mod tests`:

- `in_memory_single_view_materialize_keeps_the_views_chunk_id` — AC-5
- `filtered_single_view_source_materializes_without_a_placeholder` — AC-6
- `in_memory_stream_still_carries_placeholder_ids` — AC-7

Existing: `records_guide_counterparts.rs` (migrated), `format_round_trip.rs`, web `records_RECORDS.rs`
— AC-1, AC-4.

Run: `cargo test -p liquers-records --all-features --lib --tests`.

## Phase 4: Implementation Plan

### Steps

- [ ] 1. `batch.rs` — accessors, `with_chunk_id`, `restamp`, `is_empty`; fields still `pub` — `cargo test -p liquers-records --lib batch`. Rollback: revert (additive).
- [ ] 2. Migrate readers: `liquers-lib/src/records/polars.rs`, `liquers-web/src/records.rs`, the three test files — `cargo test -p liquers-lib --lib --tests`. Depends on 1.
- [ ] 3. `batch.rs` fields `pub(crate)` (Q1); `views.rs` uses `restamp`; new `record_batch_api.rs`, compile_fail doctest — `cargo test -p liquers-records --all-features --lib --tests --doc`. Depends on 2.
- [ ] 4. `sources.rs` single-view `materialize`, `batch.rs` `RecordStreamExt::materialize` placeholder drop; AC-5-7 tests — `cargo test -p liquers-records --lib sources`.
- [ ] 5. After `cargo clean`: `cargo test -p liquers-web --target wasm32-unknown-unknown --features debug-handles,records`.
- [ ] 6. `RECORD_STREAMS.md`, `RECORD_STREAM_GUIDE.md`, History, `reviewed:`; close `RECORD-BATCH-FIELDS-CAN-BE-MUTATED-PAST-VALIDATION` — `python3 scripts/docs_index.py --check`.
- [ ] 7. Final diff review — no serialized-format change, no edits outside the named files — `git diff --stat`.

### Validation

`cargo test -p liquers-records --all-features --lib --tests`; `cargo test -p liquers-lib --lib --tests`;
`bash scripts/check-build-matrix.sh`; diff review: no serialized-format change.
