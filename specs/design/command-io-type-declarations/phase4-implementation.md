# Phase 4: Implementation Plan — commands declare the value types they accept and produce

## Overview

Core contract first (steps 1-3), then the macro (4), then the declarations and their export (5-6),
then bindings and documents (7-8). Nothing outside this design must land first.

## Progress

- [ ] Step 1: `TypeSet`
- [ ] Step 2: metadata fields, alias copy and registry queries
- [ ] Step 3: plan-time type tracking and warning
- [ ] Step 4: `register_command!` statements and attribute
- [ ] Step 5: declarations on `ns-img`, `ns-pl`, `ns-rec`
- [ ] Step 6: regenerate `specs/command_registry.yaml`
- [ ] Step 7: Python getters
- [ ] Step 8: documentation and final checks

## Implementation Steps

### Step 1: `TypeSet`
- Files / symbols: `liquers-core/src/type_system.rs` — `TypeSet`, private `TypeSetRepr`, methods of
  Phase 2; serde `try_from`/`into`.
- Depends on: none.
- Proof: `cargo test -p liquers-core --lib type_system` — test 3.
- Rollback: revert; nothing uses it yet.

### Step 2: metadata fields, alias copy and registry queries
- Files / symbols: `command_metadata.rs` — `ArgumentInfo::accepts`, `with_accepts`;
  `CommandMetadata::produces`, `with_produces`, `with_accepts`, `accepts_input`; the `Default`
  impl and the literals in `new` / `from_key`; `CommandMetadataRegistry::commands_accepting`,
  `check_declared_types` (iterate `commands`, exhaustive over nothing new).
  `commands.rs` — `register_alias` copies `produces`.
- Depends on: 1.
- Proof: `cargo test -p liquers-core --lib command_metadata` — tests 4, 5, 9, 10; the existing
  `omitted_state_argument_matches_constructors` and declaration tests stay green.
- Rollback: revert; fields default to any.

### Step 3: plan-time type tracking and warning
- Files / symbols: `plan.rs` — `PlanBuilder::flowing`; reset in `process_resource_query` and the
  `q` / `UseQueryValue` path of `process_query`; check and update in `process_action` for both the
  `Registered` and `Alias` arms (the alias's metadata already carries the target's types after
  step 2).
- Depends on: 2.
- Proof: `cargo test -p liquers-core --lib plan` — tests 6-8; existing plan tests unchanged (no
  command in them declares types).
- Rollback: remove the field; the warning is the only effect.

### Step 4: `register_command!` statements and attribute
- Files / symbols: `liquers-macro/src/registration.rs` — `CommandSignatureStatement` gains
  `Accepts(Vec<String>)` / `Produces(Vec<String>)`; the parameter-metadata parser gains `accepts`;
  code generation emits `cm.with_accepts(TypeSet::try_from_identifiers([..])?)`,
  `cm.with_produces(…)` and `accepts: TypeSet::try_from_identifiers([..])?` in the `ArgumentInfo`
  literal; compile errors of Phase 3.
- Depends on: 2.
- Proof: `cargo test -p liquers-macro` — test 11; `cargo test -p liquers-lib --lib --tests` unchanged.
- Rollback: revert the macro; no caller uses the statements yet.

### Step 5: declarations on `ns-img`, `ns-pl`, `ns-rec`
- Files / symbols: `liquers-lib/src/image/commands.rs` (all `register_command!` calls),
  `liquers-lib/src/polars/` registrations, `liquers-lib/src/records/` `ns-rec` registrations — add
  `accepts:` / `produces:` per command after reading its input helper (`try_to_image`,
  `try_to_polars_dataframe`, `to_record`) and its result. New test file
  `liquers-lib/tests/declared_command_types.rs` (gated on `image-support`, `polars`, `records`):
  tests 12 and 13.
- Depends on: 4.
- Proof: `cargo test -p liquers-lib --test declared_command_types`; `bash scripts/check-build-matrix.sh`.
- Rollback: revert the statements; each is independent.

### Step 6: regenerate `specs/command_registry.yaml`
- Files / symbols: run `cargo run -p liquers-lib --features cli --bin export-command-registry --
  --format yaml -o specs/command_registry.yaml`; add a dated line between the CHANGELOG markers.
- Depends on: 5.
- Proof: `cargo test -p liquers-lib --test registry_export` (test 14); then the Example 2 query with
  the rebuilt `liquers-validate` reports `Warning`.
- Rollback: regenerate from the reverted tree.

### Step 7: Python getters
- Files / symbols: `liquers-py/src/command_metadata.rs` — `#[getter] accepts` on the argument
  wrapper, `#[getter] produces` on the metadata wrapper, both JSON strings like `argument_type`;
  conformance row in `liquers-py/README.md`.
- Depends on: 2.
- Proof: `bash scripts/test-liquers-py.sh`.
- Rollback: revert; the Rust side is independent.

### Step 8: documentation and final checks
- Files / symbols: the four documents in Phase 2's table, each with a History row and `reviewed:`;
  issue `COMMAND-INPUT-OUTPUT-TYPES-ARE-UNDECLARED` status per `DOCS_STRUCTURE_GUIDE.md` §4.3.
- Depends on: 1-7.
- Proof: `python3 scripts/docs_index.py --check`; final diff review for scope and stray output.

## Testing Plan

Focused: the per-step commands. Crate: `cargo test -p liquers-core`, `cargo test -p liquers-macro`,
`cargo test -p liquers-lib --lib --tests`. Matrix: `bash scripts/check-build-matrix.sh`. Bindings:
`bash scripts/test-liquers-py.sh`. Every scenario AC-1…AC-8 has a test in Phase 3.

## Rollback Plan

Each step reverts independently in reverse order. Data: none persisted except command metadata in
caches; reverting restores the former `metadata_version` values, which recomputes affected results
once more.

## Documentation Updates

`REGISTER_COMMAND_FSD.md`, `COMMAND_DECLARATION.md`, `VALUE_TYPE_SYSTEM.md`,
`COMMAND_REGISTRATION_GUIDE.md` (step 8); `specs/command_registry.yaml` (step 6);
`liquers-py/README.md` (step 7).

## Phase 5 Entry Criteria

Not applicable (no `workflow` marker). Done when every step's proof passes, the decisions in Phase 1
are recorded as made, and `docs_index.py --check` is clean.
