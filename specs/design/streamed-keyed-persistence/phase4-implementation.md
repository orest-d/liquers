# Phase 4: Implementation Plan — Streamed persistence of keyed assets

## Overview

Entry condition: `value-serialization-streaming` (at least its core and polars steps) and
`store-streaming-read-write` (at least `set_stream` with its defaults and the file / OpenDAL
overrides) have landed. Phase 1 question 1 answered A, questions 2-4 as recommended. Order: the
hasher, the streamed write, the switch in `prepare_version` and `complete_evaluation`, tests,
documents.

## Progress

- [ ] Step 1: ContentHasher
- [ ] Step 2: write_streamed and finish_streamed_persistence
- [ ] Step 3: Switch prepare_version and complete_evaluation
- [ ] Step 4: liquers-lib tests
- [ ] Step 5: Documentation and issue closure

## Implementation Steps

### Step 1: ContentHasher
- Files / symbols: `liquers-core/src/metadata.rs` `ContentHasher`, `Version::content_hasher`;
  `Version::from_content` rewritten through it.
- Depends on: none (can land before the prerequisites).
- Proof: `cargo test -p liquers-core --lib content_hasher`, plus the whole `metadata` test module
  (existing version tests pin `from_content`).
- Rollback: revert.

### Step 2: write_streamed and finish_streamed_persistence
- Files / symbols: `liquers-core/src/assets.rs` `PreparedVersion::streamed`, `StreamedWrite`,
  `AssetRef::write_streamed`, `AssetRef::finish_streamed_persistence`.
- Change: Phase 2 §Integration Points; not called yet.
- Depends on: step 1 and the prerequisites.
- Proof: `cargo test -p liquers-core --lib --tests` (no behaviour change).
- Rollback: revert.

### Step 3: Switch prepare_version and complete_evaluation
- Files / symbols: `assets.rs` `prepare_version` (`serialize_stream` instead of `as_bytes`, the
  `Whole` / `Stream` / `Err` match), `complete_evaluation` (call `finish_streamed_persistence`
  when `streamed` is set).
- Depends on: step 2.
- Proof: full `cargo test -p liquers-core --lib --tests` (AC-8: whole path unchanged), then
  `cargo test -p liquers-lib --lib --tests`.
- Rollback: revert this commit only; steps 1-2 are inert.

### Step 4: liquers-lib tests
- Files: `liquers-lib/tests/streamed_persistence.rs` (Phase 3 list), file-level
  `#![cfg(feature = "polars")]` except `streamed05`, which goes in an ungated module of the same
  file or a sibling file.
- Depends on: step 3.
- Proof: `CARGO_INCREMENTAL=0 cargo test -p liquers-lib --test streamed_persistence`;
  `cargo test -p liquers-lib --no-default-features --lib --tests` (the gated file compiles out).
- Rollback: delete the file.

### Step 5: Documentation and issue closure
- Files: `specs/reference/ASSETS.md`, `specs/reference/ASSET_LIFECYCLE.md`,
  `specs/guides/SERIALIZATION_GUIDE.md` (History row, `reviewed:` bump each); close
  `STREAMED-VALUE-IS-COLLECTED-BEFORE-IT-IS-STORED` with a resolution note naming the remaining
  non-goal (streaming in `set_state` and friends) and file it if wanted.
- Proof: `python3 scripts/docs_index.py --check`.

## Testing Plan

Each step: `cargo test -p liquers-core --lib --tests`. Final: `cargo test -p liquers-lib --lib
--tests`; `bash scripts/check-build-matrix.sh` (feature rows, wasm32: `set_stream` and the tee are
`?Send` there through `maybe_send::BoxStream`); after `cargo clean`, the `liquers-web` Node loop
with `--features debug-handles`, because the inline manager's keyed path changed.

## Rollback Plan

Revert step 3 to restore whole persistence for every value; steps 1, 2 and 4 are inert or test-only
without it.

## Documentation Updates

`reference/ASSETS.md`, `reference/ASSET_LIFECYCLE.md`, `guides/SERIALIZATION_GUIDE.md` (History,
`reviewed:`); `specs/index.csv` regenerated; no `command_registry.yaml` change.

## Phase 5 Entry Criteria

None — four-phase design.
