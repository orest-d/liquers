# Phase 4: Implementation Plan — value-interface-capability-split

## Overview

One PR, because a trait change cannot land halfway. Order: core trait first, then each implementor
crate outward (core → lib → py → web), then documents. Assumes Phase 1's recommended answers.
No prerequisite; `core-value-enum-oversized` may land before or after (rebase the `Value` impl).

## Progress

- [ ] Step 1: add `ValueType` and `TypeInfo::find` to `liquers-core`
- [ ] Step 2: move core `Value` onto `ValueType`, rename `identifier` in core
- [ ] Step 3: move `SimpleValue`, `ValueExtension`, `ExtValue`, `CombinedValue` and `ForeignValue`
- [ ] Step 4: update `liquers-py`
- [ ] Step 5: update `liquers-web`
- [ ] Step 6: add the new tests
- [ ] Step 7: documents
- [ ] Step 8: full checks and diff review

## Implementation Steps

### Step 1: add `ValueType` and `TypeInfo::find`
- Files / symbols: `liquers-core/src/type_system.rs` — new `ValueType`, `TypeInfo::find`;
  `TypeRegistry::from_value_type` bound `V: ValueType`.
- Change: signatures as in Phase 2 §Interfaces.
- Depends on: none.
- Proof: `cargo check -p liquers-core` (fails until Step 2 — run Steps 1-2 together).
- Rollback: revert the file.

### Step 2: core `Value` onto `ValueType`
- Files / symbols: `liquers-core/src/value.rs` `ValueInterface` (delete `identifier`, `type_name`,
  `default_extension`, `default_data_format`, `default_filename`, `default_media_type`,
  `type_info`, `type_descriptions`, `supports_data_format`; add supertrait `ValueType`; replace the
  head TODO by a doc paragraph on why `DefaultValueSerializer` stays a supertrait); `impl ValueType
  for Value` with a `static OnceLock` of descriptions; delete the four fact matches. Rename every
  `.identifier()` in `liquers-core/src` and `liquers-core/tests` to `.type_identifier()`; add
  `use crate::type_system::ValueType` where a concrete type calls it.
- Depends on: Step 1.
- Proof: `CARGO_INCREMENTAL=0 cargo test -p liquers-core` — `type_descriptions_match_identifier`,
  `every_default_is_in_supported_formats`, `supports_data_format_agrees_with_the_registry` pass.
- Rollback: revert Steps 1-2.

### Step 3: `liquers-lib` implementors
- Files / symbols: `liquers-lib/src/value/simple.rs` (as Step 2); `value/extended.rs`
  `ValueExtension` (delete 7 methods, supertrait `ValueType`), `CombinedValue` `impl ValueType`
  (two arms), test extensions `RefusingExtension`, `ScalarExtension`, `DeclaringExtension`;
  `value/mod.rs` `impl ValueType for ExtValue` (delete `type_name`, three `default_*` matches and
  `default_ext_type_info`); `value/foreign.rs` `ForeignValue` (delete `identifier`, three
  `default_*`, the `type_info` default; supertrait `ValueType`) and its test impls; rename all
  `identifier()` calls in `liquers-lib/src` and `liquers-lib/tests`.
- Depends on: Step 2.
- Proof: `CARGO_INCREMENTAL=0 cargo test -p liquers-lib --lib --tests`.
- Rollback: revert the crate's changes with Steps 1-2.

### Step 4: `liquers-py`
- Files / symbols: `liquers-py/src/value.rs` `impl ForeignValue for PyObjectValue` → `impl
  ValueType` (three methods) + `ForeignValue` remainder; renamed call sites.
- Depends on: Step 3.
- Proof: `bash scripts/test-liquers-py.sh` (`py_type_info_agrees_with_instance` passes).
- Rollback: revert the file.

### Step 5: `liquers-web`
- Files / symbols: `liquers-web/src/value.rs` `JsOpaque`; `src/lib.rs` getter `identifier` →
  `#[wasm_bindgen(getter, js_name = typeIdentifier)] pub fn type_identifier`; `src/bridge.rs`;
  `tests/second_value_type.rs` `MatrixExt` (`type_identifier`, `type_info`); the generated `.d.ts`
  and any e2e test reading `.identifier`.
- Depends on: Step 3.
- Proof: after `cargo clean`: `cargo test -p liquers-web --target wasm32-unknown-unknown --features
  debug-handles` (and with `,records`); `./liquers-web/scripts/check-stubs.sh --build`.
- Rollback: revert the crate's changes; Steps 1-4 do not depend on it.

### Step 6: new tests
- Files / symbols: the four new tests named in Phase 3 §Test Plan.
- Depends on: Steps 2-3.
- Proof: `cargo test -p liquers-core value_type_defaults find_falls_back state_metadata_carries`;
  `cargo test -p liquers-lib --test value_type_system --test foreign_value_registration`.
- Rollback: delete the tests.

### Step 7: documents
- Files / symbols: Phase 2 §Documentation Architecture; set `affects_docs` in `DESIGN.md`; update
  `specs/issues/CORE-VALUE-INTERFACE-CAPABILITY-SPLIT.md` to `closed` with a resolution note.
- Depends on: Steps 1-6.
- Proof: `python3 scripts/docs_index.py && python3 scripts/docs_index.py --check`.
- Rollback: revert the documents.

### Step 8: full checks
- `bash scripts/check-build-matrix.sh` (the deleted cfg-gated arms); `cargo check -p liquers-axum
  -p liquers-mcp -p liquers-agent`; `cargo fmt --check` on touched files; diff review for stray
  debug output, unrelated edits, and that `specs/command_registry.yaml` is unchanged.

## Testing Plan

Steps 2-3 run the crate suites as they go; Step 6 adds the new tests; Step 8 runs the matrix and
the bindings. No `register_command!` signature changes, so the registry export test stays green.

## Rollback Plan

Revert the PR. Because the trait change is atomic, there is no partial state to repair.

## Documentation Updates

As Step 7. History rows and `reviewed:` bumps in the four affected documents (§9.2).

## Phase 5 Entry Criteria

None — four-phase design.
