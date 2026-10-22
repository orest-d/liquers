# Phase 4: Implementation Plan — value-type-definition-macro

## Overview

Prerequisite: `value-interface-capability-split` implemented (the generated `impl ValueType`).
Order: macro and its unit tests (no consumer yet) → move arm bodies into hook functions while the
hand-written impls still call them (behaviour proved unchanged) → switch `ExtValue` to the macro →
accessor call sites → new tests → documents. Assumes Phase 1's recommended answers.

## Progress

- [ ] Step 1: parser and model in `liquers-macro`
- [ ] Step 2: code generation in `liquers-macro`
- [ ] Step 3: move arm bodies into `liquers-lib/src/value/hooks.rs`
- [ ] Step 4: define `ExtValue` with `define_value_types!`
- [ ] Step 5: accessor call sites take `Arc<T>`
- [ ] Step 6: new integration tests
- [ ] Step 7: documents and issue records
- [ ] Step 8: full checks and diff review

## Implementation Steps

### Step 1: parser and model
- Files / symbols: `liquers-macro/src/value_types.rs` — `ValueTypesDecl`, `VariantDecl`,
  `impl syn::parse::Parse`; `liquers-macro/src/lib.rs` `define_value_types` (returns the parse
  result or its compile error).
- Change: Phase 2 §Interfaces DSL; validation of required keys, duplicates and unknown keys.
- Depends on: none.
- Proof: `cargo test -p liquers-macro value_types` — `parses_full_variant`, `rejects_bad_declarations`.
- Rollback: delete the module; nothing uses it yet.

### Step 2: code generation
- Files / symbols: `value_types.rs` `expand_enum`, `expand_value_type`, `expand_extension`,
  `expand_serializer`, `expand_accessors`, `expand_type_identified`.
- Change: the six generated items of Phase 2, every arm carrying its variant's `#[cfg]`.
- Depends on: Step 1.
- Proof: `cargo test -p liquers-macro`; a smoke expansion in a `liquers-lib` unit test module with a
  two-variant private enum (compiled, then removed in Step 4).
- Rollback: as Step 1.

### Step 3: hook functions
- Files / symbols: `liquers-lib/src/value/hooks.rs` (new) — functions named in Phase 2 §Integration
  Points; the current `ExtValue` impls in `mod.rs` call them.
- Change: move, do not alter, each arm body; gate each function like its type.
- Depends on: none (parallel with Steps 1-2).
- Proof: `cargo test -p liquers-lib --lib --tests` unchanged; `bash scripts/check-build-matrix.sh`.
- Rollback: revert the move.

### Step 4: `ExtValue` through the macro
- Files / symbols: `liquers-lib/src/value/mod.rs` — replace the enum, `impl ValueType`,
  `impl ValueExtension`, `impl DefaultValueSerializer`, `trait ExtValueInterface` and both impls
  with one `define_value_types!` block (nine variants as at HEAD).
- Depends on: Steps 2-3, and `value-interface-capability-split`.
- Proof: `cargo test -p liquers-lib --lib --tests`; `grep -c 'match self' liquers-lib/src/value/mod.rs` → 0.
- Rollback: restore the hand-written impls from git; Steps 1-3 stay useful.

### Step 5: accessor signatures
- Files / symbols: every `from_polars_dataframe(` call (29, `liquers-lib/src/polars/**`, tests,
  `liquers-py/src/value.rs`).
- Change: pass `Arc::new(df)`.
- Depends on: Step 4.
- Proof: `cargo test -p liquers-lib --lib --tests`; `bash scripts/test-liquers-py.sh`.
- Rollback: revert with Step 4.

### Step 6: new tests
- Files / symbols: `liquers-lib/tests/value_type_definition.rs` — `foreign_json_hook_is_delegated`,
  `rust_types_name_their_identifiers` (`#[cfg(feature = "polars")]` on the polars assertion).
- Depends on: Step 4.
- Proof: `cargo test -p liquers-lib --test value_type_definition`.
- Rollback: delete the file.

### Step 7: documents and records
- Phase 2 §Documentation Architecture; `specs/README.md` link; close
  `VALUE-TYPE-DEFINITION-MACRO` with a note that the scalars moved to
  `VALUE-EXTENDED-SCALAR-TYPES-MISSING`; close `TYPE-IDENTIFIED-IN-HAS-NO-IMPLEMENTATIONS` and
  `FOREIGN-VALUE-TEXT-AND-JSON-HOOKS-NEVER-CALLED` if still open; propose the `CLAUDE.md`
  §Adding a Value Type edit in the PR description.
- Proof: `python3 scripts/docs_index.py && python3 scripts/docs_index.py --check`.

### Step 8: full checks
- `bash scripts/check-build-matrix.sh` (all rows incl. wasm32); `cargo test -p liquers-web
  --target wasm32-unknown-unknown --features debug-handles` after `cargo clean` (its
  `default_value.rs` matches `ExtValue`); `cargo fmt --check`; diff review (no leftover smoke enum,
  registry YAML unchanged).

## Testing Plan

Steps 1-2 run macro unit tests; Step 3 proves the move is behaviour-neutral before the switch;
Step 4 re-runs the whole lib suite; Step 8 covers every feature row and the web crate.

## Rollback Plan

Steps 1-3 are additive and can land alone. Step 4 is the switch; reverting it restores the
hand-written impls with the hooks still in use.

## Documentation Updates

As Step 7; History rows and `reviewed:` bumps (§9.2).

## Phase 5 Entry Criteria

None — four-phase design.
