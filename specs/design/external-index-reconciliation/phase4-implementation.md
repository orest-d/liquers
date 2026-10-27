# Phase 4: Implementation Plan — The reconciliation contract for external indexes

## Overview

One new module in `liquers-records`, its tests, one end-to-end test in `liquers-lib`, two
documents. No prerequisite design; the push path and real sinks are later. Assumes Phase 1's
recommended answers (Q1 projection on the sink, Q4 time-based versionless entries); Q1's
alternative would remove `ObservedEntry::projection` and fold the id into the compared version in
Step 2.

## Progress

- [ ] Step 1: the types and the `IndexSink` trait
- [ ] Step 2: `reconcile`
- [ ] Step 3: `explain`
- [ ] Step 4: `MemorySink`
- [ ] Step 5: the `sync_reconcile` suite
- [ ] Step 6: the `liquers-lib` end-to-end test
- [ ] Step 7: documents, issue and index maintenance
- [ ] Step 8: checks and final diff review

## Implementation Steps

### Step 1: the types and the `IndexSink` trait
- Files / symbols: new `liquers-records/src/sync.rs` (`ProjectionId`, `EntryFreshness`,
  `ObservedEntry`, `FieldSupport`, `ChunkUpsert`, `IndexSink`, `ReconcileOptions` with a
  `Default`-free constructor taking `now`, `RefeedReason`, `ChunkOutcome`, `ReconcileReport`,
  `ChunkStatus`); `lib.rs` `pub mod sync` and re-exports
- Depends on: none
- Proof: `cargo check -p liquers-records` and `--target wasm32-unknown-unknown`
- Rollback: delete the module

### Step 2: `reconcile`
- Files / symbols: `sync.rs` `reconcile` and private helpers `decide(descriptor, entry, …) ->
  Decision` (pure, unit-tested in `mod tests`) and `fetch(source, resolver, descriptor)`
- Change: Phase 2 §"The diff", steps 1-7. Re-check the metadata status accessor and the
  `Status` variants considered ready (`Ready`, `Source`, `Override`) against
  `liquers-core/src/metadata.rs` before writing the explicit match.
- Depends on: Step 1
- Proof: `cargo test -p liquers-records --lib sync` (unit tests of `decide`, one per row of the
  diff)
- Rollback: delete the function

### Step 3: `explain`
- Files / symbols: `sync.rs` `explain` — chunk list membership, `observed`, one `describe_chunk`,
  the same `decide` helper, no fetch
- Depends on: Step 2
- Proof: unit tests in `mod tests`
- Rollback: delete the function

### Step 4: `MemorySink`
- Files / symbols: `sync.rs` `MemorySink` and `impl IndexSink` — state behind `std::sync::Mutex`,
  never held across `.await` (the futures are `async move` over cloned data or ready values)
- Depends on: Step 1
- Proof: `cargo test -p liquers-records --lib sync::tests::memory_sink_*`
- Rollback: delete the type

### Step 5: the `sync_reconcile` suite
- Files: new `liquers-records/tests/sync_reconcile.rs` (Phase 3 Test Plan)
- Depends on: Steps 2-4
- Proof: `cargo test -p liquers-records --test sync_reconcile` (AC-1 … AC-11)
- Rollback: delete the file

### Step 6: the `liquers-lib` end-to-end test
- Files: new `liquers-lib/tests/record_sync_end_to_end.rs`; validate the query with
  `liquers-validate` first
- Depends on: Step 5
- Proof: `CARGO_INCREMENTAL=0 cargo test -p liquers-lib --test record_sync_end_to_end`
- Rollback: delete the file

### Step 7: documents, issue and index maintenance
- Files: `specs/reference/RECORD_STREAMS.md`, `specs/guides/RECORD_STREAM_GUIDE.md` (History rows,
  `reviewed:`); `specs/issues/EXTERNAL-INDEX-HAS-NO-RECONCILIATION-CONTRACT.md` — **stays open**
  with a note that slice 1 landed (real sinks, scheduling and store-scan policy remain), or is
  split into follow-up issues at that point, as the maintainer prefers; this design's status;
  `python3 scripts/docs_index.py`
- Depends on: Steps 1-6
- Proof: `python3 scripts/docs_index.py --check`
- Rollback: revert the documents

### Step 8: checks and final diff review
- Proof: `cargo test -p liquers-records --all-features --lib --tests`;
  `cargo test -p liquers-records --lib --tests`; `bash scripts/check-build-matrix.sh` (records
  rows, wasm32); diff review — no `println!`, no `unwrap`/`expect` in library code, explicit
  matches, no stray files

## Testing Plan

Unit tests of the decision table (Step 2), the integration suite with the test double (Step 5),
one end-to-end run through a real environment (Step 6), the build matrix (Step 8).

## Rollback Plan

Additive throughout; deleting `sync.rs` and the two test files restores HEAD.

## Documentation Updates

As Step 7. No generated registry change.

## Phase 5 Entry Criteria

None — four-phase design.
