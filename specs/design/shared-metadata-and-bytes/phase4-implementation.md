# Phase 4: Implementation Plan — Share metadata records and stored bytes

## Overview

Three PRs in the order A, B, C (A may also go after B). Prerequisite: `core-value-enum-oversized`
implemented (stage A shares into its `Value::Metadata(Arc<MetadataRecord>)`). Within each stage the
signature change comes first and the compiler lists every site; tests come before the behaviour
they prove.

## Progress

- [ ] Step 1: stage A tests (metadata sharing, wire pins)
- [ ] Step 2: `Metadata` record behind `Arc`, copy-on-write mutators
- [ ] Step 3: asset value install helper; reads share metadata; `set_state` shares the value
- [ ] Step 4: `from_metadata(Arc<…>)`, `metadata_record()` returns `Arc`; callers; registry
- [ ] Step 5: stage B — `bytes` in core; `AsyncStore` reads return `Bytes`; all implementors
- [ ] Step 6: stage B — asset binary family and `AssetManager::get_binary_any_status` in `Bytes`
- [ ] Step 7: stage B — `liquers-axum` bodies from `Bytes`; OpenDAL write copy
- [ ] Step 8: stage C — `Value::Bytes(Bytes)`, `SimpleValue::Bytes`, `from_bytes(Bytes)`
- [ ] Step 9: documents, issue records, full checks

## Implementation Steps

### Step 1: stage A tests
- Files / symbols: `liquers-core/src/metadata.rs` `mod tests::shared_record`; HEAD JSON/YAML literals
  for a record with log, dependencies and error data, captured before any change.
- Change: tests of Phase 3 for AC-1, AC-2, AC-3 (unit part), AC-5 (metadata part).
- Depends on: none. Proof: the wire pins pass at HEAD; the sharing tests fail to compile until step 2.
- Rollback: delete the module.

### Step 2: `Metadata` record behind `Arc`
- Files / symbols: `liquers-core/src/metadata.rs` `enum Metadata`, `new`, `from_error`,
  `from_json_value`, the 26 `&mut self` methods, `with_type_identifier`/`with_type_name` (compare
  first), `record_mut`, `From<Arc<MetadataRecord>>`; then every construction
  `Metadata::MetadataRecord(record)` in core (`.into()`) and every `= &mut … MetadataRecord(x)` site
  (`record_mut()`), in `assets.rs`, `interpreter.rs`, `store/pipeline.rs` and tests.
- Depends on: step 1. Proof: `cargo test -p liquers-core --lib metadata::` (AC-1, AC-2, AC-5).
- Rollback: revert; no format changed.

### Step 3: asset reads share
- Files / symbols: `liquers-core/src/assets.rs` new private `AssetData::install_value`; the
  production install sites (fast track, run finish, `set_value`, `set_state` paths of both managers
  — the two `Arc::new(state.data_unchecked().as_ref().clone())` become the state's `Arc`);
  `exposed_value_state`, `binary_unchecked` unchanged in shape (their setters are now no-ops).
- Depends on: step 2. Proof: new `liquers-core/tests/shared_payloads.rs` `shared01`, `shared04`,
  `shared05` (AC-3, AC-9); `cargo test -p liquers-core --lib --tests`.
- Rollback: revert the commit; behaviour returns to copying.

### Step 4: value trait shares records
- Files / symbols: `liquers-core/src/value.rs` `ValueInterface::from_metadata`, `impl for Value`;
  `metadata.rs` `metadata_record`; `context.rs`; `interpreter.rs` (two `from_metadata` calls);
  `liquers-lib/src/value/simple.rs`, `extended.rs` `from_metadata`; `liquers-lib/src/commands.rs`
  `to_metadata`, `to_assetinfo`; `template/host.rs`; `liquers-py/src/metadata.rs`.
- Change: Q2's signatures; `to_metadata_shares_state_record` test (AC-4); regenerate
  `specs/command_registry.yaml` (`to_metadata` is `version: auto`) with a CHANGELOG line.
- Depends on: step 2, `core-value-enum-oversized`. Proof:
  `cargo test -p liquers-lib --lib --tests`, `cargo test -p liquers-lib --test registry_export`,
  `bash scripts/test-liquers-py.sh`.
- Rollback: revert steps 4 alone; A's other steps stand.

### Step 5: store reads in `Bytes`
- Files / symbols: `liquers-core/Cargo.toml` (`bytes = "1.12.1"`, unless `store-streaming-read-write`
  added it), `liquers-core/src/lib.rs` (`pub use bytes::Bytes`); `store.rs` trait `get`,
  `get_bytes`, `get_data` and every implementor listed in Phase 2 (core, `liquers-store`,
  `liquers-web`, `liquers-records` test stores); `AsyncMemoryStore` entry `data: Option<Bytes>`;
  `store/storage.rs` `read_object`; `liquers-records/src/sources.rs` `read_resource` converts with
  `Vec::from`.
- Change: types only; `memory_store_get_shares_buffer` (AC-6).
- Depends on: none (independent of A). Proof: `cargo test -p liquers-core --lib --tests`,
  `cargo test -p liquers-store`, `cargo test -p liquers-records --lib --tests`, the
  `liquers-web` Node loop (`--features debug-handles`, and with `records`).
- Rollback: revert; the conformance suites guard content.

### Step 6: asset binaries in `Bytes`
- Files / symbols: `liquers-core/src/assets.rs` `AssetData::binary`, `PreparedVersion::binary`,
  `poll_binary`, `poll_binary_any_status`, `binary_unchecked`, `AssetRef::serialize_to_binary`,
  `get_binary`, `poll_binary`, `poll_binary_any_status`, `get_binary_any_status`,
  `try_poll_binary`, `AssetManager::get_binary_any_status` and both impls; fast track keeps the
  read `Bytes`; `prepare_version` wraps `as_bytes` output with `Bytes::from(vec)` (no copy);
  `interpreter.rs` `GetResource` (`Vec::from(data)` until step 8); `liquers-lib/src/search/mod.rs`.
- Depends on: step 5. Proof: `shared03_get_binary_shares_cached_buffer` (AC-8);
  `cargo test -p liquers-core --lib --tests`; `cargo test -p liquers-lib --lib --tests`.
- Rollback: revert steps 6-7 together.

### Step 7: HTTP bodies and the OpenDAL write
- Files / symbols: `liquers-axum/src/api_core/response.rs` `BinaryResponse::data: Bytes`;
  `axum_integration.rs` `Body::from(self.data)`; `query/handlers.rs` `serve_query`,
  `assets/common.rs` `asset_bytes`, `store/handlers.rs` `get_data_handler` (no `.clone()` of the
  bytes); `liquers-store/src/opendal_store.rs` `set`/`set_data` use `Buffer::from(data.to_vec())`.
- Depends on: step 6. Proof: `cargo test -p liquers-axum` (bodies unchanged, AC-8),
  `cargo test -p liquers-store`.
- Rollback: revert.

### Step 8: values hold `Bytes`
- Files / symbols: `liquers-core/src/value.rs` `Value::Bytes(Bytes)`, `bytes_as_seq`,
  `from_bytes(Bytes)`, `as_bytes`/`try_into_bytes` arms (`to_vec()`), `deserialize_from_bytes`
  bin arm (`Bytes::copy_from_slice`); `liquers-lib/src/value/simple.rs` `SimpleValue::Bytes`,
  `extended.rs` `from_bytes`; `interpreter.rs` `GetResource` passes the `Bytes`; `liquers-web` site;
  test constructions `.into()`.
- Change: as Phase 2 stage C; tests `value_bytes_wire_format_unchanged`,
  `simple_value_bytes_wire_format_unchanged`, `shared02_resource_value_shares_store_buffer`
  (AC-5, AC-7).
- Depends on: steps 5-6, `core-value-enum-oversized`. Proof:
  `cargo test -p liquers-core --lib --tests`, `cargo test -p liquers-lib --lib --tests`,
  `cargo test -p liquers-lib --test registry_export`.
- Rollback: revert step 8 alone.

### Step 9: documents and records
- Files: the four `affects_docs` documents (History row, `reviewed:`); close
  `SHAREABLE-METADATA-AND-BYTES-ARE-DEEP-COPIED` with a `## Resolution` naming the commits; design
  `status: complete`; `specs/README.md` design link if the folder is not yet listed.
- Proof: `python3 scripts/docs_index.py --check`.

## Testing Plan

Stage A: `cargo test -p liquers-core --lib --tests`, `cargo test -p liquers-lib --lib --tests`,
`registry_export`, the Python script. Stage B: add `liquers-store`, `liquers-axum`,
`liquers-records --lib --tests`, the `liquers-web` Node loops. Stage C: core, lib, registry export.
After each stage `bash scripts/check-build-matrix.sh` (a dependency is added and `ExtValue`-adjacent
matches change), and `cargo fmt --check`. `CARGO_INCREMENTAL=0` throughout; run the web loops after
`cargo clean` (CLAUDE.md).

## Rollback Plan

Each stage is one PR and reverts alone; A's steps 2-4 revert as a unit or step 4 alone; B's steps
5-7 as a unit (step 5 alone is consistent: assets would wrap the read `Bytes` with `Vec::from`);
C alone. No stored data or wire format changes, so no data migration is ever needed.

## Documentation Updates

`specs/reference/VALUE_TYPE_SYSTEM.md`, `specs/reference/ASSETS.md`,
`specs/reference/STORE_SEMANTICS.md`, `specs/guides/STORE_IMPLEMENTATION_GUIDE.md` (History rows,
`reviewed:`); `specs/command_registry.yaml` regenerated in step 4 with a CHANGELOG line;
`specs/index.csv` regenerated by the implementing session.

## Phase 5 Entry Criteria

None — four-phase design.
