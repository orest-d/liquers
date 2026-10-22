# Phase 4: Implementation Plan — A value's byte form as a pull stream

## Overview

Core primitive first, then the asset read, then HTTP, then the overrides that make something
actually stream. Prerequisites: `store-streaming-read-write` step defining `maybe_send::BoxStream`,
`store::ByteStream` and the `bytes` dependency (or step 1 here adds them as that design specifies),
and `shared-metadata-and-bytes` stages B (binary as `Bytes`) and C (`Value::Bytes(Bytes)`, needed by
step 6's AC-8 override only).

## Progress

- [ ] Step 1: stream types available in core
- [ ] Step 2: `SerializedBytes`, `serialize_stream` default, `Value` override, `from_writer`
- [ ] Step 3: `State::serialize_stream`, `AssetRef::get_serialized`; `get_binary` on top of it
- [ ] Step 4: `liquers-axum` serving functions and `StreamResponse`
- [ ] Step 5: Query API and Assets API routes use them; entries collect
- [ ] Step 6: `liquers-lib` overrides: `SimpleValue::Bytes`, `CombinedValue`, `PolarsDataFrame`
- [ ] Step 7: documents, issue records, full checks

## Implementation Steps

### Step 1: stream types available in core
- Files / symbols: `liquers-core/src/maybe_send.rs` `BoxStream`; `liquers-core/src/store.rs`
  `ByteStream`; `liquers-core/Cargo.toml` `bytes`.
- Change: none if `store-streaming-read-write` landed; otherwise exactly its definitions.
- Depends on: none. Proof: `cargo check -p liquers-core` and `--target wasm32-unknown-unknown`.
- Rollback: revert (unused until step 2).

### Step 2: the primitive
- Files / symbols: `liquers-core/src/value.rs` `SerializedBytes` (`length`, `collect`,
  `into_stream`, `from_writer` — native `spawn_blocking` + `tokio::sync::mpsc::channel(8)`, wasm
  collect), `DefaultValueSerializer::serialize_stream` (default),
  `serialize_stream_via_as_bytes` helper, `impl DefaultValueSerializer for Value` override for
  `Value::Bytes` (requires `shared-metadata-and-bytes` stage C; until then the override is omitted
  and AC-8 is deferred to step 6's re-run).
- Tests: `serialize_stream_default_equals_as_bytes`, `serialize_stream_bytes_shares_buffer`,
  `serialized_bytes_collect_and_into_stream` (AC-1, AC-8).
- Depends on: step 1. Proof: `cargo test -p liquers-core --lib value::`.
- Rollback: revert; nothing calls it yet.

### Step 3: state and asset reads
- Files / symbols: `liquers-core/src/state.rs` `State::serialize_stream`;
  `liquers-core/src/assets.rs` `AssetRef::get_serialized` (gate and wait copied from `get_binary`,
  `Whole` cached under the write lock as `serialize_to_binary` does), `get_binary` reimplemented as
  `get_serialized` → `collect` → cache.
- Tests: new `liquers-core/tests/serialized_stream.rs` `stream01`, `stream03` (AC-5, AC-7 support).
- Depends on: step 2. Proof: `cargo test -p liquers-core --test serialized_stream`, then
  `cargo test -p liquers-core --lib --tests` (every `get_binary` test unchanged).
- Rollback: revert; `get_binary` returns to its HEAD body.

### Step 4: serving functions
- Files / symbols: new `liquers-axum/src/api_core/serving.rs` (`ReadyBody`, `first_chunk`,
  `respond`, `timed_first_chunk`), `api_core/mod.rs`; `api_core/response.rs` /
  `axum_integration.rs` `StreamResponse` (from `store-streaming-read-write` if landed, else added
  here with the same headers as `BinaryResponse`, `Content-Length` when `length` is known, built
  with `build_or_500`).
- Tests: `serving::tests::streamed01`-`streamed04`, `streamed06` (AC-2, AC-3, AC-4, AC-5, AC-6).
- Depends on: step 2. Proof: `cargo test -p liquers-axum --lib api_core::serving`.
- Rollback: revert; unused until step 5.

### Step 5: routes
- Files / symbols: `liquers-axum/src/query/handlers.rs` `serve_query` (timeout through
  `timed_first_chunk`), `assets/common.rs` `asset_bytes` (`get_serialized` + `first_chunk` +
  `respond`), `asset_entry` and `assets/key_handlers.rs` entry route (`get_serialized` +
  `collect`, AC-9); module doc of `query/handlers.rs` (the timeout bounds the first chunk).
- Tests: `query_api_routes.rs` `streamed05_entry_collects` (AC-9); all existing axum tests.
- Depends on: steps 3, 4. Proof: `cargo test -p liquers-axum`.
- Rollback: revert step 5 alone; routes return to `get_binary`.

### Step 6: overrides in `liquers-lib`
- Files / symbols: `liquers-lib/src/value/simple.rs` `serialize_stream` (`SimpleValue::Bytes`),
  `value/extended.rs` `CombinedValue` forwarding, `value/mod.rs` `ExtValue::serialize_stream`
  (`#[cfg(feature = "polars")] PolarsDataFrame` for `csv`/`parquet` via `from_writer` and
  `serialize_dataframe_to_writer`; explicit arms for every other variant, including gated ones).
- Tests: new `liquers-lib/tests/dataframe_stream.rs` (AC-5, AC-7, AC-8, AC-10).
- Depends on: steps 2-3, `shared-metadata-and-bytes` stage C. Proof:
  `cargo test -p liquers-lib --test dataframe_stream`, `cargo test -p liquers-lib --lib --tests`,
  `cargo test -p liquers-lib --no-default-features --lib --tests`,
  `bash scripts/check-build-matrix.sh`.
- Rollback: revert; values fall back to `Whole`.

### Step 7: documents and records
- Files: the five `affects_docs` documents (History rows, `reviewed:`); in
  `VALUE-SERIALIZATION-IS-SYNCHRONOUS-AND-WHOLE-VALUE` a resolution note — **status stays open**
  (`in_progress` or `draft`) until the two follow-ups close, or is closed with the follow-ups named
  as the remainder (the maintainer's choice); design `status: complete`.
- Proof: `python3 scripts/docs_index.py --check`.

## Testing Plan

Per step as above. Final: `CARGO_INCREMENTAL=0 cargo test -p liquers-core --lib --tests`,
`cargo test -p liquers-lib --lib --tests`, `cargo test -p liquers-axum`,
`bash scripts/check-build-matrix.sh` (a `cfg(feature = "polars")` arm and a wasm32 branch of
`from_writer`), `cargo fmt --check`. A manual check with `curl -N` against a
server built over `liquers_lib::value::Value` serving a large frame confirms the first bytes arrive before the encoding
ends (AC-2), recorded in the PR.

## Rollback Plan

Steps 5 and 6 revert alone and return the system to whole-value serving; steps 2-4 are unused
without them. No stored format or HTTP header contract changes for whole values, so no client or
data migration is involved.

## Documentation Updates

`specs/reference/VALUE_TYPE_SYSTEM.md`, `specs/reference/ASSETS.md`,
`specs/reference/WEB_API_SPECIFICATION.md`, `specs/guides/SERIALIZATION_GUIDE.md`,
`specs/guides/TYPE_SYSTEM_GUIDE.md` (History rows, `reviewed:`); `specs/index.csv` regenerated by
the implementing session; `specs/command_registry.yaml` unchanged.

## Phase 5 Entry Criteria

None — four-phase design.
