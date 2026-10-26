# Phase 1: High-Level Design — The reconciliation contract for external indexes

## Design Readiness

- **Readiness:** needs-decision
- **Automatic fixing:** not-eligible — new public trait, structs and functions in
  `liquers-records` (auto-fix rules 3, 4); `XL` source (rule 1).
- **Leading issue:** **Open design - where projection identity lives** (Q1).
- **Explanation:** the feed already exists at HEAD — `RecordSource::chunks()` lists chunks without
  reading data and `describe_chunk` returns each chunk's `Metadata` (version, status, volatility) —
  so the slice is a pure diff over two `(chunk, version)` sets plus a fetch, with a test double.
  The issue's seven questions all have evidence-backed recommendations; they stay visible as
  decisions because they fix the public contract every future engine sink implements.
- **Open questions** (the issue's seven, in its numbering; severity order Q1, Q7, Q4, Q3, Q5, Q6, Q2):
  1. **Open design - projection identity:** declared by the **sink**
     (`IndexSink::projection() -> ProjectionId`, e.g. tokenizer + embedding model) and stored on
     every observed entry; a mismatch re-feeds the chunk. **Recommended.** Alternative: fold it
     into the chunk version (`interoperability-layer.md` §7) — then two sinks with different models
     need two feeds, and a report cannot say *why* a chunk was re-fed.
  2. **Proposed resolution - test double first:** yes. `MemorySink` ships `pub` (not test-only) so
     an engine crate tests its sink against the same scenarios.
  3. **Open design - `produce`:** for a record-source feed, fetching a chunk *is* evaluating its
     query, so production is inherent and there is no `when-ready` mode in this slice
     (`indexation-policy.md` §1, "indexation may evaluate"). A store-scan corpus with a
     configurable content policy is a later slice. Recommended: accept that, and say so.
  4. **Open design - volatile and versionless entries:** a chunk whose metadata has no version
     (volatile, or an unkeyed query chunk — see `UNKEYED-ASSET-HAS-NO-VERSION`) is entered as
     *time-based* with the time it was fed, and re-fed when the sink's `Expires` budget has passed
     since then; reports and `explain` say "time-based", never a version. Recommended.
  5. **Proposed resolution - bounding and deduplication:** a source's chunk list is the declared
     set; `ReconcileOptions::max_chunks` (default 100 000) refuses, before any fetch, a feed that
     lists more. No deduplication: a chunk id is an identity, so two spellings of one computation
     are two entries (documented limitation).
  6. **Proposed resolution - "why is this not in my results?":** `explain(chunk)` answers one of:
     not in the feed; in the feed, not yet fed; fed at version *v* under projection *p*; fed
     time-based at *t*, refresh due or not; last fetch failed with *e*.
  7. **Open design - first deliverable:** the contract, the diff and `MemorySink` — **recommended**
     (`interoperability-layer.md` §8.5: "a trait with one implementation is a guess"). Not the
     in-tree per-document filter index, which is an asset, needs no reconciliation, and can be
     built independently.

## Purpose

A full-text engine, a vector store, a RAG pipeline or an external SQL mirror is a materialized
view of a record source living outside Liquers. Give such a view one contract for being fed and
staying correct — a diff of `(chunk, version, projection)` between what the source offers and what
the sink holds — so that only changed chunks are re-read, deletions are detected, and a model
upgrade cannot leave an index looking fresh. Serves VISION priority area 2 (records) and the
"trust the result" principle: correctness comes from reconciliation, push only reduces latency.

## Problem Example

`docs/corpus.manifest.yaml` lists 100 keyed chunks (one per document folder). A team feeds them to
a search engine. Today (HEAD cd6d5da) nothing tells them which chunks changed: the in-tree
`ns-search` commands scan on every query and keep nothing, and Liquers offers no way to compare
what an engine holds with what the source offers. Their only option is a full re-index on a
schedule — 100 chunk evaluations and 100 engine writes per run — and a chunk removed from the
manifest stays in the engine until someone rebuilds it.

Should be: `reconcile(source, &sink, resolver, &options)` after one chunk's file changed and one
chunk was removed from the manifest reads 100 chunk descriptors (metadata only), fetches **one**
chunk, and applies one replacement and one deletion; the report lists 98 `Unchanged`,
1 `Replaced(VersionChanged)`, 1 `Deleted`. A second run fetches nothing.

## Scope and Acceptance Criteria

- **AC-1** First feed
  WHEN an empty sink is reconciled with a source of N known chunks
  THEN every chunk is fetched once and applied as an upsert; the report has N `Added`
- **AC-2** No change, no work
  WHEN the same source (keyed, stored chunks) and sink are reconciled again with nothing changed
  THEN no chunk query is evaluated, `apply` receives nothing, and the report has N `Unchanged`
- **AC-3** A changed chunk is replaced wholesale
  WHEN one chunk's stored version differs from the observed one, or its status is not ready
  THEN only that chunk is fetched, and the sink's rows for it are replaced, not merged
- **AC-4** Deletions where the list is known
  WHEN the sink holds a chunk the source's `ChunkList::Known` no longer lists
  THEN it is deleted; WHEN the list is `Unbounded` THEN nothing is deleted and the report says
  deletions are not detectable
- **AC-5** Projection change re-feeds
  WHEN the sink's `projection()` differs from the projection stored on its entries
  THEN every chunk is re-fed with reason `ProjectionChanged`, although no version changed
- **AC-6** Versionless chunks are time-based
  WHEN a chunk's metadata has no version
  THEN it is fed and entered as time-based with its feed time, re-fed only once the sink's
  refresh `Expires` has passed since then, and reported as time-based
- **AC-7** Failures are per chunk and runs converge
  WHEN one chunk's fetch fails
  THEN the others are applied, that chunk is `Failed` with the error, the sink keeps its previous
  entry for it, and a later run with the error gone ends with every chunk `Unchanged` or applied;
  applying the same upsert twice leaves the sink identical
- **AC-8** Field support is reported
  WHEN a sink is first fed a schema
  THEN the report carries its `Exact` / `Inexact` / `Unsupported` answer per field
- **AC-9** One chunk can be explained
  WHEN `explain` is asked about a keyed, stored chunk
  THEN it returns one of the Q6 answers from the chunk list, the chunk's descriptor and the sink's
  observed set, without evaluating anything
- **AC-10** The declared set is bounded
  WHEN a source lists more chunks than `max_chunks`
  THEN reconciliation fails before any fetch, naming both numbers
- **AC-11** Only sources whose chunk queries yield their rows
  WHEN the source has no manifest (an in-memory, filtering or mapping source)
  THEN reconciliation fails before any fetch, explaining that per-chunk refresh needs a manifest
  source

Non-goals (later slices or other items): real engine sinks (Tantivy, Meilisearch, Qdrant, an SQL
mirror — `relational-database-writes` decides the last); a command, configuration or scheduler that
runs reconciliation; the push path (`external-index-sync`); store-scan corpora with inclusion
patterns and a `metadata-only` / `when-ready` / `produce` policy; the in-tree word-filter index;
hierarchical digests; several Liquers instances writing one sink; access control at the sink;
deduplicating query spellings; wrapper sources (AC-11).

## Core Interactions

- **Records:** a new `sync` module over `RecordSource`, `ChunkList`, `ChunkDescriptor`,
  `ChunkResolver`, `view_from_chunk_value`.
- **Assets / metadata:** read only — `Metadata::version()`, `status`, `is_volatile`, through
  `describe_chunk`; `Expires` for the refresh budget.
- Query, commands, store, web: none in this slice.

## Crate Placement

`liquers-records` only: it owns the source abstraction, depends on `liquers-core` only and is
wasm-clean, so an engine sink in any crate (native or browser) can implement the trait without
`liquers-lib`. No feature flag: the module has no new dependency.

## Documentation Intent

- Reference: extend `specs/reference/RECORD_STREAMS.md` with a "Reconciliation with external
  indexes" section (the contract, the outcomes, the time-based regime, AC-11's limit).
- Guide: extend `specs/guides/RECORD_STREAM_GUIDE.md` with "Writing an index sink" (implementing
  `IndexSink`, testing it against `MemorySink`'s scenarios).
- Other documents: none; `external-index-sync`'s analysis documents stay as they are.

## Design Dependencies

- `record-streams` (complete) — **requires**: `RecordSource`, `ChunkList`, `describe_chunk`,
  `ChunkResolver`, `view_from_chunk_value`.
- `external-index-sync` (finished Phase 4) — **overlaps**: its scoped expiration events are the
  later push path that shortens staleness; nothing here waits for it.
- `relational-database-writes` — **required-by**: its recommended SQL mirror is an `IndexSink`.
- `asset-manager-store-sync` — **overlaps** (weak): reconciles an asset manager with its store, a
  different pair; no shared type.
- `record-source-row-streaming` — **overlaps** (weak): both traverse a source outside a command
  with an `EnvResolver`.
- `UNKEYED-ASSET-HAS-NO-VERSION` (issue, filed 2026-10-26, no design) — **overlaps**: until it is
  fixed, unkeyed query chunks reconcile on the time-based regime (AC-6) instead of by version.

## Consolidated Findings

- **The feed already exists.** `chunks()` + `describe_chunk` give `(id, version, status,
  volatility)`; the slice adds a diff, a fetch and a trait — no change to `liquers-core` or to the
  record model. "Metadata alone" holds for keyed, stored chunks only: `EnvResolver::metadata`
  *evaluates* a keyed chunk not yet stored and every unkeyed chunk (`liquers-records/src/sources.rs`),
  so for those the diff costs an evaluation (cached by the asset manager). AC-2 and AC-9 are
  scoped accordingly.
- **Two regimes, both explicit:** versioned entries diff by version; versionless entries (volatile,
  and at HEAD every unkeyed chunk, because `AssetRef::prepare_version` skips keyless assets) are
  time-based. The second is broader than the analysis assumed, hence the new issue.
- **AC-11 limits the slice to manifest sources**, whose chunk query reproduces exactly the chunk's
  rows. A filtered or mapped source's chunk query yields the *unfiltered* rows, so per-chunk
  refresh would feed the wrong data; such sources need the wrapper applied per chunk (later).
- **`Unbounded` lists** reconcile append-only over the computed prefix; a template manifest's
  later chunks are found only by a walk, which this slice does not do.
- **Contract choices fixed here** (Q1, Q3, Q4, Q7) bind every future sink; that is why the design is
  `needs-decision` although nothing in it is uncertain technically.
