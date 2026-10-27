# Phase 4: Implementation Plan — extended scalar value types

## Overview

This plan starts only after `value-interface-capability-split`, `value-type-definition-macro` and
`value-conversion-registry` have landed (step 1 checks that). It then adds the dependencies, the
scalar module, the declarations, the edges, the render arms, the tests and the documents.
Phase 1's recommended answers are assumed.

## Progress

- [ ] Step 1: prerequisites present
- [ ] Step 2: dependencies and the `ext-scalars` feature
- [ ] Step 3: `scalars.rs` — `ExtScalar`, its fifteen impls, generic hooks
- [ ] Step 4: fifteen declarations in `define_value_types!`
- [ ] Step 5: `From` / `TryFrom` impls
- [ ] Step 6: conversion edges
- [ ] Step 7: render arms in hand-written consumers
- [ ] Step 8: integration tests and the build matrix
- [ ] Step 9: documents and records
- [ ] Step 10: full checks and diff review

## Implementation Steps

### Step 1: prerequisites
- Files / symbols: confirm that `liquers_macro::define_value_types`,
  `liquers_core::conversion::{Conversion, ConversionClass}` and `ValueExtension::conversions`
  exist. If the macro's DSL differs from `value-type-definition-macro` Phase 2, update this
  design's Phase 2 declaration first.
- Depends on: those designs being merged.
- Proof: `cargo check -p liquers-lib`.
- Rollback: none (no change).

### Step 2: dependencies
- Files / symbols: `liquers-lib/Cargo.toml` gets `rust_decimal` (optional, default features) and
  `uuid` (optional, no `v4`), plus `ext-scalars = ["dep:rust_decimal", "dep:uuid"]` added to
  `default`.
- Depends on: step 1.
- Proof: `cargo check -p liquers-lib --no-default-features --features ext-scalars`.
- Rollback: revert the manifest.

### Step 3: scalar module
- Files / symbols: `liquers-lib/src/value/scalars.rs` (new, gated): `ExtScalar`, impls for the
  fifteen payload types, `encode`, `decode`, `as_base`, `json`, `duration_to_iso`,
  `duration_from_iso`. JSON and text are matched explicitly by data format, and anything else is
  the standard refusal. Add a `mod scalars;` line.
- Depends on: step 2.
- Proof: Phase 3 tests 3-7.
- Rollback: delete the module.

### Step 4: declarations
- Files / symbols: `liquers-lib/src/value/mod.rs`, the `define_value_types!` block, with fifteen
  entries per the Phase 2 table, each with `cfg: feature = "ext-scalars"` and `rust_type`.
- Depends on: step 3.
- Proof: Phase 3 tests 8 and 9; `ext_value_type_descriptions_complete` in both feature states.
- Rollback: remove the entries.

### Step 5: producers and extractors
- Files / symbols: `scalars.rs`, `From<T> for ExtValue`, `From<T> for Value` (except `f32`),
  `TryFrom<&ExtValue> for T`. Check that no `From<T> for Value` already exists for these types
  (only `f32` has one, on `SimpleValue`).
- Depends on: step 4.
- Proof: Phase 3 test 12.
- Rollback: revert.

### Step 6: conversion edges
- Files / symbols: `liquers-lib/src/value/mod.rs` `ValueExtension::conversions` for `ExtValue`,
  plus a gated `scalars::conversions()` holding the Phase 2 edge table.
- Depends on: steps 3 and 4.
- Proof: Phase 3 tests 10 and 11.
- Rollback: drop the extend call.

### Step 7: render arms
- Files / symbols: `liquers-lib/src/ui/web/html.rs`, `liquers-lib/src/egui/mod.rs` and
  `liquers-web/src/default_value.rs`, each with gated arms that show `to_text`. Find them with
  `cargo check` across the matrix: every non-exhaustive match is a compile error, which is
  intended.
- Depends on: step 4.
- Proof: `bash scripts/check-build-matrix.sh` (egui, webui and wasm rows).
- Rollback: revert.

### Step 8: integration tests and matrix
- Files / symbols: new `liquers-lib/tests/ext_scalars.rs` (tests 12 and 13), additions to
  `tests/value_type_system.rs` and `tests/value_conversion.rs`, and the two new `LIB_CONFIGS`
  rows in `scripts/check-build-matrix.sh`.
- Depends on: steps 4-7.
- Proof: the Phase 3 Test Plan commands.
- Rollback: revert.

### Step 9: documents and records
- Files / symbols: the three documents in the Phase 2 documentation table, each with History and
  `reviewed:`. Propose the `CLAUDE.md` feature-matrix line in the PR. Close
  `VALUE-EXTENDED-SCALAR-TYPES-MISSING`. Regenerate the index.
- Depends on: step 8.
- Proof: `python3 scripts/docs_index.py --check`.
- Rollback: revert.

### Step 10: full checks
- Run `cargo test -p liquers-lib --lib --tests`, `cargo test -p liquers-lib --no-default-features --lib --tests`
  and `bash scripts/check-build-matrix.sh`. Review the diff for `println!`, `unwrap`, `_ =>` over
  `ExtValue`, and hand-written arms inside the value module.

## Testing Plan

The Phase 3 table: unit tests in `scalars.rs`, integration tests in three files, and the matrix
rows for the feature-off and wasm32 cases.

## Rollback Plan

Every step is gated by `ext-scalars`. Turning the feature off restores today's build. Reverting
steps 3-7 removes the code, and no stored data depends on it.

## Documentation Updates

`specs/reference/VALUE_TYPE_SYSTEM.md` (bare set), `specs/guides/TYPE_SYSTEM_GUIDE.md`,
`specs/guides/SERIALIZATION_GUIDE.md`, and the `CLAUDE.md` proposal. The source issue is closed.

## Phase 5 Entry Criteria

Progress is complete, every Phase 3 test passes, the matrix including wasm32 is green, the bare-set
change is reviewed, and decisions 1-4 are recorded.
