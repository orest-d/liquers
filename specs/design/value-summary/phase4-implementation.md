# Phase 4: Implementation Plan — value-summary

## Overview

Core model and fields first, then the hook and its sync, then library summaries, then tests and
documents. Prefer landing after `value-interface-capability-split` (same traits); independent of the
rest. Assumes Phase 1's recommended answers.

## Progress

- [ ] Step 1: `ValueSummary` and the metadata fields
- [ ] Step 2: the hook on the value traits and the sync in `State`
- [ ] Step 3: core `Value` and `SimpleValue` summaries
- [ ] Step 4: `ExtValue` summaries and foreign delegation
- [ ] Step 5: integration tests
- [ ] Step 6: documents and records
- [ ] Step 7: full checks

## Implementation Steps

### Step 1: model and fields
- Files / symbols: `liquers-core/src/metadata.rs` — `ValueSummary` (`new`, `with`),
  `MetadataRecord::value_summary`, `AssetInfo::value_summary`, `MetadataRecord::with_value_summary`,
  `Metadata::with_value_summary`, `get_asset_info`, `AssetInfo::new`; struct literals elsewhere
  (`liquers-axum/src/assets/value_description.rs`, axum tests) gain `value_summary: None`.
- Depends on: none.
- Proof: `cargo test -p liquers-core value_summary_is_optional_in_json`; `cargo check -p liquers-axum --tests`.
- Rollback: revert.

### Step 2: hook and sync
- Files / symbols: `liquers-core/src/value.rs` `ValueInterface::value_summary` (default `None`);
  `liquers-lib/src/value/extended.rs` `ValueExtension::value_summary` and `CombinedValue` delegation;
  `liquers-lib/src/value/foreign.rs` `ForeignValue::value_summary`; `liquers-core/src/state.rs`
  `sync_metadata_with_value`.
- Depends on: Step 1.
- Proof: `cargo test -p liquers-core`.
- Rollback: revert Steps 1-2.

### Step 3: core summaries
- Files / symbols: `impl ValueInterface for Value` (`Text`, `Bytes`, `Array`, `Object` arms; every
  other variant `None`, explicitly); `liquers-lib/src/value/simple.rs` the same.
- Depends on: Step 2.
- Proof: `cargo test -p liquers-core summary_follows_the_value`.
- Rollback: revert the impls (hook defaults remain).

### Step 4: library summaries
- Files / symbols: `liquers-lib/src/value/mod.rs` `ValueExtension::value_summary` for `ExtValue`
  (`Image`, gated `PolarsDataFrame`, gated `RecordView`, `Foreign` delegates; others `None`, explicit
  arms).
- Depends on: Step 2.
- Proof: `cargo test -p liquers-lib --lib --tests`; `bash scripts/check-build-matrix.sh`.
- Rollback: revert.

### Step 5: integration tests
- Files / symbols: `liquers-lib/tests/value_summary.rs` (tests 5-9 of Phase 3, gated per feature).
- Depends on: Steps 3-4.
- Proof: `cargo test -p liquers-lib --test value_summary`.

### Step 6: documents and records
- Phase 2 §Documentation Architecture; set `affects_docs`; close `VALUE-DESCRIPTION` with a
  resolution note (model renamed, `kind`/`schema_version` dropped).
- Proof: `python3 scripts/docs_index.py && python3 scripts/docs_index.py --check`.

### Step 7: full checks
- `cargo test -p liquers-axum` (metadata JSON assertions may need the new key);
  `bash scripts/test-liquers-py.sh`; the `liquers-web` Node loop after `cargo clean`;
  `cargo fmt --check`; diff review.

## Testing Plan

Unit tests at Steps 1-3, the lib suite and matrix at Step 4, the integration file at Step 5, and the
dependent crates (axum, py, web) at Step 7.

## Rollback Plan

Steps 1-2 together, Steps 3-4 independently. Stored records carrying the field are harmless to a
reverted build except that it reads them as legacy metadata.

## Documentation Updates

As Step 6, with History rows and `reviewed:` bumps (§9.2).

## Phase 5 Entry Criteria

None — four-phase design.
