# Phase 4: Implementation Plan — slice 1, the remote store

## Overview

Feature and module skeleton first, then the read path, the write path, the factory, the
conformance suite, documentation. Prerequisites: none (questions 1-2 resolved as recommended; if
question 1 picks a new crate, steps 1 and 4 move to it unchanged in substance).

## Progress

- [ ] Step 1: `client` feature and module skeleton
- [ ] Step 2: read path and error mapping
- [ ] Step 3: write and directory path
- [ ] Step 4: `api_store_factory`
- [ ] Step 5: conformance and integration tests
- [ ] Step 6: documentation and final checks

## Implementation Steps

### Step 1: `client` feature and module skeleton
- Files / symbols: `liquers-axum/Cargo.toml` (`client = ["dep:reqwest"]`, optional `reqwest`
  0.12 with `rustls-tls`, no default features; dev-dependency `liquers-core` with
  `store-conformance`); `liquers-axum/src/lib.rs` (`#[cfg(feature = "client")] pub mod client;`);
  `liquers-axum/src/client/mod.rs` (`ApiStoreConfig`, `ApiStore`, `ApiStore::new`, private
  `url_for(route, &Key) -> Result<String, Error>` doing prefix removal and per-segment encoding).
- Depends on: none.
- Proof: `cargo check -p liquers-axum --features client`; `cargo check -p liquers-axum`.
- Rollback: remove the feature and module.

### Step 2: read path and error mapping
- Files / symbols: `client/mod.rs` — `impl AsyncStore for ApiStore`: `get`, `get_bytes`,
  `get_metadata`, `contains`, `is_dir`, `listdir`, `keys`, `is_supported`, `store_name`,
  `key_prefix`; private `decode_error(status, body, &Key, read: bool) -> Error`.
- Change: per the Phase 2 mapping; `DataEntry` decoded with `ciborium`; envelope `ApiResponse`.
- Depends on: step 1.
- Proof: tests `server_errors_keep_their_type`, `unreachable_server_is_read_or_write_error`,
  `keys_map_under_the_mount_prefix` (read half).
- Rollback: revert the file.

### Step 3: write and directory path
- Files / symbols: `client/mod.rs` — `set`, `set_metadata`, `remove`, `removedir`, `makedir`.
- Depends on: step 2 (shared request and error helpers).
- Proof: `set_then_get_round_trips_data_and_metadata`, `keys_map_under_the_mount_prefix`.
- Rollback: revert; reads keep working.

### Step 4: `api_store_factory`
- Files / symbols: `client/mod.rs` `pub fn api_store_factory() -> StoreTypeMap` with
  `StoreTypeInfo::new("liquers_api")` and arguments `url`, `headers`, `timeout_ms`.
- Depends on: step 1.
- Proof: `factory_refuses_missing_url_and_metadata_section`.
- Rollback: remove the function; `ApiStore::new` remains usable.

### Step 5: conformance and integration tests
- Files: `liquers-axum/tests/remote_store_conformance.rs` (Phase 3 test plan).
- Depends on: steps 2-4.
- Proof: `cargo test -p liquers-axum --features client --test remote_store_conformance`; every
  `AllowedFailure` (if any) has a reason and a filed issue.
- Rollback: delete the file.

### Step 6: documentation and final checks
- Files: `WEB_API_SPECIFICATION.md` (§"Client", Appendix B), `STORE_CONFIG_FSD.md` (`liquers_api`),
  `STORE_IMPLEMENTATION_GUIDE.md` §5 suite list — each with History row and `reviewed:` bump;
  `CLAUDE.md` "Adding a Store Backend" needs no change (not OpenDAL).
- Issue `NO-REMOTE-STORE-OR-ASSET-MANAGER`: stays open (slices 2-3 remain) with a progress note;
  closed when `REMOTE-ASSET-MANAGER-AND-SYSTEM-API` and `WEB-HAS-NO-READ-WRITE-REMOTE-STORE` take
  the remainder, as the issue's resolution says.
- Checks: `cargo test -p liquers-axum`; `cargo test -p liquers-axum --features client`;
  `cargo clippy -p liquers-axum --features client --no-deps -- -D clippy::unwrap_used -D
  clippy::expect_used`; `bash scripts/check-build-matrix.sh` (new optional dependency; add a
  `liquers-axum --features client` row if the script has no axum rows — say so in the PR);
  `python3 scripts/docs_index.py` and `--check`; final diff review.
- Depends on: step 5.

## Testing Plan

Steps 2-4 each run the subset of `remote_store_conformance` they make pass; step 5 runs the file;
step 6 the crate suites with and without the feature. Default builds (without `client`) must not
compile `reqwest` into `liquers-axum`.

## Rollback Plan

Everything is behind `client`; reverting the feature removes the slice. A partial landing (steps
1-2) gives a read-only remote store, which is still correct with `read_only: true` on its mount.

## Documentation Updates

As step 6. No generated file changes (`specs/command_registry.yaml` untouched).

## Phase 5 Entry Criteria

None — four-phase design.
