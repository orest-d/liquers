# Phase 4: Implementation Plan — post-init-command-registration

## Overview

Built under question 1 (a), in compile-order: first the cloneable registry and the `Snapshot` type,
then the trait change with every call site fixed in one step (it does not compile in halves), then
the publishing slot and `update_commands`, then tests and documents. Prerequisite: the maintainer's
answer to question 1. If the answer is (b) or (c), this plan does not apply: (b) needs a revised
Phase 2, and (c) closes the issue as not planned.

## Progress

- [ ] Step 1: Record the decisions on questions 1-3
- [ ] Step 2: `Clone` for `CommandRegistry`
- [ ] Step 3: `Snapshot` and the trait accessors, all call sites
- [ ] Step 4: Publishing slot and `update_command_registry` on `GenericEnvironment`
- [ ] Step 5: `EnvRef::update_commands` with refresh and removal expiry
- [ ] Step 6: Integration tests
- [ ] Step 7: Workspace and wasm checks
- [ ] Step 8: Documents and records

## Implementation Steps

### Step 1: Record the decisions on questions 1-3
- Files / symbols: `phase1-high-level-design.md` Design Readiness
- Change: replace the open questions by the decisions; revise Phases 2-4 if any differs
- Depends on: none
- Proof: review
- Rollback: n/a

### Step 2: `Clone` for `CommandRegistry`
- Files / symbols: `liquers-core/src/commands.rs` `CommandRegistry`
- Change: hand-written `impl<E: Environment> Clone` (the maps clone their `Arc`s); unit test
  `clone_shares_executors`
- Depends on: Step 1
- Proof: `CARGO_INCREMENTAL=0 cargo test -p liquers-core --lib clone_shares_executors`
- Rollback: delete the impl

### Step 3: `Snapshot` and the trait accessors, all call sites
- Files / symbols: `liquers-core/src/context.rs` `Snapshot`, `Environment::{get_command_metadata_registry,
  get_command_executor}`, `EnvRef` wrappers, `GenericEnvironment` impl (still returns
  `Snapshot::Borrowed`), `tests/environment_builder.rs` `CustomEnvironment`; argument-passing call
  sites in core, lib, axum, mcp, agent, web, py, records, template (Phase 2 Integration Points)
- Change: signatures as in Phase 2; `&*x` or a local binding where a `&CommandMetadataRegistry`
  argument is expected
- Depends on: Step 1
- Proof: `cargo check --workspace --all-targets` is avoided (disk). Run per crate instead: `cargo
  check -p liquers-core --tests`, `-p liquers-lib --tests`, `-p liquers-axum --tests`, `-p
  liquers-mcp --tests`, `-p liquers-agent --tests`, `-p liquers-py`; wasm in Step 7. Unit test
  `snapshot_derefs_in_both_arms`
- Rollback: revert the commit. Behaviour is unchanged until Step 4

### Step 4: Publishing slot and `update_command_registry` on `GenericEnvironment`
- Files / symbols: `context.rs` `GenericEnvironment` fields `published`, `update_lock`, `Published`,
  `RegistryUpdate`, the provided trait method and its override
- Change: Phase 2 algorithm (clone, apply, `refresh_metadata_versions`, diff keys, swap)
- Depends on: Steps 2-3
- Proof: `cargo test -p liquers-core --test post_share_registration failed_update_publishes_nothing
  snapshot_taken_before_update_is_unchanged`
- Rollback: the slot stays `None` when no update is made, so earlier behaviour holds without it

### Step 5: `EnvRef::update_commands` with refresh and removal expiry
- Files / symbols: `context.rs` `EnvRef::update_commands`; uses
  `AssetManager::refresh_command_versions_and_expire` and `cascade_expire_dependents`
  (`assets.rs`)
- Change: async wrapper. For each `removed` key, cascade both `DependencyKey::for_command_metadata`
  and `for_command_implementation`
- Depends on: Step 4
- Proof: `cargo test -p liquers-core --test post_share_registration` (AC-1 to AC-4)
- Rollback: remove the method. Step 4 is still usable through the trait

### Step 6: Integration tests
- Files / symbols: `liquers-core/tests/post_share_registration.rs` (all Phase 3 tests)
- Change: new file
- Depends on: Step 5
- Proof: `CARGO_INCREMENTAL=0 cargo test -p liquers-core --test post_share_registration`
- Rollback: delete the file

### Step 7: Workspace and wasm checks
- Files / symbols: none
- Change: none. Run the regression suites (AC-7, AC-8)
- Depends on: Step 6
- Proof: `cargo test -p liquers-core --lib --tests`; `cargo test -p liquers-lib --lib --tests`;
  `cargo test -p liquers-mcp`; `cargo test -p liquers-agent`; `bash scripts/check-build-matrix.sh`;
  after `cargo clean`, `cargo test -p liquers-web --target wasm32-unknown-unknown --features
  debug-handles`; `bash scripts/test-liquers-py.sh`
- Rollback: n/a

### Step 8: Documents and records
- Files / symbols: `DOC_04_ENVIRONMENT_CONTEXT_EVALUATION.md`, `ASSETS.md`,
  `COMMAND_REGISTRATION_GUIDE.md` (History rows, `reviewed:`); `CLAUDE.md` Key Types if affected;
  `POST-INIT-COMMAND-REGISTRATION` closed; follow-up issue
  `LANGUAGE-INTEGRATIONS-REBUILD-ENVIRONMENT-ON-REGISTRATION` left open; this design `complete`
- Change: as listed
- Depends on: Step 7
- Proof: `python3 scripts/docs_index.py` then `python3 scripts/docs_index.py --check`
- Rollback: n/a

## Testing Plan

Unit tests after Steps 2 and 3. The integration file grows through Steps 4-6. The full regression
and wasm runs come once, in Step 7, because Step 3 touches nine crates. `specs/command_registry.yaml`
does not change, because no command signature changes.

## Rollback Plan

Each step is one commit. Steps 2-3 change no behaviour. Steps 4-5 are inert until
`update_commands` is called. Reverting Step 3 requires reverting 4-6 first.

## Documentation Updates

`affects_docs`: `specs/reference/api/DOC_04_ENVIRONMENT_CONTEXT_EVALUATION.md`,
`specs/reference/ASSETS.md`, `specs/guides/COMMAND_REGISTRATION_GUIDE.md`. Regenerate
`specs/index.csv`.

## Phase 5 Entry Criteria

None — four-phase design.
