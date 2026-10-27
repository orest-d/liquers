# Phase 2: Solution & Architecture — Streamed persistence of keyed assets

## Overview

`prepare_version` asks the state for `serialize_stream()` instead of `as_bytes()`. A `Whole`
encoding takes today's path unchanged (hash, cache as `binary`, finalize, persist in the
background). A `Stream` encoding is written before finalization: the chunks pass through a
`ContentHasher` and a byte counter into `store.set_stream(key, tee, provisional)`, where
`provisional` is the asset's record with status `Storing`. When the write ends, the prepared
version is the hash (or a unique fallback on any error), finalization installs it with the
readable status in one transaction as today, and a final `set_metadata` replaces the provisional
record and records the persistence status. No binary is cached.

Rejected alternatives (Phase 1 question 1): **B** non-content version — loses content-stable
cascades (AC-9) for exactly the large values where recomputation is most expensive; **C** whole
serialization — leaves the problem. Also rejected: **finalize first, then write the version into
the asset afterwards** — breaks the single-transaction rule documented on `prepare_version`; **a
metadata finalizer callback in `set_stream`** — a store API change on an unfinished design, and the
provisional record achieves the same crash safety.

## Known-Issue Preflight

| Issue | Status | Priority | Impact on this design | Fix first? | Blocking? | Action |
|---|---|---|---|---|---|---|
| `VALUE-SERIALIZATION-IS-SYNCHRONOUS-AND-WHOLE-VALUE` | draft, designed | P2 | provides `serialize_stream` | yes | yes (ordering) | `requires` |
| `CORE-STORE-OPENBIN-MISSING` | accepted, designed | P3 | provides `set_stream` | yes | yes (ordering) | `requires` |
| `SERIALIZED-BINARY-RETAINED-WITH-NO-DISPOSAL-POLICY` | draft, designed | P2 | whole values only | no | no | none |
| `CANCEL-CAN-OVERTAKE-A-RETURNED-COMMAND` | draft, designed | P3 | a cancel must not drop the write half-way | no | no | covered by `cancel-point-of-no-return`; the write runs after the command returned |
| `SERIALIZATION-CANNOT-REPORT-A-LOSSY-WRITE` | draft, designed (`serialization-lossy-write-warnings`) | — | warnings found mid-stream are `value-serialization-streaming`'s follow-up | no | no | none |

## Interfaces

```rust
// liquers-core/src/metadata.rs
/// Incremental form of [`Version::from_content`]: feeding the same bytes in any chunking gives
/// the same version.
pub struct ContentHasher { hasher: blake3::Hasher }
impl Version { pub fn content_hasher() -> ContentHasher; }
impl ContentHasher {
    pub fn update(&mut self, chunk: &[u8]);
    pub fn finish(self) -> Version;    // first 16 bytes as u128, HASH_FLAG set — as from_content
}

// liquers-core/src/assets.rs (private)
struct PreparedVersion {
    binary: Option<Bytes>,             // Arc<Vec<u8>> until shared-metadata-and-bytes stage B
    version: Version,
    serialization_error: Option<Error>,
    /// Set when the encoding was streamed: what the write did, for the final record.
    streamed: Option<StreamedWrite>,
}
struct StreamedWrite { file_size: u64, result: Result<SaveOutcome, Error> }

impl<E: Environment> AssetRef<E> {
    /// Writes a streamed encoding under `key` while hashing it. Never fails: errors become the
    /// fallback version and `result`.
    async fn write_streamed(&self, key: &Key, chunks: ByteStream<'static>,
                            provisional: Metadata, store_it: bool) -> PreparedVersion;
    /// After finalization: the final record and the persistence status.
    async fn finish_streamed_persistence(&self, key: &Key, streamed: StreamedWrite);
}
```

Async throughout; `ContentHasher` is sync. The tee is a `futures::StreamExt::map` over the chunks
that updates a `std::sync::Mutex<(ContentHasher, u64)>` owned by `write_streamed` (borrowed by the
stream, which `set_stream` takes as `ByteStream<'_>`), so the hasher is read back after the call.

## Integration Points

`liquers-core/src/assets.rs`:
- `prepare_version`: `state.serialize_stream()` replaces `state.as_bytes()`. `Ok(Whole(b))` →
  today's `PreparedVersion` from `b`. `Ok(Stream { chunks, .. })` → `write_streamed(key, chunks,
  provisional, metadata.stored())` where `provisional` is the locked record cloned with
  `set_status(Status::Storing)`. `Err(e)` → today's fallback.
- `write_streamed`: when `store_it`, `store.set_stream(key, tee(chunks), &provisional)`; otherwise
  drain the tee. On success, `version = hasher.finish()`, `result = Ok(Written)` (or
  `Ok(Skipped)` when not stored). On an error from either side, `version = Version::new_unique()`,
  `serialization_error = Some(e.clone())` when the error came from the value, and `result = Err(e)`.
- `finalize_status_with_version`: unchanged; `binary` is `None` for a streamed value.
- `complete_evaluation`: when `prepared.streamed` is `Some`, call `finish_streamed_persistence`
  instead of `persist_with_status_tracking` (the background flag is ignored). It supersedes the
  queued metadata snapshot (`metadata_saver.supersede()`, as `save_to_store` does), writes the
  finalized record with `file_size` set via `store.set_metadata` when the data write succeeded,
  refreshes the parent listing version, and calls `record_persistence_result`.
- `serialize_to_binary` / `get_binary`: unchanged (a reader re-serializes; nothing is cached for a
  streamed value).

`liquers-core/src/metadata.rs`: `ContentHasher`, `Version::content_hasher`; `from_content` is
re-expressed through it so the two cannot drift.

## Error Handling

| Failure | Asset | Store | Persistence status |
|---|---|---|---|
| value stream error mid-way | `Ready`, unique version, warning log | no partial object (`set_stream` AC-4) | from `classify_persistence_error` |
| `set_stream` error | `Ready`, unique version, warning log | as above | from `classify_persistence_error` |
| final `set_metadata` error | `Ready`, content version | data + `Storing` record (recomputed on load) | failed |
| `stored: false` | `Ready`, content version | untouched | `None` |

Errors are the existing typed ones; nothing new in `ErrorType`; no `unwrap`.

## Relevant Commands

None; no registry change.

## Documentation Architecture

`specs/reference/ASSETS.md` §Keyed assets: storing and persistence — a streamed keyed value is
written before it is readable, with the crash rule; `specs/reference/ASSET_LIFECYCLE.md` — the
in-flight period; `specs/guides/SERIALIZATION_GUIDE.md` — the consequence of overriding
`serialize_stream` for keyed persistence. History rows and `reviewed:` bumps.

## Risks

| Assessment | Finding |
|---|---|
| Files likely to change | `liquers-core/src/assets.rs` (`prepare_version`, `PreparedVersion`, `complete_evaluation`, two new private methods), `liquers-core/src/metadata.rs`; new `liquers-lib/tests/streamed_persistence.rs` |
| Crates and workflows affected | keyed evaluation of streaming value types (polars frames after `value-serialization-streaming`); every other value is on the unchanged `Whole` path |
| Existing tests likely to change | none by signature; tests that assert `poll_binary().is_some()` on a keyed polars asset would change (none found by `grep` at HEAD) |
| New validation | `ContentHasher` equivalence unit test; `liquers-lib` streamed tests behind `polars`; build matrix (`polars` off: the path is never taken) |
| Compatibility / data / concurrency / performance / security | readiness of a streamed keyed value waits for its write (documented); the store's lock is held for the write (as `set`); a provisional `Storing` record is visible to other processes during the write and read as "not stored"; no format change |
| Recovery | revert; `prepare_version` returns to `as_bytes` |
| Certainty and open questions | high on the asset path (read at HEAD); depends on the exact `SerializedBytes` and `set_stream` signatures of two unfinished designs |
