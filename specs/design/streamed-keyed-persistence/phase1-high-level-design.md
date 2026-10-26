# Phase 1: High-Level Design — Streamed persistence of keyed assets

## Design Readiness

- **Readiness:** needs-decision
- **Automatic fixing:** not-eligible — a new `pub` incremental hasher in `liquers-core`
  (rules 3, 4), changes when a keyed asset becomes readable (an observable lifecycle change), `L`,
  and open decisions (rule 5).
- **Leading issue:** **Open design - when a streamed keyed asset becomes readable.** Recommended:
  after its encoding is written and hashed (option A), so the content version is still installed
  in the same transaction as the readable status.
- **Explanation:** every site of the keyed persistence path was read at HEAD
  (`complete_evaluation`, `prepare_version`, `finalize_status_with_version`,
  `persist_with_status_tracking`, `save_to_store`). Under option A the change is local to that path
  and leaves whole-value persistence untouched. It cannot land before its three prerequisites
  (Design Dependencies). Phases 3-4 assume the recommended answers.
- **Open questions:**
  1. **Open design - readiness timing (the issue's options):**
     **A** stream to `set_stream` while hashing; the asset stays in flight until the write ends,
     then becomes readable with its content version (recommended: keeps the "version installed with
     the readable status" invariant of `keyed-expiry-cascade-fix` and content-stable cascades);
     **B** finalize at once with a non-content version (`Version::new_unique`) and stream in the
     background — readers do not wait, but every recomputation expires all dependents even when the
     bytes are unchanged; **C** keep whole serialization for keyed assets and document it — no
     change, peak memory stays value + encoding.
  2. **Proposed resolution - status during the write:** `Processing` (already set; readers see
     "pending"). Not `Storing`, whose contract says the store maintains it and an `AssetRef`
     should not be in it.
  3. **Proposed resolution - crash safety:** the streamed write stores a provisional record with
     status `Storing`; the final record (status, version, `file_size`) is written after the
     in-memory finalization. A process that stops in between leaves a record that a later load
     treats as failed (`Status::Storing` doc), so the key is recomputed, never served stale.
  4. **Proposed resolution - scope:** only the evaluation path (`complete_evaluation`) streams.
     `set_state`, `set_value`, `to_override` and `AssetManager::set_binary` keep whole persistence
     (a follow-up if a caller needs it). `save_in_background` does not apply to a streamed value:
     its write is on the readiness path by option A.
  5. **Implementation detail (resolved):** an incremental `ContentHasher` beside
     `Version::from_content`, giving bit-identical versions (blake3 is incremental).

## Purpose

A keyed recipe whose value is large (a DataFrame export, later a record source's rows) is
serialized whole before it is stored, because its content version hashes the whole encoding. Once
values can stream their encoding (`value-serialization-streaming`) and stores can take a stream
(`store-streaming-read-write`), keyed persistence should not collect it: peak memory should be the
value plus one chunk, not the value plus its whole encoding (VISION: data service; DataFrames and
records).

## Problem Example

A keyed recipe `exports/sales.csv` (`-R/data/in.csv/-/ns-pl/from_csv/sales.csv`, validates) whose
frame encodes to 2 GB of CSV. Today `prepare_version` calls `state.as_bytes()`, hashes the 2 GB
with `Version::from_content`, installs them as `AssetData::binary`, and `save_to_store` writes them
with `store.set`. Peak memory: the frame plus 2 GB (and the bytes stay cached until
`serialized-binary-disposal-policy` lands). Should be: the CSV chunks flow through a hasher into
`set_stream`; when the stream ends the asset becomes `Ready` with
`Version::from_content(<the same 2 GB>)`, and no buffer larger than a chunk was held.

## Scope and Acceptance Criteria

- **AC-1** A streamed keyed value is never collected
  WHEN a keyed computed asset's value encodes as `SerializedBytes::Stream` and its store overrides
  `set_stream`
  THEN the store receives more than one chunk, none of them the whole encoding, and the stored
  bytes equal the value's `as_bytes` in its data format
- **AC-2** The version is the content hash
  WHEN that asset becomes readable
  THEN its version equals `Version::from_content` of the stored bytes, as for a whole value
- **AC-3** Readable only with its version
  WHEN any observer polls the asset during the write
  THEN it sees an in-flight status; the first readable status it sees carries the version
- **AC-4** `stored: false` still gets a content version
  WHEN the recipe says `stored: false`
  THEN nothing is written, the version is still the content hash, and no whole encoding is held
- **AC-5** An encoding error mid-stream
  WHEN the value's stream yields an error after some chunks
  THEN the asset is readable with its value and a unique fallback version and a warning log entry,
  its persistence status records the failure, and the key holds no partial object
- **AC-6** A store error mid-write
  WHEN `set_stream` fails
  THEN the same fallback as AC-5 applies, and the value is served
- **AC-7** A crash between data and final record
  WHEN the final record write fails after the data was written
  THEN the stored record has status `Storing`, and a new environment requesting the key recomputes
  it instead of loading it
- **AC-8** Whole values are unchanged
  WHEN the value encodes as `SerializedBytes::Whole`
  THEN persistence, caching of the binary and readiness timing are as at HEAD
- **AC-9** Unchanged content does not cascade
  WHEN a streamed keyed asset is recomputed and produces the same bytes
  THEN its dependents are not expired

Non-goals: streaming in `set_state` / `set_value` / `to_override` / `set_binary`; lazy values that
need the environment to encode (`RECORD-SOURCE-ROWS-CANNOT-BE-STREAMED`); caching a streamed
encoding; changing `Status::Storing`'s contract; whole-write copies
(`STORE-WHOLE-WRITE-COPIES-CALLER-BYTES`).

## Core Interactions

- **Assets:** the keyed finalization path of `complete_evaluation`.
- **Store:** `set_stream` and `set_metadata` on the environment's store.
- **Value types:** `State::serialize_stream` (from `value-serialization-streaming`).
- **Metadata:** an incremental content hasher beside `Version::from_content`.

## Crate Placement

`liquers-core` only (`assets.rs`, `metadata.rs`). `liquers-lib` gains tests (polars values stream);
no other crate changes.

## Documentation Intent

- Reference: extend `specs/reference/ASSETS.md` (§Keyed assets: storing and persistence — the
  streamed path and its readiness timing) and `specs/reference/ASSET_LIFECYCLE.md` (the in-flight
  period now includes the write for a streamed keyed value).
- Guide: extend `specs/guides/SERIALIZATION_GUIDE.md` §For implementers (a streaming override also
  changes how a keyed value persists).
- Documents to update: the three above, History rows and `reviewed:` bumps.

## Design Dependencies

- `value-serialization-streaming` (needs-decision) — **requires**: `SerializedBytes`,
  `State::serialize_stream`, and a polars override that streams. Origin of this issue (its Q5).
- `store-streaming-read-write` (needs-decision) — **requires**: `AsyncStore::set_stream`,
  `ByteStream`, and its AC-4 (no partial object on a stream error).
- `shared-metadata-and-bytes` (needs-decision) — **requires** stage B only indirectly (the
  `bytes` dependency); either it or `store-streaming-read-write` adds it.
- `serialized-binary-disposal-policy` (needs-decision) — **overlaps**, weak: a streamed encoding is
  never cached, so that policy applies only to whole values. Either order.
- `store-shared-buffer-write` (this batch) — **overlaps**, weak: it changes the whole-value write
  in `save_to_store`; this design adds a separate streamed write before finalization, so the change
  sites differ (the same-change-site test was considered and does not hold). Either order.
- `cancel-point-of-no-return` (ready) — **overlaps**: the streamed write runs after the command
  returned `Ok`, so a cancel must not drop it half-way; that design makes this hold.
- `keyed-expiry-cascade-fix` (complete) — the invariant kept by option A.

## Consolidated Findings

- The invariant "version installed in the same transaction as the readable status"
  (`finalize_status_with_version`) is the reason the issue exists; option A keeps it by moving the
  write *before* finalization for streamed values only.
- Readiness latency grows by the part of the write not overlapped with encoding. Today the whole
  encoding is already on the readiness path (`prepare_version` runs before finalization), so the
  additional cost is bounded by write time, not by encoding time.
- `set_stream` takes its metadata before the stream is consumed, so the version cannot be in the
  record it writes; a provisional `Storing` record plus a final `set_metadata` gives crash safety
  without changing the store API.
- Fallback on any mid-stream failure is the existing serialization-error fallback (unique version,
  warning), so no new failure state is introduced.
- Tests need a value that streams, so they live in `liquers-lib` behind the `polars` feature.
