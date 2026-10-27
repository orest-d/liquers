# Phase 3: Examples & Use-cases — The reconciliation contract for external indexes

## Overview Table

| # | Kind | Name | Shows / checks | Scenarios |
|---|---|---|---|---|
| 1 | Example | Keeping a search engine in step with a corpus | the whole cycle | AC-1, AC-2, AC-3, AC-4 |
| 2 | Example | Upgrading an embedding model | projection identity | AC-5 |
| 3 | Example | A volatile dashboard chunk | time-based regime | AC-6 |
| 4 | Test | `liquers-records/tests/sync_reconcile.rs` | the diff with `MemorySink` | AC-1 … AC-11 |
| 5 | Test | `liquers-lib/tests/record_sync_end_to_end.rs` | a stored manifest through `EnvResolver` | AC-1, AC-2, AC-3 |

## Example 1: keeping a search engine in step with a corpus

```rust
let source = /* ManifestSource for docs/corpus.manifest.yaml, 100 keyed chunks */;
let resolver: Arc<dyn ChunkResolver> = Arc::new(EnvResolver::new(envref.clone()));
let sink = MemorySink::new(ProjectionId("bm25-en-v1".into()), Expires::InDuration(Duration::from_secs(600)));
let options = ReconcileOptions { max_chunks: 100_000, now: Utc::now(), tz_offset_seconds: 0 };

let first = reconcile(source.clone(), &sink, resolver.clone(), &options).await?;   // 100 Added
let again = reconcile(source.clone(), &sink, resolver.clone(), &options).await?;   // 100 Unchanged, no apply
// docs/a/notes.csv rewritten; chunk "docs/z.csv" removed from the manifest:
let third = reconcile(source2, &sink, resolver, &options).await?;
// 98 Unchanged, 1 Replaced(VersionChanged), 1 Deleted; one apply call
```

The source is read the way `RECORD_STREAM_GUIDE.md` §3.4 reads it
(`-R/docs/corpus.manifest.yaml/-/ns-rec/to_record_source`, validated: `Ok`).

## Example 2: upgrading an embedding model

The vector sink's projection changes from `"e5-small-v2"` to `"e5-base-v2"`. Without any document
change, the next run re-feeds all 100 chunks with `Replaced(ProjectionChanged)`; `explain` on a
chunk before that run returns `Fed { projection: "e5-small-v2", current: false, … }`.

## Example 3: a volatile dashboard chunk

A chunk produced by a volatile command has no version. First run: `Added`, entry
`TimeBased { fed_at: 09:00 }`. A run at 09:05 with `refresh = InDuration(10 min)`:
`StillFresh { fed_at: 09:00 }`. A run at 09:11: `Replaced(RefreshDue)`.

## Edge and Error Cases

| Case | Expected | Scenario |
|---|---|---|
| chunk 3's query fails | chunk 3 `Failed`, others applied, sink keeps chunk 3's old rows | AC-7 |
| same upsert applied twice | identical `MemorySink` state | AC-7 |
| `Unbounded` list, sink holds an unlisted chunk | not deleted; `deletions_detectable == false` | AC-4 |
| 11 chunks, `max_chunks = 10` | `Err` naming 11 and 10, `apply_calls() == 0` | AC-10 |
| `InMemorySource` | `Err` naming manifest sources, nothing fetched | AC-11 |
| chunk status `Expired` with the same version | re-fed, `Replaced(NotReady)` | AC-3 |
| first feed of a schema with a `Similarity` field | report carries `field_support`, `Exact` from `MemorySink` | AC-8 |

## Test Plan

**`liquers-records/tests/sync_reconcile.rs`** — a fixture `ChunkResolver` (as in
`liquers-records/src/value.rs` tests) over a `ManifestSource` built from a `ManifestSpec` with
explicit keyed chunks; the fixture returns a configurable `Metadata` (version, status, volatility)
per chunk, counts `evaluate` calls, and can fail one query. `#[tokio::test]` throughout.

- `first_feed_adds_every_chunk` (AC-1)
- `unchanged_corpus_fetches_nothing` (AC-2: `evaluate` count 0, `apply_calls` unchanged)
- `changed_version_replaces_one_chunk` and `not_ready_status_refeeds` (AC-3)
- `known_list_detects_deletion`, `unbounded_list_never_deletes` (AC-4)
- `projection_change_refeeds_all` (AC-5)
- `versionless_chunk_is_time_based` (AC-6: three runs with injected `now`)
- `failed_fetch_keeps_old_entry_and_converges`, `apply_is_idempotent` (AC-7)
- `first_feed_reports_field_support` (AC-8)
- `explain_answers_each_status` (AC-9: not in feed, not yet fed, fed current, fed stale
  projection, time-based; fixture `evaluate` count stays 0)
- `too_many_chunks_is_refused` (AC-10)
- `source_without_manifest_is_refused` (AC-11)

Command: `cargo test -p liquers-records --test sync_reconcile` (and
`cargo test -p liquers-records --all-features --lib --tests`).

**`liquers-lib/tests/record_sync_end_to_end.rs`** (`#![cfg(feature = "records")]`) —
`DefaultEnvironment<Value>` with `AsyncMemoryStore::new(&Key::new())` holding
`docs/corpus.manifest.yaml` and three CSV chunks; evaluate
`-R/docs/corpus.manifest.yaml/-/ns-rec/to_record_source` to obtain the source; `EnvResolver`;
`MemorySink`. `end_to_end_first_feed_then_no_change` (AC-1, AC-2) and
`end_to_end_rewritten_chunk_is_replaced` (AC-3: overwrite one CSV through the store, re-run, only
that chunk replaced). Command: `CARGO_INCREMENTAL=0 cargo test -p liquers-lib --test record_sync_end_to_end`.

## Learning Log

- The descriptor read is metadata-only only for stored keyed chunks; test AC-2 with keyed, stored
  chunks, or the evaluate count is not zero.
- A fetch can turn a never-evaluated chunk into a stored one; the descriptor must be re-read after
  the fetch, or the entry records `TimeBased` for a chunk that now has a version.
