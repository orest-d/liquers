# Phase 2: Solution & Architecture — The reconciliation contract for external indexes

## Overview

A new module `liquers-records/src/sync.rs` defines the sink side (`IndexSink`), the entries a sink
holds (`ObservedEntry`), the diff (`reconcile`), the explanation (`explain`) and a test double
(`MemorySink`). `reconcile` lists the source's chunks, reads each descriptor through the caller's
`ChunkResolver`, compares it with the sink's observed entry, fetches only what must be re-fed (the
chunk's query through the resolver, then `view_from_chunk_value` with the source's declared
schema), and hands upserts and deletions to the sink in one `apply` call. Everything is async on
the resolver's futures; nothing is spawned, so it runs the same on native and wasm.

Rejected:
- **A push hook** (the LiQuer Whoosh prototype): correctness would depend on delivery
  (`interoperability-layer.md` §1). Push stays an optional accelerator (`external-index-sync`).
- **Record-level diff:** a record has no version of its own; the chunk is the smallest unit whose
  staleness is decidable from metadata (`interoperability-layer.md` §3).
- **Projection identity in the chunk version:** Phase 1 Q1.
- **Placing the contract in `liquers-lib`:** a browser or Python sink would then need the rich
  library; `liquers-records` is core-only and wasm-clean.
- **A command now (`ns-rec/sync`):** a command needs a configured sink, and there is none yet; the
  first real sink brings its configuration and the command together.

## Known-Issue Preflight

| Issue | Status | Priority | Impact on this design | Fix first? | Blocking? | Action |
|---|---|---|---|---|---|---|
| `UNKEYED-ASSET-HAS-NO-VERSION` | draft (filed 2026-10-26) | P3 | unkeyed chunks fall into the time-based regime | no | no | AC-6 covers it |
| `ASSET-EXPIRATION-EVENTS-CANNOT-BE-OBSERVED-EXCEPT-PER-ASSET` (`external-index-sync`) | draft | P2 | push path only | no | no | overlaps |
| `UNCACHED-STORED-COPY-EXPIRY-RACES-AN-INFLIGHT-EVALUATION` | draft | P3 | a descriptor read during a racing write may report the older version; the next run corrects it | no | no | stated in the reference |

## Interfaces

```rust
// liquers-records/src/sync.rs  (pub mod sync; re-exported from lib.rs)

/// What a sink's projection rule is: tokenizer, analyzer, embedding model and version, field
/// mapping. Opaque; equality is all that matters. A sink changes it when its rule changes.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ProjectionId(pub String);

/// How an entry vouches for its freshness.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EntryFreshness {
    /// The chunk's content version when it was fed.
    Versioned(Version),
    /// The chunk had no version; it was fed at this time and is refreshed by schedule.
    TimeBased { fed_at: DateTime<Utc> },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObservedEntry {
    pub chunk: ChunkId,
    pub freshness: EntryFreshness,
    pub projection: ProjectionId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldSupport { Exact, Inexact, Unsupported }

/// One chunk to (re)place. Its rows replace every row the sink holds for `chunk`.
#[derive(Debug, Clone)]
pub struct ChunkUpsert {
    pub chunk: ChunkId,
    pub freshness: EntryFreshness,
    pub rows: Arc<dyn RecordView>,
}

/// The external side. Implementations must make `apply` idempotent: applying the same upserts and
/// deletions twice leaves the same state.
pub trait IndexSink: MaybeSend + MaybeSync {
    fn projection(&self) -> ProjectionId;
    /// How stale a time-based entry may become (`Expires::InDuration(…)` typically).
    fn refresh(&self) -> Expires;
    fn observed(&self) -> BoxFuture<'_, Result<Vec<ObservedEntry>, Error>>;
    fn field_support(&self, schema: &RecordSchema) -> Vec<(String, FieldSupport)>;
    fn apply(
        &self,
        projection: ProjectionId,
        upserts: Vec<ChunkUpsert>,
        deletions: Vec<ChunkId>,
    ) -> BoxFuture<'_, Result<(), Error>>;
}

#[derive(Debug, Clone)]
pub struct ReconcileOptions {
    pub max_chunks: usize,          // default 100_000
    pub now: DateTime<Utc>,         // injected for tests; callers pass Utc::now()
    pub tz_offset_seconds: i32,     // for Expires::AtTimeOfDay / OnDayOfWeek
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefeedReason { VersionChanged, VersionUnknown, NotReady, ProjectionChanged, RefreshDue }

#[derive(Debug, Clone)]
pub enum ChunkOutcome {
    Added,
    Replaced(RefeedReason),
    Deleted,
    Unchanged,
    /// Time-based and not yet due.
    StillFresh { fed_at: DateTime<Utc> },
    Failed(Error),
}

#[derive(Debug, Clone)]
pub struct ReconcileReport {
    pub outcomes: Vec<(ChunkId, ChunkOutcome)>,
    pub deletions_detectable: bool,               // false for ChunkList::Unbounded
    pub field_support: Option<Vec<(String, FieldSupport)>>,  // on the first feed only
}

#[derive(Debug, Clone)]
pub enum ChunkStatus {
    NotInFeed,
    NotYetFed,
    Fed { freshness: EntryFreshness, projection: ProjectionId, current: bool },
    Refused(Error),     // describe failed
}

pub async fn reconcile(
    source: Arc<dyn RecordSource>,
    sink: &dyn IndexSink,
    resolver: Arc<dyn ChunkResolver>,
    options: &ReconcileOptions,
) -> Result<ReconcileReport, Error>;

pub async fn explain(
    source: Arc<dyn RecordSource>,
    sink: &dyn IndexSink,
    resolver: &dyn ChunkResolver,
    chunk: &ChunkId,
    options: &ReconcileOptions,
) -> Result<ChunkStatus, Error>;

/// Test double: holds rows per chunk in memory; `field_support` answers `Exact` for every field.
pub struct MemorySink { /* Mutex<BTreeMap-like state>, projection, refresh, apply_calls */ }
impl MemorySink {
    pub fn new(projection: ProjectionId, refresh: Expires) -> Self;
    pub fn set_projection(&self, projection: ProjectionId);
    pub fn rows(&self, chunk: &ChunkId) -> Option<Arc<dyn RecordView>>;
    pub fn apply_calls(&self) -> usize;
    /// Makes the next `apply` fail, to test AC-7.
    pub fn fail_next_apply(&self, error: Error);
}
impl IndexSink for MemorySink { … }
```

`Version` and `Expires` come from `liquers-core` (`metadata`, `expiration`); `DateTime<Utc>` from
`chrono`, already a `liquers-records` dependency at the version `liquers-core` uses. `MemorySink` keeps its state behind
`std::sync::Mutex` (no `.await` while held). `ChunkId` is `Hash + Eq`, so observed entries are
indexed in a `HashMap`.

### The diff (inside `reconcile`)

1. `source.manifest()` is `None` → `Err` (AC-11). `chunks()`: `Known(ids)` → feed set, deletions
   detectable; `Unbounded { computed }` → feed set = `computed`, deletions not detectable. More
   than `max_chunks` → `Err` naming both numbers (AC-10).
2. `observed = sink.observed().await?` into a map. `projection = sink.projection()`.
3. For each feed chunk, `describe_chunk(id, resolver)`; on `Err` → `Failed`. Decide:
   - no entry → fetch, `Added`;
   - entry projection ≠ `projection` → fetch, `Replaced(ProjectionChanged)`;
   - status not `Ready`/`Source`/`Override` (metadata status) → fetch, `Replaced(NotReady)`;
   - metadata version `Some(v)`: entry `Versioned(v)` → `Unchanged`; otherwise fetch,
     `Replaced(VersionChanged)` (a `TimeBased` entry whose chunk now has a version also re-feeds,
     reason `VersionChanged`);
   - version `None` (volatile or unkeyed): entry `TimeBased { fed_at }` and
     `sink.refresh().to_expiration_time(fed_at, tz).is_expired_at(now)` false → `StillFresh`;
     otherwise fetch, `Replaced(RefreshDue)` or, with a `Versioned` entry, `Replaced(VersionUnknown)`.
4. Fetch: `resolver.evaluate(descriptor.query)` → `view_from_chunk_value(value, source.schema())`;
   then re-read the descriptor so the upsert carries the version the fetch produced (a fetch can
   produce a never-evaluated chunk); `Some(v)` → `Versioned(v)`, `None` → `TimeBased { fed_at:
   now }`. A fetch error → `Failed`, no upsert, the sink's old entry stays (AC-7).
5. Deletions: when detectable, every observed chunk not in the feed set → `Deleted`.
6. `field_support`: when `observed` was empty and `source.schema()` is `Some`, ask the sink once.
7. One `sink.apply(projection, upserts, deletions)` call when either is non-empty. An `apply`
   error is returned as `Err` (nothing in the report is claimed applied).

Fetches run sequentially in feed order in this slice (memory: one chunk at a time plus the
upserts gathered so far). Bounded concurrency and batched `apply` calls (every k chunks) are
implementation choices for a later slice; the contract does not change.

## Integration Points

- `liquers-records/src/lib.rs`: `pub mod sync;` and re-exports.
- `liquers-records/Cargo.toml`: none — `chrono` and `futures` are already dependencies.
- `liquers-records/src/sources.rs`: none; `view_from_chunk_value` and `EnvResolver` are already
  `pub`.
- `liquers-lib`, `liquers-axum`, `liquers-web`, `liquers-py`: none in this slice.

## Error Handling

| Failure | Result |
|---|---|
| no manifest; more than `max_chunks` | `Err(Error::general_error(…))` before any fetch |
| `sink.observed()` fails | `Err`, nothing fetched |
| a descriptor or a fetch fails | that chunk `Failed(error)`; others continue |
| `sink.apply` fails | `Err(error)`; the next run re-diffs from what the sink reports |

No `unwrap`/`expect` in library code; every match on `ChunkList`, `EntryFreshness`, metadata
`Status` is explicit.

## Relevant Commands

None in this slice; `specs/command_registry.yaml` is unchanged.

## Documentation Architecture

`specs/reference/RECORD_STREAMS.md`: new section "Reconciliation with external indexes" (types,
the diff table above, time-based regime, AC-11 limit, `Unbounded` limit). `specs/guides/
RECORD_STREAM_GUIDE.md`: new §6.7 "Writing an index sink". History rows and `reviewed:` bumps.

## Risks

| Assessment | Finding |
|---|---|
| Files likely to change | `liquers-records/src/{sync.rs,lib.rs}`, `liquers-records/tests/sync_reconcile.rs` (new), `liquers-lib/tests/record_sync_end_to_end.rs` (new), two documents |
| Crates and workflows affected | `liquers-records` only; no existing behaviour changes |
| Existing tests likely to change | none |
| New validation | Phase 3 suite with `MemorySink` and fixture resolvers; one `liquers-lib` end-to-end test through `EnvResolver` and a stored manifest |
| Compatibility / data / concurrency / performance / security | additive public API; contract choices (Q1, Q4) are hard to change once sinks exist; describe for unkeyed chunks evaluates (cost proportional to the corpus for such sources); the sink sees whatever the source yields (no access control); a single `apply` per run holds all upserts in memory — acceptable for the slice's corpora (hundreds to low thousands of chunks), to be batched later |
| Recovery | remove the module; nothing depends on it |
| Certainty and open questions | high on mechanics (every type exists at HEAD); medium on the exact metadata status accessor; Q1, Q3, Q4, Q7 are contract decisions |
