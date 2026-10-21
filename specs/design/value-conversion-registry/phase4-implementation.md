# Phase 4: Implementation Plan — value-conversion-registry

## Overview

Core registry and scalar edges first (testable alone), then environment plumbing, then library edges
extracted from the existing helpers, then the command. Prefer landing after
`value-interface-capability-split`; nothing here requires it. Assumes Phase 1's recommended answers.

## Progress

- [ ] Step 1: `liquers-core/src/conversion.rs` with classes, registry and scalar edges
- [ ] Step 2: `ValueInterface::conversions` and the core `Value` / `SimpleValue` lists
- [ ] Step 3: environment and builder plumbing
- [ ] Step 4: `ValueExtension::conversions`, `CombinedValue` lifting, `ExtValue` edges
- [ ] Step 5: the `convert` command and registry export
- [ ] Step 6: integration tests
- [ ] Step 7: documents, follow-up and records
- [ ] Step 8: full checks

## Implementation Steps

### Step 1: conversion module
- Files / symbols: `liquers-core/src/conversion.rs` (new) — `ConversionClass`, `ConversionPolicy`,
  `ConversionInput`, `ConvertFn`, `Conversion` (`new`, `map`), `ConversionRegistry`,
  `scalar_conversions`; `liquers-core/src/lib.rs` `pub mod conversion`.
- Depends on: none.
- Proof: `cargo test -p liquers-core conversion` — tests 3-6 of Phase 3.
- Rollback: delete the module.

### Step 2: value types declare edges
- Files / symbols: `liquers-core/src/value.rs` `ValueInterface::conversions` (default empty), core
  `Value` returns `scalar_conversions()`; `liquers-lib/src/value/simple.rs` the same.
- Depends on: Step 1.
- Proof: `cargo test -p liquers-core`; `ConversionRegistry::<Value>::from_value_type()` non-empty.
- Rollback: revert.

### Step 3: environment plumbing
- Files / symbols: `liquers-core/src/context.rs` `Environment::get_conversion_registry`,
  `GenericEnvironment` field and `assemble` parameter, `new_with_type_registry` seeds
  `from_value_type`, `EnvRef::get_conversion_registry`; `environment_builder.rs` field,
  `with_conversion_registry`, `build`; `liquers-core/tests/environment_builder.rs` `CustomEnvironment`.
- Depends on: Step 2.
- Proof: `cargo test -p liquers-core`.
- Rollback: revert Steps 1-3 together.

### Step 4: library edges
- Files / symbols: `liquers-lib/src/value/extended.rs` `ValueExtension::conversions<B>`,
  `CombinedValue::conversions` (lift base edges with `Conversion::map`); `value/mod.rs`
  `ExtValue::conversions` (gated per feature); extract `records_from_text_or_bytes` and
  `records_from_json` in `records/convert.rs` and `dataframe_from_text_or_bytes` in
  `polars/util.rs`, with `to_record` / `try_to_polars_dataframe` calling them.
- Depends on: Step 3.
- Proof: `cargo test -p liquers-lib --lib --tests` (existing `ns-rec`, `ns-pl` suites unchanged).
- Rollback: revert; the extractions are behaviour-neutral.

### Step 5: `convert` command
- Files / symbols: `liquers-lib/src/commands.rs` — `fn convert(state: &State<Value>, target: String,
  context: &Context<E>) -> Result<Value, Error>`; registration line in `register_core_commands!`.
- Depends on: Step 4.
- Proof: `cargo run -p liquers-lib --features cli --bin export-command-registry -- --format yaml -o
  specs/command_registry.yaml` plus a CHANGELOG line; `cargo test -p liquers-lib --test registry_export`;
  `liquers-validate -- '-R/data/sales.csv/-/convert-RecordView'` with the real registry.
- Rollback: remove the registration and regenerate.

### Step 6: integration tests
- Files / symbols: `liquers-lib/tests/value_conversion.rs` (tests 7-11 of Phase 3, gated).
- Depends on: Step 5.
- Proof: `cargo test -p liquers-lib --test value_conversion`.

### Step 7: documents and records
- Phase 2 §Documentation Architecture; set `affects_docs`; `VALUE-CONVERSION-CAPABILITY` stays open
  for the automatic half until `VALUE-NOT-CONVERTED-AUTOMATICALLY` is accepted, then is closed with
  a pointer to it (maintainer's call).
- Proof: `python3 scripts/docs_index.py && python3 scripts/docs_index.py --check`.

### Step 8: full checks
- `bash scripts/check-build-matrix.sh` (gated edges); `cargo test -p liquers-web --target
  wasm32-unknown-unknown --features debug-handles` after `cargo clean` (new `Environment` method,
  `MaybeSend` closure bounds on wasm); `bash scripts/test-liquers-py.sh`; diff review.

## Testing Plan

Steps 1-3 run core tests; Step 4 the lib suite (refactor neutrality); Steps 5-6 the registry export
and new integration tests; Step 8 the matrix and both bindings.

## Rollback Plan

Steps 1-2 are additive. Reverting Step 3 requires reverting 4-6. Each extraction in Step 4 can stay.

## Documentation Updates

As Step 7, with History rows and `reviewed:` bumps; `specs/command_registry.yaml` regenerated.

## Phase 5 Entry Criteria

None — four-phase design.
