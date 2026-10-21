# Phase 4: Implementation Plan — Configuring a store from a URI

## Overview

Core first (field, declaration, resolution, errors, core schemes), then the OpenDAL half, then
documents. Prerequisites: decisions Q1, Q2 and Q6 (Phase 1). Nothing outside `liquers-core` and
`liquers-store` changes.

## Progress

- [ ] Step 1: `uri` field, `StoreUri`, variable expansion
- [ ] Step 2: `uri_schemes` and scheme resolution
- [ ] Step 3: exclusivity and the unclaimed-scheme error
- [ ] Step 4: core schemes `file` and `memory`
- [ ] Step 5: OpenDAL resolution and construction
- [ ] Step 6: documents and records

## Implementation Steps

### Step 1: `uri` field, `StoreUri`, variable expansion
- Files / symbols: `liquers-core/src/store_config.rs` `StoreConfig::uri`, `with_uri`, `uri_scheme`,
  `StoreUri::parse`, `StoreConfig::expand_env_vars`, private `expand_env_vars_uri_encoded`
- Depends on: Q1, Q6
- Proof: `cargo test -p liquers-core --lib store_config::tests::uri_`
- Rollback: remove the field and helpers

### Step 2: `uri_schemes` and scheme resolution
- Files / symbols: `liquers-core/src/store_factory.rs` `StoreTypeInfo::uri_schemes`,
  `with_uri_scheme`, default `StoreFactory::resolve`, `StoreTypeMap::resolve`
- Depends on: 1
- Proof: `uri_scheme_first_in_chain_wins`; `resolve01`–`resolve04` unchanged
- Rollback: restore the exact-match rule

### Step 3: exclusivity and the unclaimed-scheme error
- Files / symbols: same file: `check_type_or_uri` called in `StoreRouterBuilder::build_router`;
  `unknown_uri_scheme_error` in `ChainedStoreFactory::create`
- Depends on: 2
- Proof: `uri_and_type_are_exclusive`, `entry_needs_type_or_uri`,
  `uri_unclaimed_scheme_lists_schemes`, `uri_errors_omit_the_query`
- Rollback: remove the check

### Step 4: core schemes `file` and `memory`
- Files / symbols: `core_store_factory` (both `filesystem` declarations, native and wasm32, and
  `memory`), the filesystem constructor
- Depends on: 2
- Proof: `file_uri_builds_filesystem`, `memory_uri_builds_memory`,
  `file_uri_must_be_local_and_absolute`; `cargo check -p liquers-core --target wasm32-unknown-unknown`
- Rollback: drop the declarations

### Step 5: OpenDAL resolution and construction
- Files / symbols: `liquers-store/src/store_factory.rs` `service_uri_scheme`,
  `OpendalStoreFactory::type_info`, `resolve`, `create`, new `operator_from_uri` (one cfg-gated arm
  per `service_arguments` arm)
- Depends on: 2, 3; Q2
- Proof: `cargo test -p liquers-store --lib store_factory` (AC-1, AC-6, AC-7 tests), and
  `bash scripts/check-build-matrix.sh` (every service feature state)
- Rollback: revert the factory changes

### Step 6: documents and records
- Files: `STORE_CONFIG_FSD.md`, `STORE_CONFIG_GUIDE.md`, `STORE_IMPLEMENTATION_GUIDE.md` (History
  rows, `reviewed:`); close `STORE-CONFIG-FROM-URI` with a resolution
- Depends on: 5
- Proof: `python3 scripts/docs_index.py --check`
- Rollback: revert the documents

## Testing Plan

Per-step proofs, then `cargo test -p liquers-core --lib`, `cargo test -p liquers-store --lib
--tests`, the build matrix (wasm32 row for core's declarations) and
`cargo test -p liquers-lib --lib --tests` as a smoke test of configuration loading.

## Rollback Plan

Every step is additive behind an optional field, so reverting a step leaves `type:` documents
working; no persisted data or configuration needs migrating.

## Documentation Updates

The three documents of Phase 2. No `specs/command_registry.yaml` change; `specs/index.csv` by the
maintainer's index run.

## Phase 5 Entry Criteria

None — four-phase design.
