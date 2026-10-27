# Phase 3: Examples & Use-cases — Streamed persistence of keyed assets

## Overview Table

| # | Kind | Name | Shows / checks | Scenarios |
|---|---|---|---|---|
| 1 | Example | A large keyed CSV export | memory and readiness | AC-1, AC-2, AC-3 |
| 2 | Test | `metadata::tests::content_hasher_matches_from_content` | chunking-independent hash | AC-2 |
| 3 | Test | `streamed01_keyed_frame_is_written_in_chunks` | no whole buffer at the store | AC-1 |
| 4 | Test | `streamed02_version_is_content_hash` | version equals `from_content(stored bytes)` | AC-2 |
| 5 | Test | `streamed03_never_readable_without_version` | observer during a gated write | AC-3 |
| 6 | Test | `streamed04_stored_false_hashes_without_writing` | `stored: false` | AC-4 |
| 7 | Test | `streamed05_value_stream_error_falls_back` | encoding error | AC-5 |
| 8 | Test | `streamed06_store_error_falls_back` | store error | AC-6 |
| 9 | Test | `streamed07_storing_record_is_recomputed` | crash window | AC-7 |
| 10 | Test | `streamed08_whole_value_path_unchanged` | text value keyed asset | AC-8 |
| 11 | Test | `streamed09_same_bytes_do_not_cascade` | dependents kept | AC-9 |

## Example 1: a large keyed CSV export

`exports/recipes.yaml` declares `-R/data/in.csv/-/ns-pl/from_csv/sales.csv` (validates with
`liquers-validate`; `-R/` then `/-/` starts the action segment). With an `AsyncFileStore` or an
OpenDAL store:

```rust
let asset = envref.get_asset_manager().get(&parse_key("exports/sales.csv")?).await?;
let state = asset.get().await?;            // returns once the CSV is written and hashed
assert!(asset.poll_binary().await.is_none());          // nothing cached
let version = state.metadata.version();               // Some(content hash)
```

Peak memory: the frame plus one chunk (64 KiB by default at the file store). Before this design:
the frame plus the whole CSV.

## Edge and Error Cases

- **Store without a `set_stream` override** (memory store, pipeline, read-only, web, Python):
  the default collects, so memory is not saved, but versions and readiness are as specified
  (tests 4-11 run on `AsyncMemoryStore`; test 3 needs the chunk-recording wrapper).
- **Empty frame** → a stream that yields one short chunk (CSV header) or none; `ContentHasher`
  over no chunks equals `from_content(b"")` → test 2.
- **`stored: false` together with a streamed value** → drained into the hasher only → test 6.
- **Cancel during the write** → not interruptible once the command returned `Ok`
  (`cancel-point-of-no-return`); covered there, not here.
- **Environment dropped mid-write** → the future is dropped; the record stays `Storing`, which
  test 9 models by failing the final `set_metadata`.
- **Delegated evaluation** (a non-owner keyed asset) → `ValueOrigin::Delegated` never serializes;
  unchanged.

## Test Plan

Unit test in `liquers-core/src/metadata.rs`: `content_hasher_matches_from_content` — random
buffers fed in 1, 7 and 4096-byte chunks and empty input give `Version::from_content` (AC-2).

New `liquers-lib/tests/streamed_persistence.rs`, `#![cfg(feature = "polars")]`, using the lib
environment builder with:
- `ChunkRecordingStore`, a test wrapper over `AsyncMemoryStore` that overrides `set_stream` to
  record each chunk length, the total and the metadata it was given, then forwards the collected
  bytes to the inner `set`; switches to make `set_stream` or `set_metadata` fail;
- `data/in.csv` with 100 000 rows (`a,b` integers) written in the test; recipe
  `exports/sales.csv` as in Example 1 and, for AC-9, the dependent `exports/head.csv` =
  `-R/exports/sales.csv/-/ns-pl/head-5/head.csv` (validates);
- a recipe with `stored: false` for AC-4.

Tests:
- `streamed01_keyed_frame_is_written_in_chunks` (AC-1): more than one chunk, max chunk < total,
  stored bytes equal `ExtValue::as_bytes("csv")` of the frame.
- `streamed02_version_is_content_hash` (AC-2).
- `streamed03_never_readable_without_version` (AC-3): the wrapper's `set_stream` waits on a
  `tokio::sync::Notify` before finishing; a concurrent `poll_state` sees in-flight; after release
  the first readable state has `version().is_some()`.
- `streamed04_stored_false_hashes_without_writing` (AC-4): wrapper saw no call; version is the hash
  of `as_bytes("csv")`.
- `streamed05_value_stream_error_falls_back` (AC-5): a test-only `ExtValue::Foreign` value whose
  `serialize_stream` yields two chunks then an error (`ForeignValue` override; no polars needed —
  this test is ungated); asset readable, `version().kind() == Timestamp`, log has the warning,
  `get_bytes(key)` is `KeyNotFound`.
- `streamed06_store_error_falls_back` (AC-6).
- `streamed07_storing_record_is_recomputed` (AC-7): `set_metadata` fails; stored record status is
  `Storing`; a second environment over the same store recomputes (command counter +1).
- `streamed08_whole_value_path_unchanged` (AC-8): a keyed text recipe; `poll_binary()` is `Some`
  after evaluation (until `serialized-binary-disposal-policy` lands) and readiness precedes the
  write, as today.
- `streamed09_same_bytes_do_not_cascade` (AC-9): expire and recompute `exports/sales.csv` with the
  same input; `exports/head.csv` stays `Ready`.

Command: `CARGO_INCREMENTAL=0 cargo test -p liquers-lib --test streamed_persistence`, and
`cargo test -p liquers-core --lib content_hasher`.

## Learning Log

- Tests of streamed behaviour need a value whose `serialize_stream` streams, which core `Value`
  never does; `liquers-lib` is the lowest crate that has one.
