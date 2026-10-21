# Phase 4: Implementation Plan — Streaming reads and writes on AsyncStore

## Overview

Core first (alias, trait defaults, file store, router), then the conformance rules that every
suite runs, then OpenDAL, then the HTTP consumers, then documents. Each step leaves every store
correct because the defaults exist from step 2. Prerequisites: decisions Q1 and Q2 (Phase 1);
nothing to fix first.

## Progress

- [ ] Step 1: stream alias and dependencies
- [ ] Step 2: trait methods with collecting defaults
- [ ] Step 3: `AsyncFileStore` overrides
- [ ] Step 4: router and `SharedStore` forwarding
- [ ] Step 5: conformance rules `stream01`-`stream03`
- [ ] Step 6: `AsyncOpenDALStore` overrides
- [ ] Step 7: axum store routes
- [ ] Step 8: documents and records

## Implementation Steps

### Step 1: stream alias and dependencies
- Files / symbols: `liquers-core/Cargo.toml` (`bytes = "1"`; tokio `io-util` in the native row);
  `liquers-core/src/maybe_send.rs` `BoxStream`
- Change: as Phase 2 Interfaces
- Depends on: none
- Proof: `cargo test -p liquers-core --lib maybe_send`; `bash scripts/check-build-matrix.sh` wasm32 rows
- Rollback: revert the two files

### Step 2: trait methods with collecting defaults
- Files / symbols: `liquers-core/src/store.rs` `ByteStream`, `AsyncStore::get_stream`,
  `AsyncStore::set_stream`, private `clip_range`; trait doc's `openbin` sentence and both
  `// TODO: implement openbin` blocks replaced
- Change: defaults per Phase 2
- Depends on: 1
- Proof: `cargo test -p liquers-core --lib stream_default_collects_through_get_and_set stream_methods_refuse`
- Rollback: revert; additive

### Step 3: `AsyncFileStore` overrides
- Files / symbols: `store.rs` `impl AsyncStore for AsyncFileStore` `get_stream`, `set_stream`
  (lock, `Storing` sidecar, chunked `write_all`, final sidecar, cleanup on error)
- Depends on: 2
- Proof: `cargo test -p liquers-core --lib file_store_` (bounded chunks, failed stream, lock, range)
- Rollback: remove the overrides; the defaults apply

### Step 4: router and `SharedStore` forwarding
- Files / symbols: `store.rs` `AsyncStoreRouter`, `SharedStore`
- Depends on: 2 (parallel with 3)
- Proof: `cargo test -p liquers-core --lib router_forwards_streams`
- Rollback: remove the forwards

### Step 5: conformance rules `stream01`-`stream03`
- Files / symbols: `liquers-core/src/store_conformance/rules/streaming.rs` (new),
  `rules/mod.rs` `RULES`
- Depends on: 2-4
- Proof: `cargo test -p liquers-core --features store-conformance --test store_conformance_CONF`;
  a failure in a suite is a store defect to fix here or list in that suite's `AllowedFailure` with
  a filed issue
- Rollback: unregister the rules

### Step 6: `AsyncOpenDALStore` overrides
- Files / symbols: `liquers-store/src/opendal_store.rs` `get_stream` (`reader_with` +
  `into_bytes_stream`), `set_stream` (`writer`, `write`, `close`; `abort` + `delete` for
  non-atomic services); the `openbin` comment removed
- Depends on: 2, 5
- Proof: `cargo test -p liquers-store --lib opendal_stream`;
  `cargo test -p liquers-store --features store-conformance --test store_conformance_CONF`
- Rollback: remove the overrides

### Step 7: axum store routes
- Files / symbols: `liquers-axum/src/store/handlers.rs` `get_data_handler`, `put_data_handler`;
  `api_core/response.rs` `StreamResponse`; `axum_integration.rs` its `IntoResponse` (no `unwrap`)
- Change: single-range parsing with `416` pre-check against `file_size`; `206` + `Content-Range`;
  streamed `PUT`
- Depends on: 2 (better with 3 and 6)
- Proof: `cargo test -p liquers-axum --test store_api_routes`
- Rollback: restore the two handlers; the response type is unused

### Step 8: documents and records
- Files: `STORE_SEMANTICS.md` new section; `STORE_IMPLEMENTATION_GUIDE.md` §1, §2, §8, §9; History
  rows, `reviewed:`; close `CORE-STORE-OPENBIN-MISSING` with a resolution note; file a follow-up
  feature for streaming through `MetadataPipelineStore` / `ReadOnlyStore` and through
  `upload_handler` / `DataEntry`; note in `VALUE-SERIALIZATION-IS-SYNCHRONOUS-AND-WHOLE-VALUE` that
  its store side exists
- Depends on: 1-7
- Proof: `cargo test -p liquers-core --features store-conformance --test conformance_docs_CONF`;
  `python3 scripts/docs_index.py --check`
- Rollback: revert the documents

## Testing Plan

After each step its proof; after step 5 every conformance suite; at the end
`cargo test -p liquers-lib --lib --tests` (consumers compile), `bash scripts/check-build-matrix.sh`
(the tokio feature and `BoxStream` per target), and, after `cargo clean`, the `liquers-web` Node
loop, whose stores inherit the defaults and run the new rules.

## Rollback Plan

Every override can be removed alone, leaving the defaults. Reverting step 2 needs 3-7 reverted
first. Nothing persistent changes format: the file store's on-disk layout and `Storing` protocol
are those of `set`.

## Documentation Updates

`specs/reference/STORE_SEMANTICS.md` (new section, History row, `reviewed:`),
`specs/guides/STORE_IMPLEMENTATION_GUIDE.md` (§1, §2, §8, §9, History, `reviewed:`),
`specs/index.csv` regenerated by the maintainer's index run. No `specs/command_registry.yaml`
change.

## Phase 5 Entry Criteria

None — four-phase design.
