# Phase 4: Implementation Plan — Memory size estimate and byte budget

## Overview

Part A (steps 1-4) can land now and changes no behaviour. Part B (steps 5-7) requires
`asset-garbage-collection` to have landed (its `Registration<E>`, `CacheLimits`, `GcReport`,
`collect_garbage`, collector task). Phase 1 questions 1-3 assumed as recommended.

## Progress

- [ ] Step 1: Core trait method and core Value
- [ ] Step 2: Records — Column buffer bytes and RecordView estimates
- [ ] Step 3: Lib — SimpleValue, ExtValue, CombinedValue, ValueExtension, ForeignValue
- [ ] Step 4: Build matrix for part A
- [ ] Step 5: Options and CacheLimits byte fields
- [ ] Step 6: Measurement cache and byte step in collect_garbage; triggers
- [ ] Step 7: Tests, documentation and issue closure

## Implementation Steps

### Step 1: Core trait method and core Value
- Files / symbols: `liquers-core/src/value.rs` `ValueInterface::estimated_memory_size`
  (provided), `impl ValueInterface for Value` override (explicit `match`).
- Depends on: none.
- Proof: `core_value_estimates`; `cargo test -p liquers-core --lib --tests`.
- Rollback: revert.

### Step 2: Records — Column buffer bytes and RecordView estimates
- Files / symbols: `liquers-records/src/column.rs` `Column::buffer_bytes`; `batch.rs`
  `RecordView::estimated_memory_size` (provided), `RecordBatch` override; `mutable.rs`
  `RecordBatchMut`; `views.rs` forwarding views.
- Depends on: none.
- Proof: `record_batch_estimate_covers_buffers`; `cargo test -p liquers-records --all-features
  --lib --tests`.
- Rollback: revert.

### Step 3: Lib — value overrides
- Files / symbols: `liquers-lib/src/value/extended.rs` (`ValueExtension` provided method,
  `CombinedValue` forward), `foreign.rs` (`ForeignValue` provided method), `simple.rs`
  (`SimpleValue`), `mod.rs` (`ExtValue`, every `#[cfg]` arm). Confirm `DataFrame::estimated_size`
  and `DynamicImage::as_bytes` at the pinned `polars` / `image` versions (`cargo doc -p polars-core`
  or the registry sources); if a name differs, use the equivalent and note it in `VALUE_TYPE_SYSTEM.md`.
- Depends on: steps 1-2.
- Proof: `ext_value_estimates`, `simple_value_estimates`, `combined_and_foreign_forward`;
  `cargo test -p liquers-lib --lib --tests`.
- Rollback: revert.

### Step 4: Build matrix for part A
- Proof: `bash scripts/check-build-matrix.sh` (a `match` over `ExtValue` changed).
- Rollback: n/a.

### Step 5: Options and CacheLimits byte fields
- Files / symbols: `liquers-core/src/environment_builder.rs` (`max_cached_key_bytes`,
  `max_cached_query_bytes`, `with_*`, zero check); `assets.rs` `CacheLimits` fields,
  `is_unbounded`, both managers' `with_policies`.
- Depends on: `asset-garbage-collection` landed.
- Proof: `byte_limits_round_trip_and_zero_rejected`.
- Rollback: revert.

### Step 6: Measurement cache and byte step; triggers
- Files / symbols: `assets.rs` `Registration::measured`, `Registration::measure`, the byte step in
  both managers' `collect_garbage`, `GcReport` fields, the insert trigger when a byte limit is set,
  the 10 s sweep default; unit test `measure_cache_invalidates_on_new_value`.
- Depends on: step 5.
- Proof: `cargo test -p liquers-core --lib measure_ gc_`; existing `asset_gc` tests unchanged.
- Rollback: revert; count and idle limits keep working.

### Step 7: Tests, documentation and issue closure
- Files: `liquers-core/tests/asset_gc.rs` (gc09-gc11), `liquers-lib/tests/asset_gc_bytes.rs`
  (gc12, gc13); the five `affects_docs` documents (History row, `reviewed:` bump each); close
  `VALUE-HAS-NO-MEMORY-SIZE-ESTIMATE` with a resolution note.
- Proof: `CARGO_INCREMENTAL=0 cargo test -p liquers-core --test asset_gc`;
  `cargo test -p liquers-lib --test asset_gc_bytes`; `python3 scripts/docs_index.py --check`.

## Testing Plan

Per step as above. Final: `cargo test -p liquers-lib --lib --tests`, `cargo test -p
liquers-records --all-features --lib --tests`, `bash scripts/check-build-matrix.sh`, and after
`cargo clean` the `liquers-web` Node loop (`--features debug-handles`, and `,records`), because
the inline manager's collector and `ExtValue` changed.

## Rollback Plan

Part B (steps 5-7) reverts alone; part A is additive and unused by anything else.

## Documentation Updates

`reference/VALUE_TYPE_SYSTEM.md`, `reference/ASSETS.md`, `reference/ENVIRONMENT_CONFIG.md`,
`guides/TYPE_SYSTEM_GUIDE.md`, `guides/ENVIRONMENT_CONSTRUCTION_GUIDE.md` (History, `reviewed:`);
`specs/index.csv` regenerated; no `command_registry.yaml` change.

## Phase 5 Entry Criteria

None — four-phase design.
