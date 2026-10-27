# Phase 4: Implementation Plan — A record source's rows served as a byte stream

## Overview

Prerequisite: `value-serialization-streaming` Steps 1-5 (`SerializedBytes`, `get_serialized`, the
axum serving functions). Then the records encoder (independent of core), the core seam, the
`liquers-lib` encoder and registration, the axum switch, and documents. Assumes Phase 1's
recommended answers; Q1 alternative (b) would replace Steps 2 and 4 with the `RecordValue`
generalization; alternative (c) moves Step 4's registration into an `EnvironmentBuilder` method.

## Progress

- [ ] Step 1: `encode_record_stream` in `liquers-records`
- [ ] Step 2: `StreamEncoder`, the registry field, `Environment::stream_encoders`
- [ ] Step 3: `AssetRef::get_streamed`
- [ ] Step 4: `RecordSourceEncoder` and its registration
- [ ] Step 5: axum data routes call `get_streamed`
- [ ] Step 6: documents, issue and index maintenance
- [ ] Step 7: feature matrix and final diff review

## Implementation Steps

### Step 1: `encode_record_stream` in `liquers-records`
- Files / symbols: new `liquers-records/src/formats/stream.rs` (`encode_record_stream`,
  `is_streamable`); `formats/mod.rs` (`pub mod stream`, re-export)
- Change: per Phase 2 Interfaces; per batch call `csv::write_csv(view, sep, &WriteOptions { header })`
  with `header` true only for the first, or `ndjson::write_ndjson`; error line built with
  `serde_json::json!` from `error.error_type` and `error.message`. `Json`, `Markdown`, `Html`,
  `Ipc`, `Parquet` arms return a one-item `Err` stream naming the format.
- Depends on: none (uses `futures`, already a dependency)
- Proof: `cargo test -p liquers-records --lib formats::stream` (Phase 3 unit tests) and
  `cargo check -p liquers-records --target wasm32-unknown-unknown`
- Rollback: delete the module

### Step 2: `StreamEncoder`, the registry field, `Environment::stream_encoders`
- Files / symbols: `liquers-core/src/value.rs` `StreamEncoder`; `commands.rs`
  `CommandRegistry::stream_encoders` (+ `new`); `context.rs` `Environment::stream_encoders`
  (defaulted) and the `GenericEnvironment` impl
- Change: additive; grep for `CommandRegistry {` struct literals outside `new` first (none expected)
- Depends on: parent Step 2 (`SerializedBytes`)
- Proof: `cargo check -p liquers-core` and `--target wasm32-unknown-unknown`;
  `cargo test -p liquers-core --lib`
- Rollback: revert the three files

### Step 3: `AssetRef::get_streamed`
- Files / symbols: `liquers-core/src/assets.rs` `AssetRef::get_streamed`
- Change: reuse `get_serialized`'s gate and wait (factor the shared part into a private helper
  rather than copying); then iterate `envref.get_stream_encoders()`-equivalent
  (`self.get_envref()?.0.stream_encoders()`), first `Some` wins, never cached; else delegate.
  Re-check the effective data format of a `RecordSource` asset with a `.csv` filename (Phase 2
  Risks) and add a core test with a fake encoder: `get_streamed_prefers_a_claiming_encoder`,
  `get_streamed_falls_back_to_get_serialized`
- Depends on: Step 2; parent Step 3
- Proof: `cargo test -p liquers-core --test serialized_stream` (parent's file, two tests added)
- Rollback: remove the method (no caller yet)

### Step 4: `RecordSourceEncoder` and its registration
- Files / symbols: new `liquers-lib/src/records/stream.rs` `RecordSourceEncoder`;
  `records/mod.rs` (`pub mod stream`); `records/commands.rs` `register_records_commands!` (push
  `Arc::new(RecordSourceEncoder)` onto `registry.stream_encoders` in the block that registers
  `store_config`)
- Change: per Phase 2; explicit format list; `Vec<u8>` → `bytes::Bytes`
- Depends on: Steps 1-3
- Proof: `CARGO_INCREMENTAL=0 cargo test -p liquers-lib --test record_source_stream` (Phase 3)
- Rollback: remove the registration line; the encoder is then dead code

### Step 5: axum data routes call `get_streamed`
- Files / symbols: `liquers-axum/src/query/handlers.rs` `serve_query`; `assets/common.rs`
  `asset_bytes` — `get_serialized` → `get_streamed`; entry routes unchanged
- Depends on: Step 3; parent Step 5
- Proof: `cargo test -p liquers-axum --test record_source_stream_routes` and the parent's route tests
- Rollback: revert the two calls

### Step 6: documents, issue and index maintenance
- Files: the five `affects_docs` documents (History row, `reviewed:` each);
  `specs/issues/RECORD-SOURCE-ROWS-CANNOT-BE-STREAMED.md` → `closed` with a Resolution; this
  design's `DESIGN.md` status; `python3 scripts/docs_index.py`
- Depends on: Steps 1-5
- Proof: `python3 scripts/docs_index.py --check`
- Rollback: revert the documents

### Step 7: feature matrix and final diff review
- Proof: `bash scripts/check-build-matrix.sh`; `cargo test -p liquers-lib --lib --tests`;
  `cargo test -p liquers-records --all-features --lib --tests`; diff review: no `println!`, no
  `unwrap`/`expect` in library code, no `_ =>` on `TableFormat` or `ExtValue`, no stray files

## Testing Plan

Unit tests at Step 1, core seam tests at Step 3, lib integration at Step 4, routes at Step 5, the
matrix at Step 7.

## Rollback Plan

Every step is additive; removing the registration (Step 4) restores the parent's behaviour for
every route.

## Documentation Updates

As Step 6. No generated registry change.

## Phase 5 Entry Criteria

None — four-phase design.
