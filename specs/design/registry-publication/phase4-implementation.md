# Phase 4: Implementation Plan — publishing registries across realms

## Overview

Core first (steps 1-2), then the server route (step 3), then the page (step 4), documents (step 5),
and the type import once `realm-type-registry` has landed (step 6). The answers to Phase 1
questions 1-2 are recorded there before step 1. Steps 1-5 need no other design.

## Progress

- [ ] Step 1: `published`, `import_realm`, `remove_realm`
- [ ] Step 2: imported command plans, has no executor
- [ ] Step 3: `RegistryApiBuilder` and handlers
- [ ] Step 4: `importRealm` in `liquers-web`
- [ ] Step 5: documents
- [ ] Step 6: type import (after `realm-type-registry`)

## Implementation Steps

### Step 1: core registry methods
- Files / symbols: `liquers-core/src/command_metadata.rs` `CommandMetadataRegistry::{published,
  import_realm, remove_realm}`. `import_realm` builds the relabelled list first (clone, resolve,
  set `realm`, retarget `CommandDefinition::Alias` — exhaustive `match`), checks every refusal,
  then calls `add_command`.
- Proof: tests 3-6 — `cargo test -p liquers-core --lib import_realm`.
- Rollback: revert; no caller yet.

### Step 2: environment behaviour
- Files / symbols: `liquers-core/tests/environment_builder.rs` test 7 only; no source change
  expected (execution already fails without an executor). If the error does not name the command,
  fix the message in `commands.rs` `CommandRegistry::execute_async`.
- Depends on: step 1. Proof: test 7.

### Step 3: server route
- Files / symbols: new `liquers-axum/src/registry/mod.rs`, `builder.rs` (`RegistryApiBuilder`,
  `join_base` as the other builders), `handlers.rs` (`commands_handler`, `types_handler`, a private
  `etag_response(body, headers)`); `liquers-axum/src/lib.rs` `pub mod registry` and re-export.
  No `unwrap`/`expect` (`cargo clippy -p liquers-axum --no-deps -- -D clippy::unwrap_used -D
  clippy::expect_used`).
- Depends on: step 1. Proof: tests 8-10 — `cargo test -p liquers-axum registry`.
- Rollback: remove the module; nothing else references it.

### Step 4: page API
- Files / symbols: `liquers-web/src/environment.rs` `IMPORTED_REALMS`, `ImportedRealm`,
  `import_realm_on`, application in `new_environment`; `liquers-web/src/lib.rs` the module-level
  `importRealm` binding;
  refusal when a retained JavaScript command uses the realm; the TypeScript stub.
- Depends on: step 1. Proof: test 11 and `./liquers-web/scripts/check-stubs.sh --build`.
- Containment: like `configure_store_on`, retain-then-apply with restore on failure.

### Step 5: documents
- `WEB_API_SPECIFICATION.md` (new Registry API section, §1, §8, Appendix B), `REALMS.md`
  ("Imported realms"), `liquers-web/README.md`; History rows and `reviewed:` bumps.

### Step 6: type import
- Depends on: `realm-type-registry` steps 2-3 (`TypeRegistry` serde, `import_realm`).
- Files / symbols: `liquers-web/src/environment.rs` applies `ImportedRealm::types` with
  `TypeRegistry::import_realm` in `new_environment`; the interim "not yet available" error is
  removed. Proof: test 12.

## Testing Plan

`CARGO_INCREMENTAL=0 cargo test -p liquers-core --lib --tests`; `cargo test -p liquers-axum`;
`cargo clean` then the liquers-web Node loop with and without `records`, and `check-stubs.sh`;
`bash scripts/check-build-matrix.sh` (wasm32 row for core). `specs/command_registry.yaml` does
not change.

## Rollback Plan

Each step is additive and independent of the others except through step 1. Removing the router
or the page API leaves core's methods unused but harmless.

## Documentation Updates

As step 5; then close `REGISTRIES-NOT-PUBLISHED-ACROSS-REALMS`, set this design `complete`, run
`python3 scripts/docs_index.py` and `--check`. Tell `multi-realm-dispatch` (Phase 1 question 4)
that its remote-metadata question is answered.

## Phase 5 Entry Criteria

Steps 1-5 done with their tests green; step 6 either done or explicitly deferred with
`realm-type-registry` still open; Phase 1 questions 1-2 answered.
