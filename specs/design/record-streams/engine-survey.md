---
id: RECORD-STREAMS-ENGINE-SURVEY
kind: analysis
title: Engine survey — is the field-role model right, and can records front a relational database?
workflow: liquers-project
status: draft
area: [lib/value]
created: 2026-09-20
---
# Engine survey — is the field-role model right, and can records front a relational database?

Written against three questions raised in review of Phase 2:

1. `FieldRole` looks like a **hint**. Is its data representation right — can a field have several roles,
   possibly different per engine?
2. It talks about *indexing*. Does that cover **relational** indexing? Do hash or soundex indexes
   belong there?
3. Records are specified as a tabular **view of Liquers data**. Can the same design be an **access
   layer into relational databases** — sqlx, Postgres, MySQL, SQLite, SurrealDB?

Plus a survey of DataFusion, mem0 and OpenViking for intersections, integration aside.

## 1. The representation is nearly right, and wrong in one specific way

### `indexed` must be plural

`FieldRole { indexed: Option<IndexKind>, stored, fast }` allows a field **one** access path. Real
engines routinely give one field several, *within a single engine*:

| Engine | One field, several access paths |
|---|---|
| PostgreSQL | a B-tree **and** a `pg_trgm` GIN index on the same column — equality/range plus `LIKE '%x%'` |
| Elasticsearch | the canonical **multi-field**: a `text` field with a `keyword` sub-field, so the same value is both analyzed and exact |
| Lucene / Tantivy | indexed, docValues/fast and stored simultaneously |

So `indexed: Vec<IndexKind>`. The booleans already compose; the index kinds must too.

### Which answers "can roles differ per engine" better than per-engine roles would

The instinct behind the question is right, but per-engine role maps in the schema would be the wrong
fix — that is layer 3 leaking into layer 2, and the schema would become the union of every engine's
configuration. A CSV reader producing records does not know what engines exist downstream.

With `indexed` plural there is a better formulation, and it is a genuine change of meaning:

> **The schema declares the access paths the data *affords*. Each engine takes the subset it can
> serve.**

`FieldRole` stops being "what to do" and becomes "what is possible and wanted". A field may declare
`[Exact, Substring, FullText{…}]`; Postgres takes B-tree plus trigram, Tantivy takes STRING plus
TEXT, Qdrant takes a keyword payload index, and a CSV export takes none. Nothing needs to know what
the others did.

This composes with the sink obligation already recorded in `interoperability-layer.md`: an engine
**reports what it declined**. Declaring a union and reporting the projection is a complete story;
per-engine maps in the schema are not.

### Where this leaves the "hint" framing

A hint it is, and deliberately: nothing enforces that an engine honours a role, and a record stream
is perfectly usable with every role left at default. The one exception is the `Id` field, whose
`Exact` + stored requirement is **enforced** in `RecordSchema::new`, because reconciliation by
delete-by-term is not optional — a schema without it silently cannot be updated.

## 2. Relational indexing: mostly yes, with one addition and one exclusion

### The model describes access paths, not data structures — which is the right level

| Relational index | In this model |
|---|---|
| **B-tree** | `Exact` **and** `Range` — a B-tree is ordered, so it serves both |
| **Hash** | `Exact` only, no `Range` — precisely the functional difference |
| **GIN / GiST over full text** | `FullText { … }` |
| **BRIN** | `Range`, with weaker selectivity — a statistics concern, not a capability one |
| **Spatial (R-tree, PostGIS)** | **not represented today** — no geo type in the subset. **Expected to arrive with GIS support**; see below |

The B-tree/hash distinction falling out of `Exact` + `Range` rather than needing to be named is
evidence the abstraction sits at the right level: **say what queries are supported, not which data
structure supports them.**

### One addition: `Substring`

`LIKE '%x%'`, `pg_trgm`, and fuzzy matching are neither `Exact` nor `FullText` — full text is
tokenized, and `Substring` matches inside tokens. It is common enough in both relational and search
engines to earn a variant:

```rust
pub enum IndexKind {
    Exact,
    Substring,                                       // LIKE '%x%', trigram, fuzzy
    FullText { analyzer: Analyzer, positions: bool },
    Range,
    Similarity { metric: VectorMetric },
}
```

### Anticipated: spatial proximity, when GIS support arrives

Spatial indexes are the one relational access path with no representation here, and the absence is
**expected to be temporary** — proximity search is a likely addition once Liquers gains GIS
functionality. Recorded so it reads as deferred rather than overlooked.

Adding it needs two things, and the design already has the right shape for both:

1. A geometry **`FieldType`** — a point, and probably a bounding box — with a matching `Column`
   variant. Arrow has extension types for geometry, and a `FixedSizeList(Float64, 2)` carries a point
   without one.
2. An **`IndexKind::Proximity`** variant, parameterized much as `Similarity { metric }` is — a
   coordinate reference system in place of a distance metric.

The second costs nothing structurally, because `indexed` is already a `Vec<IndexKind>`: a geometry
column declares `[Proximity { crs }]`, or `[Exact, Proximity { … }]` where an exact match on a region
id is wanted too. The seam is already cut.

### One exclusion: functional indexes such as soundex

**Soundex does not belong in `FieldRole`, and the reason generalises.** `CREATE INDEX ON t
(soundex(name))` indexes a *derived expression*, not the field. It is a property of a `(field,
function)` pair, and admitting it would mean the schema carries expressions — at which point it is a
query language.

The Liquers-native answer is better and already available: **a derived index becomes a derived
column.** A command adds `name_soundex` with `IndexKind::Exact` via `RecordBatch::with_columns`, and
the index is then on a real field. Same for `lower(email)`, a computed bucket, or a normalised form.
The model stays flat and the derivation becomes visible, cacheable data rather than hidden engine
configuration.

**One caveat, from §3:** this holds when Liquers owns the schema. Reading *from* a database that has
functional indexes, new columns cannot be added — those indexes are the database's business and
simply do not appear in the schema.

## 3. Records as an access layer to relational databases

The two directions are **different problems**, and the design currently addresses only the first:

| | Tabular **view** of Liquers data | **Access layer** into a database |
|---|---|---|
| Who owns the schema | Liquers — declared by the producer | the database — **discovered** |
| Roles mean | what we want an index to do | what indexes **exist** |
| Chunk boundaries | chosen by the producer | must be **derived** from a key |
| Direction | read | read **and write** |

The capability framing in §1 turns out to serve both: "the access paths this data affords" reads
naturally whether the paths are wanted or observed.

### What fits

**sqlx** maps cleanly. `query(…).fetch(&pool)` returns a `BoxStream` of rows — row-at-a-time,
already the shape a batch builder consumes, with the driver managing the cursor. Postgres, MySQL and
SQLite all reach it through the same API.

**Keyset chunking works because of an invariant already required.** Paginating with
`LIMIT`/`OFFSET` degrades quadratically; keyset pagination (`WHERE id > $last ORDER BY id LIMIT n`)
does not, and it needs exactly one ordered unique key — which `RecordSchema` **already enforces**.
The `Id` field that exists for reconciliation is exactly the key that makes SQL chunking efficient.

### What does not fit — four gaps, stated rather than glossed

**1. `SourceBacking` has no variant for this.** It offers `Manifest(Vec<Query>)` and
`Materialized(…)`. A SQL source is neither: a connection plus a query, streamed lazily, whose chunk
boundaries are not known in advance. This is exactly the "partition discoverable only incrementally"
case that Phase 2 says would justify reviving a source trait — **the relational direction brings it
back.** Either a third backing or the trait is required; it cannot be dodged.

**2. `Decimal` stops being deferrable.** `NUMERIC`/`DECIMAL` is the correct type for money, and
reading a Postgres or MySQL table as `Float` is a data-corruption bug, not an approximation. The
deferral is defensible for the view direction and **is not** for the access-layer direction.
Likewise absent: `uuid`, `jsonb`, arrays, intervals, and `TIME`/`TIMESTAMPTZ` distinctions.

**3. Writing is entirely undesigned.** An access layer implies `INSERT`/`UPDATE`/`DELETE`,
transactions, and conflict handling. The whole design is read-only, and `RecordSource` has no write
counterpart. This is the largest gap and should be scoped separately rather than bolted on.

**4. SurrealDB fits least.** It is multi-model: record ids are `table:id` things rather than scalars,
documents nest, graph edges are first-class, and schemafull is optional. A `SELECT` returns rows that
project into a batch, so the tabular view works — but nested documents and edges have no
representation in a flat column model whose Arrow subset deliberately excludes `Struct` and `List`.
Usable for the flat subset; not a faithful interface to what SurrealDB is.

## 4. Three engines surveyed

### DataFusion — the closest structural match, and a better idea than ours

DataFusion's `TableProvider` **is** the `RecordSource` concept, independently arrived at: asked
repeatedly for a plan, never consumed, producing a stream of Arrow batches. Source → stream → batch,
exactly. That is strong external validation of the three-way split.

It also has one thing this design should take. `supports_filters_pushdown` returns, per filter, one
of **`Exact` / `Inexact` / `Unsupported`**:

- `Exact` — the source applied the filter fully; the engine need not re-check.
- `Inexact` — the source filtered *approximately*; the engine **must** re-apply to be correct.
- `Unsupported` — the source did nothing.

`Inexact` is the state this design lacks and needs. The sink contract currently has two outcomes
(indexed / declined), which cannot express "this engine narrowed the candidates but you must
verify" — and that is precisely the **filter-then-verify** shape the search design wants for an
engine whose tokenization does not match the predicate exactly. **Recommendation: adopt the
three-state model in the sink contract**, in `interoperability-layer.md`.

Two further notes:

- DataFusion's stream carries `schema()`; this design puts the schema on the chunk descriptor
  instead, because chunks are **not** promised to share one. The divergence is deliberate and is the
  price of not promising uniformity.
- DataFusion's `Statistics` (row counts, null counts, min/max per column) drive optimization. This
  design has none. Not needed now; the natural home later is `ChunkDescriptor`, where min/max per
  chunk would allow skipping chunks a predicate cannot match — the same trick as parquet row-group
  pruning.

### mem0 — validates the schema shape, and suggests a content hash

A mem0 memory carries `id`, `memory` (the text), `hash`, `metadata`, `user_id`, `agent_id`, `app_id`,
`run_id`, `created_at`, `updated_at`, plus an embedding.

Two intersections:

- **Scope fields are first-class, not generic metadata.** `user_id`/`agent_id`/`run_id` sit beside
  the id rather than inside a bag. That is the same shape as `KeyRole` plus qualified column names,
  and it is what makes filtering cheap and exact.
- **`hash` is a content hash, for deduplication and change detection.** This design has `Version` per
  *chunk* but nothing per *record*. A per-record content hash would let reconciliation skip unchanged
  records inside a changed chunk — a real optimization for a large chunk with few edits. Worth an
  optional `KeyRole::ContentHash` or a conventional column; **recorded, not adopted**, since chunk
  granularity is sufficient until measured otherwise.

mem0's documentation also warns that metadata keys must match exactly between write and search or the
filter silently fails — which is the failure mode this design's **qualified names and
ambiguity-is-an-error** rule exists to prevent. Independent confirmation that the strictness is
worth its cost.

### OpenViking — the most surprising convergence

ByteDance/Volcengine's context database for AI agents organises context as a **virtual filesystem**
under `viking://`, with `resources/`, `user/{id}/memories/`, `skills/` and `peers/`, operated on with
`ls`, `tree`, `read`, `write`, `find` and `grep`. Retrieval is hybrid: vector search finds candidate
*directories*, then traversal explores their contents.

Liquers already is a key-addressed store with exactly that shape, which makes the convergence worth
noting even with no integration in view: a hierarchical namespace plus similarity search over it is
apparently what agent memory converges on, and this design's `key.path` field beside a `Similarity`
index expresses the same hybrid.

**The loading tiers are the idea worth noting — and Liquers already has two of the three.** Each
OpenViking entry carries three depths: **L0** a one-sentence abstract for relevance checks, **L1** an
overview for planning, **L2** the full data, read only when needed.

Liquers' metadata already carries `title` and `description` — on `AssetInfo` (`metadata.rs:678`) and
`MetadataRecord` (`:871`) — and they play **effectively the same roles**: a title is the one-line
abstract a consumer judges relevance by, a description is the overview it plans with, and the asset
itself is the detail. So the tiering this design would want is not a new mechanism: it is
`meta.title` and `meta.description` as projected columns, with the locator or chunk query as the
route to L2.

That reframes the idea rather than dismissing it. What OpenViking adds is not the *depths* but the
discipline of **declaring them as a contract** — a consumer knowing it may scan at L0 without paying
for L2, and a producer knowing it must supply L0 cheaply. Whether a record source should declare
depth levels explicitly is the open question; the fields to carry them already exist.

Licence note: OpenViking is **AGPLv3**, which would matter for integration, not for borrowing a
structural idea.

## Summary of recommended changes

| # | Change | Where |
|---|---|---|
| 1 | `indexed: Option<IndexKind>` → `Vec<IndexKind>`; reframe roles as **capabilities the data affords**, which engines project | Phase 2, `FieldRole` |
| 2 | Add `IndexKind::Substring` for `LIKE`/trigram/fuzzy | Phase 2, `IndexKind` |
| 3 | State that access paths, not data structures, are described — B-tree is `Exact + Range`, hash is `Exact` | Phase 2 |
| 4 | State that functional indexes (soundex, `lower()`) are **derived columns**, not roles | Phase 2 |
| 5 | Adopt DataFusion's **Exact / Inexact / Unsupported** for the sink contract | `interoperability-layer.md` |
| 6 | Record that the relational direction **requires** a third `SourceBacking` or the source trait | Phase 2 open questions |
| 7 | Record that `Decimal` is **not deferrable** for the access-layer direction | Phase 2 open questions |
| 8 | File the relational access layer as its own issue — writes and schema discovery are out of scope here | `specs/issues/` |
| 9 | OpenViking depth tiers: `title`/`description` **already serve as L0/L1**, so the open part is whether to *declare* depths as a contract, not to add fields | Phase 3 |
| 11 | **Spatial proximity is anticipated** with GIS support — a geometry `FieldType` plus `IndexKind::Proximity`, which `Vec<IndexKind>` already accommodates | recorded above |
| 10 | Record a per-record **content hash** as a possible reconciliation optimization | Phase 3 |
