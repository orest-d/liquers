# Phase 4: Implementation Plan — Metadata records in a database

## Overview

The store first, proved by the plain and `A3`-style suites; then, once `central-storage-model` is
implemented, the central suite and the documents. Prerequisites: decision Q1 (engine) before step
1; `central-storage-model` before step 5.

## Progress

- [ ] Step 1: dependency and feature
- [ ] Step 2: `AsyncSqliteStore` data-only methods
- [ ] Step 3: ordinary `AsyncStore` methods
- [ ] Step 4: factory and declaration
- [ ] Step 5: conformance suites
- [ ] Step 6: build matrix row
- [ ] Step 7: documents and records

## Implementation Steps

### Step 1: dependency and feature
- Files / symbols: `liquers-store/Cargo.toml` (`sqlx` optional, `sqlite-store` feature, tokio
  `sync`); `src/lib.rs` module declaration
- Depends on: Q1
- Proof: `cargo check -p liquers-store --features sqlite-store`
- Rollback: revert the manifest

### Step 2: `AsyncSqliteStore` data-only methods
- Files / symbols: `liquers-store/src/sqlite_store.rs` `AsyncSqliteStore::open`, lazy schema,
  `get_data`, `set_data`, `remove_data`, `rename_data`, `listdir_data`, `data_facts`,
  `get_stored_metadata`, `data_capabilities`
- Depends on: 1
- Proof: `cargo test -p liquers-store --features sqlite-store --lib sqlite_` (listing, literal keys,
  renames, facts, capabilities)
- Rollback: remove the module

### Step 3: ordinary `AsyncStore` methods
- Files / symbols: same file: `get`, `set`, `set_metadata`, `remove`, `removedir`, `is_dir`,
  `listdir`, `contains`, `keys`, `is_supported`, `key_prefix`, `store_name`
- Depends on: 2
- Proof: `sqlite_record_only_key_is_enumerable`, `sqlite_rewrite_touches_one_row`
- Rollback: as step 2

### Step 4: factory and declaration
- Files / symbols: `sqlite_store.rs` `SqliteStoreFactory` (declared in both feature states);
  `store_factory.rs` `default_store_factory`; tests `sqlite_type_*`, `availability02_…`,
  `default01_…`
- Depends on: 3
- Proof: `cargo test -p liquers-store --lib store_factory` with and without `sqlite-store`
- Rollback: unchain the factory

### Step 5: conformance suites
- Files / symbols: `liquers-store/tests/store_conformance_CONF.rs` `c8_sqlite`,
  `a3_sqlite_per_entry`, `a5_sqlite_central`, `status_table` rows
- Depends on: 4; `a5_sqlite_central` also on `central-storage-model` implemented
- Proof: `cargo test -p liquers-store --features sqlite-store,store-conformance --test store_conformance_CONF`
- Rollback: drop the suites (a failing rule is a defect: fix it in steps 2-3)

### Step 6: build matrix row
- Files: `scripts/check-build-matrix.sh` — `liquers-store --features sqlite-store` (lib and tests)
- Depends on: 4
- Proof: `bash scripts/check-build-matrix.sh`
- Rollback: remove the row

### Step 7: documents and records
- Files: `STORE_CONFIG_FSD.md`, `STORE_SEMANTICS.md` §11 and §13, `STORE_CONFIG_GUIDE.md` §9,
  `STORE_IMPLEMENTATION_GUIDE.md` §9 (History rows, `reviewed:`); close
  `STORE-NO-DATABASE-STORAGE-MODEL` with a resolution note recording the dropped `PreparedWrite`
  half (Q2), or file it separately if the decision keeps it
- Depends on: 5
- Proof: `python3 scripts/docs_index.py --check`
- Rollback: revert the documents

## Testing Plan

Per-step proofs; then `cargo test -p liquers-store --lib --tests` (default features: the type is
declared unavailable), the same with `sqlite-store,store-conformance`, and the build matrix.
`liquers-lib` and `liquers-axum` are unaffected unless they enable the feature.

## Rollback Plan

The feature is off by default, so a partly landed change is invisible to default builds; removing
the module and the factory line restores HEAD.

## Documentation Updates

The four documents of Phase 2; `specs/index.csv` by the maintainer's index run. No
`specs/command_registry.yaml` change.

## Phase 5 Entry Criteria

None — four-phase design.
