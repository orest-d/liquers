# Phase 2: Solution & Architecture — Store and asset search

> **Revision 3.** Revision 1 put the source inside the predicate, added `select` to two core traits
> and gave every hit an `AssetInfo`; revision 2 corrected those. Revision 3 removes the `Record`
> struct itself: rows are positional and the **schema** owns field names, types and roles, modelled
> on the systems this design integrates with. §0 records every change, because three of them reverse
> Phase 1 decisions.

## Overview

A search is a **predicate applied to a stream of records**. Records are produced by **commands**; the
stream's identity is the query that produces it. `liquers-core` gains a record module — records,
batches, chunks, a stream trait, a predicate — and **no new method on `AsyncStore` or
`AssetManager`** beyond repairing `get_asset_info` so that describing an asset stops starting it.
`liquers-lib` gains an `ns-search` namespace whose commands are one clause each.

Scope is milestones **M0–M3** of [`roadmap.md`](./roadmap.md) plus the `get_asset_info` repair.

## 0. What changed, and what it reverses

| Change | Why |
|---|---|
| **`root` and `sources` leave the predicate** | The predicate is a filter; *what it filters* is the stream. Putting the source inside it smuggled the set into the filter, contradicting this design's own model ("select records **from a set**"). The source is a **query**, which also gives the two execution paths: evaluate it and filter the stream, or hand the query to an external engine that already processed it. |
| **`SearchSource` deleted** | With the source being a query, command discovery is a command producing records from the registry — not an enum variant. One less concept. |
| **`AsyncStore::select` and `AssetManager::select` dropped from the MVP** | **Reverses Phase 1 axis B2/B3.** If records come from commands, a trait method is a *push-down optimization*, not the mechanism. Dropping it removes every change to two widely-implemented traits, removes the conformance-rule family, and forecloses nothing: the method can be added when a backend can actually exploit it. |
| **Fields are `serde_json::Value`, not `liquers_core::value::Value`** | A bug in the first draft: `Value` is **704 bytes**, so a `BTreeMap<String, Value>` per record is indefensible — and the draft cited `CORE-VALUE-ENUM-OVERSIZED` two sections earlier. JSON values are small, need no type parameter, and make a record set directly serializable as JSON or NDJSON, which is the tabular-interchange goal. |
| **`Hit` replaced** | It embedded an `AssetInfo`, which assumes one record per asset. **A CSV row has no `AssetInfo`; the file does.** Asset description moves to a per-source side table, and retrieval is explicit. |
| **`FieldMatch` → `ClauseMatch`** | Evidence now names *which clause* matched, so "why did this match" answers against the predicate rather than floating free. |
| **A record stream is added** | This is what `select` was standing in for. Batches and chunks are the missing abstraction — and it is `futures::Stream`, already a `liquers-core` dependency, rather than a bespoke trait. |
| **Rev. 3: the `Record` struct is removed** | Singling out `text` matches no system we integrate with: Tantivy and Lucene build a schema from fields with *options* and privilege no text field. `source` and `record_id` are likewise fields with roles. Rows become positional and the schema owns names, types and roles. |
| **Rev. 3: field names live in the schema, once per batch** | A map per row re-creates every field name for every record. Positional rows are what Arrow, polars and every database do, and a field name then resolves to an index once per query instead of per row. |
| **Rev. 3: `FieldValue` replaces `serde_json::Value`** | Measured 24 bytes against JSON's 32 — smaller *and* more expressive. JSON cannot carry bytes without base64, a timestamp as a type, or a 1536-dimension vector compactly, and the last is the RAG milestone's central case. Not a type parameter: that would infect the stream, the predicate and `Value` itself, and every referenced system uses a dynamic type enum instead. |

The net effect is a **smaller** core change than revision 1 and a **larger** record model.

## Known-Issue Preflight

Searched: every non-terminal `issue`/`feature` in `specs/index.csv` whose `area` intersects
`core/store`, `core/assets`, `core/value`, `core/commands`, `core/context`, `core/query`,
`core/plan`, `lib/commands`, `macro`, `store/backends`, `axum`, `web` — 93 records.

| Issue | Status | Pri | Relevance and solution impact | First? | Blocking? | Required action | Priority action |
|---|---|---|---|---|---|---|---|
| `DESCRIBING-AN-ASSET-CAN-TRIGGER-ITS-EVALUATION` | draft | P1 | The record-producing commands describe assets; this call must not schedule. | yes | no | **Fixed here** | keep P1 |
| `CORE-VALUE-ENUM-OVERSIZED` | draft | P2 | `Value` is 704 bytes. Drove fields to JSON and the new variant behind `Arc`. | no | no | Honoured throughout | keep P2 |
| `STORE-NO-CONTENT-OR-METADATA-SEARCH` | draft | P2 | Asks for selection *on the store*. Revision 2 does **not** close it — the user-facing gap closes, the trait gap does not. | no | no | Update it in Phase 5 to record that the capability now exists above the store, and that push-down remains open | keep P2 |
| `CORE-METADATA-NO-APPLICATION-ATTRIBUTES` | draft | P2 | Front-matter fields have nowhere to live. | no | no | Fields resolve by qualified name, so fixing it is a pure upgrade | keep P2 |
| `COMMAND-CONTEXT-PARAM-ORDER` | accepted | P2 | `context` must be last. | no | no | Honoured | keep P2 |
| `COMMAND-CANNOT-BE-RUN-WITH-A-RESTRICTED-CONTEXT` | draft | P2 | Record-producing commands are ordinary commands with a `Context`; nothing stops one evaluating. | no | no | Purity is a contract until it lands | keep P2 |
| `STORE-COMMAND-NAMESPACE-MISSING` | accepted | P3 | Record-producing commands read the store. | no | no | Independent; `ns-search` owns its own commands | keep P3 |
| `CORE-FILE-STORE-LISTDIR-DROPS-METADATA-ONLY-KEYS` | draft | P2 | A record-producing command over a directory inherits the divergence. | no | no | Document it in `SEARCH.md`: the record set is exactly what enumeration reported | keep P2 |
| `AXUM-ASSETS-API-ENDPOINTS-NOT-IMPLEMENTED` | draft | P0 | — | no | no | No endpoint added | keep P0 |
| `ASSETS-CANNOT-BE-DECLARED-NON-PERSISTENT`, `DIRECTORY-LISTING-DEPENDENCY-…`, `VALUE-SERIALIZATION-HAS-NO-INCREMENTAL-WRITER`, `CORE-STORE-OPENBIN-MISSING`, `ASSET-EXPIRATION-EVENTS-…` | draft/accepted | P2–P3 | M4–M7 only | no | no | Monitor | keep |
| `CORE-SESSION-AND-KEY-ACL` | accepted | P2 | Excluded by design | no | no | — | keep P2 |
| `STORE-TEST-IDS-COLLIDE-WITH-CONFORMANCE-RULE-IDS` | draft | P2 | **No longer relevant** — revision 2 adds no conformance rules | no | no | — | keep P2 |

**No blocker.**

## Data Structures

New module `liquers-core/src/records.rs`.

### Rows, not records: the schema owns the field names and the roles

**Revision 3 removes the `Record` struct.** The review's argument is decisive and matches every
system this design wants to integrate: Tantivy and Lucene build a schema from *fields with options*
and privilege no single text field; Arrow and polars have `Schema` + positional arrays; GlueSQL has
columns; Qdrant has payload fields plus named vectors. A struct with a privileged `text`, `source`
and `record_id` maps onto none of them.

```rust
/// A batch of rows sharing one schema. The unit of memory, and a table.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RecordBatch {
    /// Field names and roles live here — **once per batch, not once per row**.
    pub schema: Arc<RecordSchema>,
    /// Dictionary of sources; a field with role `Source` indexes it.
    pub sources: Vec<SourceInfo>,
    pub rows: Vec<Row>,
}

/// Positional, aligned to `schema.fields`. Arrow's model minus the columnar layout.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Row(pub Vec<FieldValue>);
```

**This answers the sharpest objection:** a map per row re-creates every field name for every record.
Positional rows store the names **once, in the schema**, which is what Arrow, polars and every
database do — and it makes the predicate faster as a side effect, because a field name resolves to an
index once against the schema and each row test is then a positional access.

The cost is honest: a row is meaningless without its schema, so constructing one ad hoc takes more
ceremony (a `RecordBatchBuilder` owns that), and **heterogeneous rows are handled at batch
granularity** — the schema is per *batch*, so a set may hold batches with different schemas. That is
Arrow's answer too.

### FieldValue — not JSON, and not a type parameter

```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum FieldValue {
    Null,
    Bool(bool),
    Int(i64),
    UInt(u64),
    Float(f64),
    Text(Arc<str>),
    Bytes(Arc<[u8]>),
    /// Epoch microseconds — Arrow's `Timestamp(Microsecond)`.
    Timestamp(i64),
    List(Arc<[FieldValue]>),
    Object(Arc<RecordBatch>),
    /// First-class because the alternative is a JSON array of 1536 numbers.
    Vector(Arc<[f32]>),
}
```

**Measured: 24 bytes**, against `serde_json::Value`'s 32. So the dedicated enum is *smaller* than
JSON as well as more expressive — it can carry bytes without base64, a timestamp as a type rather
than a convention, and a vector compactly, which the RAG milestone needs and which JSON does badly.

**Not a type parameter.** `Record<V>` would infect `RecordBatch<V>`, `RecordBatchStream<V>`,
`SearchPredicate<V>` and then `Value::Records(Arc<RecordSet<V>>)` — circular, since `Value` is the
obvious `V`. Every system in the reference list reached the same conclusion: Arrow, GlueSQL, Tantivy
and Qdrant all use a **dynamic** type enum rather than generics, because the cell type is data, not a
compile-time parameter.

### RecordSchema — modelled on the systems to be integrated

Two orthogonal axes, because no single system has both and the union needs both: a **logical type**
(what Arrow, polars and GlueSQL care about) and a **role** (what Tantivy, Lucene and Qdrant care
about).

```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RecordSchema {
    pub fields: Vec<FieldSchema>,
    /// Liquers' own type identity for the *thing the rows describe*, when there is one.
    pub type_identifier: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FieldSchema {
    pub name: String,
    pub data_type: FieldType,
    pub role: FieldRole,
    pub nullable: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FieldType { Bool, Int, UInt, Float, Text, Bytes, Timestamp, List, Object, Vector }

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FieldRole {
    /// The record's identity within its source. Exactly one per schema.
    Id,
    /// Index into `RecordBatch::sources`. At most one per schema.
    Source,
    /// Tokenized and matched by a text clause. **Any number** — this is the review's point:
    /// a title, a body and a comment are all text, and none is privileged.
    Text,
    /// Exact match and facet; never tokenized.
    Keyword,
    /// Returned, never searched.
    Stored,
    /// Range clauses.
    Numeric,
    /// Similarity clauses.
    Vector,
    /// Carried and ignored by every consumer.
    Ignored,
}
```

**The guarantee moves from the type to the schema, and is checked once.** Phase 1's F1 required
identity to be structural rather than conventional; with rows that guarantee cannot live in the row
type, so `RecordSchema::new` **fails** unless exactly one field has role `Id`, and accessors
(`schema.id_field()`, `schema.source_field()`, `schema.text_fields()`) keep consumers from indexing
by string. That is how Tantivy does it: the schema is validated when built and hands out field
handles.

**How each target maps:**

| System | Mapping |
|---|---|
| **Tantivy / Lucene** | `FieldRole` *is* the field options — `Text`→TEXT, `Keyword`→STRING, `Stored`→STORED, `Numeric`→fast field |
| **Arrow / polars** | `FieldSchema` → `arrow::Field`; `FieldType` → `DataType`; `Row` transposes to columnar at the boundary |
| **GlueSQL** | `FieldSchema` → column; `FieldType` → SQL type; a schemaless table is a schema of `Object` |
| **Qdrant** | role `Vector` → a named vector; everything else → payload |
| **tinysearch** | only the `Text` fields are fed to the per-document filter |
| **Liquers type system** | `RecordSchema::type_identifier` carries the `TypeInfo` identity of the described value; `FieldType` is deliberately *not* that registry — one is about fields, the other about values |

### Arrow-shaped, not Arrow-dependent

Adopting `arrow-rs` in `liquers-core` is rejected: it is a large crate family, `CLAUDE.md` requires
core to stay minimal, and the wasm baseline must stay cheap. But `Row` + `RecordSchema` **is** Arrow's
model without the columnar buffers, so the conversion is mechanical and belongs where Arrow already
lives: `liquers-lib`, behind the existing `polars` feature, which pulls Arrow transitively. Core pays
nothing; a polars-enabled build gets `RecordBatch ↔ arrow::RecordBatch` for free.

Rows are **row-major deliberately** — the consumer is a predicate walking one row at a time. The
transpose to columnar happens once, at the Arrow boundary, at batch granularity.

### SourceInfo — identity, description and **retrieval**

```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SourceInfo {
    /// The asset these rows were projected from, as a query.
    #[serde(with = "query_format")]
    pub asset: Query,
    /// The query that re-produces this source's rows. **The retrieval path**: evaluating it
    /// yields the batch again, and the `Id` field indexes into it.
    #[serde(with = "query_format")]
    pub chunk: Query,
    /// Description of the asset itself, when it has one. `None` for a source that is not an asset.
    pub info: Option<AssetInfo>,
    /// How to turn an `Id` value into a directly evaluable query, when the projection can.
    pub locator: Option<LocatorRule>,
}

/// A command applied to `asset`, with the id supplied as its final parameter.
/// Rendered through `ActionRequest`, never by string templating.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LocatorRule {
    pub namespace: String,
    pub command: String,
    pub leading_parameters: Vec<String>,
}
```

A hit must be **retrievable**, not merely identified. `chunk` is the guaranteed path — re-evaluate and
index by the `Id` field — and `locator` is the direct one when a projection can offer it
(`-R/f.csv/-/ns-csv/row-42`). `info` is optional because **a CSV row has no `AssetInfo`; the file
does**, and one per source rather than per row also keeps 656 bytes from repeating.

### RecordSet — one value, and searches compose

```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct RecordSet {
    /// Batches may differ in schema; each carries its own.
    pub batches: Vec<RecordBatch>,
    pub truncated: bool,
    /// Present when this set is a search result; parallel to the rows, batch by batch.
    pub matches: Option<Vec<Vec<ClauseMatch>>>,
    pub diagnostics: Diagnostics,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ClauseMatch {
    /// Which clause of the predicate matched. Answers "why" against the predicate itself.
    pub clause_index: usize,
    /// The field that satisfied it, by schema index.
    pub field: Option<usize>,
    pub excerpt: Option<String>,
    /// Reserved. `None` until a scoring clause exists — Phase 1's ordering promise.
    pub score: Option<f32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct Diagnostics {
    pub scanned: usize,
    /// Fields a clause named that no schema declared — the commonest cause of
    /// "why is X not in my results?", reported rather than silently false.
    pub unavailable_fields: Vec<String>,
}
```

A search result **is** a record set, so a search composes with another search and with any command
that consumes rows.

### SearchPredicate — a pure filter

```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct SearchPredicate {
    /// Implicit AND. Empty matches every record.
    pub clauses: Vec<Clause>,
    /// Hard cap on retained records. `None` is a deliberate, explicit choice by a caller that
    /// knows the stream is bounded.
    pub limit: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Clause {
    Text { needle: String, case_sensitive: bool },
    Field { name: String, test: FieldTest },
    Not(Box<Clause>),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum FieldTest {
    Equals(FieldValue),
    Contains(String),
    Prefix(String),
    Glob(String),
    Range { min: Option<FieldValue>, max: Option<FieldValue> },
    OneOf(Vec<FieldValue>),
    Exists,
}
```

**No `root`, no `sources`, no `depth`.** The stream decides what is in scope; the predicate decides
what survives. The `Key` clause of revision 1 becomes `Field { name: "key.path", test: Glob(..) }` —
a key is a field like any other.

**A `Text` clause has no privileged target.** It matches against **every field whose role is `Text`**
in the batch's schema, which is the review's point: a title, a body and a comment are all text and
none is special. `Field { name }` resolves against the schema **once per batch**, yielding an index;
a name no schema declares lands in `unavailable_fields` and the clause does not match.

Neither enum is `#[non_exhaustive]`: a consumer that silently ignores a clause it does not understand
is a correctness bug, so an exhaustive match making it a compile error is the signal `CLAUDE.md`
exists to preserve. No match uses a default arm.

### Field resolution: qualified names, ambiguity is a parse error

`status` is the asset lifecycle on `MetadataRecord` and a document's lifecycle in front-matter. So a
record's field names are **qualified at projection time**: `meta.` (`meta.status`,
`meta.type_identifier`, `meta.file_size`, `meta.updated`), `attr.` (application attributes, when they
exist), `key.` (`key.path`, `key.name`, `key.extension`). `Clause::Field` matches a qualified name
**exactly**, so a predicate from HTTP or MCP is unambiguous by construction. Unqualified names are a
front-end convenience the syntax parser expands, **failing with an error naming every candidate**
when more than one source could supply it.

## The record stream: batches and chunks

This is what `select` was standing in for, and the abstraction the review identified as missing.

### The stream is `futures::Stream`, not a bespoke trait

`futures = "0.3.34"` is **already a direct dependency of `liquers-core`** (`Cargo.toml:77`, used in
`assets.rs`), so the standard trait costs nothing to adopt and a hand-rolled `next_batch` would be a
worse version of it. The whole combinator vocabulary comes with it — and it is exactly the vocabulary
this design needs: applying a predicate is `filter_map`, the limit is `take`, and the concurrency
deferred in revision 1 is later `buffer_unordered` rather than a rewrite.

`liquers-core/src/maybe_send.rs` already solves the wasm half, and its own documentation states the
trap: a trait-object bound cannot use the `MaybeSend` marker (E0225), so the **whole boxed type is
aliased per target**, and `FutureExt::boxed()` is "always `Send`-boxed and thus wrong on wasm".
`StreamExt::boxed()` has the identical defect, so the sibling alias and boxing helper are added
beside the existing ones:

```rust
// liquers-core/src/maybe_send.rs — mirroring BoxFuture / MaybeBoxed exactly
#[cfg(not(target_arch = "wasm32"))]
pub type BoxStream<'a, T> = Pin<Box<dyn Stream<Item = T> + Send + 'a>>;
#[cfg(target_arch = "wasm32")]
pub type BoxStream<'a, T> = Pin<Box<dyn Stream<Item = T> + 'a>>;

/// Box a stream with the target-correct Send-ness. Replaces `StreamExt::boxed()`,
/// which is always `Send`-boxed and therefore wrong on wasm.
pub trait MaybeBoxedStream<'a>: Stream + Sized + 'a {
    fn maybe_boxed(self) -> BoxStream<'a, Self::Item>;
}
```

```rust
// liquers-core/src/records.rs

/// A batch of records sharing one source dictionary. The unit of memory.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RecordBatch {
    pub sources: Vec<SourceInfo>,
    pub records: Vec<Record>,
}

/// An in-process stream of batches. A **type alias, not a trait** — there is nothing to add to
/// `Stream` that a combinator does not already give. Not a `Value`: neither cloneable nor
/// cacheable, which is why it stops at the query boundary (`record-model.md` §4).
pub type RecordBatchStream<'a> = BoxStream<'a, Result<RecordBatch, Error>>;

/// A source that knows its own partition. The unit of refresh.
#[async_trait]
pub trait ChunkedRecordSource: MaybeSend + MaybeSync {
    /// Chunk descriptors — id, refresh query, version — **without producing any records**.
    async fn partition(&self) -> Result<Vec<ChunkDescriptor>, Error>;
    /// Open one chunk. The returned stream borrows `self`.
    async fn open_chunk(&self, id: &ChunkId) -> Result<RecordBatchStream<'_>, Error>;
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChunkDescriptor {
    pub id: ChunkId,
    #[serde(with = "query_format")]
    pub query: Query,
    pub version: Option<Version>,
    pub source: SourceInfo,
    /// Optional and advisory; field roles are its valuable content.
    pub schema: Option<RecordSchema>,
}
```

**Three consequences of using the standard trait**, each a simplification:

1. **`RecordStream` stops being a trait.** One fewer concept, and no object-safety question to
   reason about: `Stream` is object-safe and `BoxStream` is the established boxed form.
2. **`schema()` moves off the stream** onto `ChunkDescriptor`, which is where it belongs — a schema
   describes a *source*, not an iteration, and a consumer needs it *before* opening the stream in
   order to configure an engine (`record-model.md` §5).
3. **`filter_batch` becomes a combinator.** `SearchPredicate` keeps a per-batch method for testing,
   but the pipeline is `stream.map(|b| predicate.filter_batch(b)).take(limit)` rather than a
   hand-written loop.

`ChunkedRecordSource` uses the project's `MaybeSend`/`MaybeSync` supertrait markers rather than bare
`Send`/`Sync`, as `maybe_send.rs` requires for supertrait bounds, and `#[async_trait]` at each site
needs the usual `cfg_attr(…, async_trait(?Send))` pair.

**Defined now, minimally implemented now.** The roadmap's M0 said Level 0 only; this is a deliberate
extension, on the roadmap's own test: a stream interface is F4 generalized ("a bounded opaque result,
not a bare `Vec`"), expensive to retrofit and cheap to define. M0–M3 ship one implementation —
`futures::stream::once` over a `RecordSet`'s single batch — so nothing streams yet and nothing has to
be redesigned when something does.

### Value extension

```rust
// liquers-core/src/value.rs
pub enum Value {
    // … existing …
    Records(Arc<RecordSet>),
}
```

One variant, `Arc`-wrapped, so `Value` does not grow past 704 bytes. `RecordSet` is the only
record-shaped thing that crosses a query boundary; `RecordBatch`, `Row` and the stream stay internal.
Type identifier `Records`
(bare CamelCase — `liquers-core` owns the concept), default extension `json`, media type
`application/json`. A `TypeInfo` entry in `Value::type_descriptions()` (`value.rs:407`) is
**required** — `CLAUDE.md`'s "four steps, not three; a type with no `TypeInfo` cannot be stored" —
and `type_descriptions_match_identifier` checks it. Every existing `match` on `Value` gains an
explicit arm; a comparable variant (`Value::Recipe`) is named at 17 sites, 8 of them in `value.rs`.

## Clause syntax: one action per clause

The question was whether clauses need a syntax, and whether Liquers entities (`~entity`) should
carry it. **The query language already has a syntax for composition, and clauses should use it.**

```
-R-key/specs/issues/-/ns-search/records/text-expiration/not_text-expired/field-meta.status-draft
```

Checked with `liquers-validate`: it plans cleanly and `meta.status` decodes as written — a bare `.`
is not a separator, so a qualified field name needs no escaping.

One action per clause. Each parameter is escaped independently by `ActionRequest::encode`, so no
operator character can collide with `-`, `~` or `/`; every clause is separately validatable by
`liquers-validate`; and the chain is inspectable in the plan, which is what lets an external engine
translate it (execution path (b)).

**The entity alternative, and why not now.** A single parameter carrying `"phrase" -excluded
kind:issue` needs operator characters that the query language reserves, and entities are the
mechanism for that — `~n<name>~` decodes named entities today. Defining search-specific entities
would work, but it extends the query language for an ergonomic gain that composition already
provides, and the entity table decodes to *characters*, so the operators would still need a parser
downstream. **Deferred, and named as the right mechanism if a one-parameter syntax is ever wanted.**

The human-facing syntax (`use-cases.md` U1: one box, no syntax required) is then a **front end that
compiles to a clause chain**, living in `liquers-lib` and never in the query itself.

## Execution: two paths from one query

Because the source is a query, a search has exactly two executions, and the design supports both
without choosing:

| Path | How | When |
|---|---|---|
| **(a) Evaluate and filter** | Evaluate the source query, consume its `RecordStream`, apply the predicate batch by batch, retain up to `limit` | The MVP, and the only path for a corpus with no engine |
| **(b) Push the query to an engine** | Hand the source query and the clause chain to an external engine that already processed it | `interoperability-layer.md`; the engine is fed by the same query, so what it holds and what (a) would compute are the same records |

Path (b) needs no new interface here: the engine reads the plan.

## Trait Implementations

### `AssetManager::get_asset_info` — repaired, and the only core-trait change

```rust
/// Describe an asset. **Never schedules evaluation.**
///
/// Previously a live key was routed through `get`, which submits to the job queue for an asset
/// that is neither finished nor fast-trackable — so describing an asset in `Status::Recipe` ran
/// it. It now reports from the handle `lookup_key_asset` already returns.
async fn get_asset_info(&self, key: &Key) -> Result<AssetInfo, Error>;
```

Confirmed at `assets.rs:3967-3970` (trait default) and `:5334-5336` (`DefaultAssetManager`).
`listdir_asset_info` (`:4043`) calls it per entry and inherits the repair, so listing a directory
stops starting the assets in it.

### No `select`, and no conformance rules

Revision 1 added `AsyncStore::select` and `AssetManager::select` with a default scan and six
`select*` conformance rules. **Both are dropped.** With records produced by commands, a trait method
is a push-down optimization: valuable when a backend can filter without materializing, absent
everywhere today, and addable later without changing a single consumer. Dropping it removes all
changes to two widely-implemented traits, removes the rule family, and makes
`STORE-TEST-IDS-COLLIDE-WITH-CONFORMANCE-RULE-IDS` irrelevant to this design.

The cost is stated plainly: `STORE-NO-CONTENT-OR-METADATA-SEARCH` asks for selection *on the store*
and is **not** closed by this work. Phase 5 updates it to record that the capability exists above the
store and that push-down remains open.

## Function Signatures

### `liquers-core/src/records.rs`

```rust
impl RecordSchema {
    /// Fails unless exactly one field has role `Id`. The guarantee Phase 1's F1 asked for,
    /// checked once per schema rather than per row.
    pub fn new(fields: Vec<FieldSchema>) -> Result<Self, Error>;
    pub fn id_field(&self) -> usize;
    pub fn source_field(&self) -> Option<usize>;
    pub fn text_fields(&self) -> &[usize];
    pub fn index_of(&self, name: &str) -> Option<usize>;
}

/// A predicate bound to one schema: every field name already resolved to an index.
/// Built once per batch, applied per row.
pub struct BoundPredicate<'a> { /* … */ }

impl SearchPredicate {
    /// Resolve names against a schema. Names no schema declares are returned for
    /// `unavailable_fields` rather than failing.
    pub fn bind(&self, schema: &RecordSchema) -> (BoundPredicate<'_>, Vec<String>);
    /// True when no clause needs a `Text`-role field — the producer may skip projecting bodies.
    pub fn needs_text(&self) -> bool;
}

impl<'a> BoundPredicate<'a> {
    /// Pure, positional. `None` for a clause whose field the schema does not declare.
    pub fn matches(&self, row: &Row) -> Option<(bool, Vec<ClauseMatch>)>;
}

impl SourceInfo {
    /// Build the directly evaluable query for one row, when `locator` allows.
    pub fn locator_query(&self, id: &FieldValue) -> Option<Query>;
}

pub struct RecordBatchBuilder { /* … */ }

pub fn excerpt(text: &str, needle: &str, case_sensitive: bool, radius: usize) -> Option<String>;
```

### `liquers-lib/src/search/mod.rs`

```rust
/// Produce records describing the assets under the key carried by the state.
/// One record per asset (Level 0). Never evaluates: uses the repaired `get_asset_info`.
pub async fn records(state: State<Value>, context: Context<CommandEnvironment>)
    -> Result<Value, Error>;

/// Produce records describing registered commands. Command discovery, no search-specific code.
pub async fn command_records(state: State<Value>, context: Context<CommandEnvironment>)
    -> Result<Value, Error>;

/// Clause commands — each applies one clause to the record set in the state.
pub fn text(state: &State<Value>, needle: String) -> Result<Value, Error>;
pub fn not_text(state: &State<Value>, needle: String) -> Result<Value, Error>;
pub fn field(state: &State<Value>, name: String, value: String) -> Result<Value, Error>;
pub fn limit(state: &State<Value>, n: i64) -> Result<Value, Error>;

/// The one-box convenience: compile a human syntax into clauses and apply them.
pub fn search(state: &State<Value>, query: String) -> Result<Value, Error>;

/// Compile the human syntax. Unrecognised input is a literal term, not an error;
/// an ambiguous unqualified field name IS an error, naming every candidate.
pub fn parse_search_syntax(input: &str) -> Result<Vec<Clause>, Error>;
```

Clause commands are **sync and borrow** — pure transformations of a value in hand. `records` and
`command_records` are **async with owned `State`**, per the macro's rule, with `context` last.

## Integration Points

| Crate | File | Change |
|---|---|---|
| `liquers-core` | `src/records.rs` (new) | `FieldValue`, `RecordSchema`, `Row`, `RecordBatch`, `SourceInfo`, `RecordSet`, predicate and binding, `RecordBatchStream`, `ChunkedRecordSource` |
| `liquers-lib` | `src/records/arrow.rs` (new, `polars` feature) | `RecordBatch ↔ arrow::RecordBatch`, so core pays nothing for Arrow |
| `liquers-core` | `src/maybe_send.rs` | `BoxStream` + `MaybeBoxedStream`, mirroring `BoxFuture`/`MaybeBoxed` |
| `liquers-core` | `src/lib.rs` | `pub mod records;` |
| `liquers-core` | `src/value.rs` | `Value::Records(Arc<RecordSet>)`, every match arm, the `TypeInfo` entry |
| `liquers-core` | `src/assets.rs` | `get_asset_info` repair (two sites) |
| `liquers-lib` | `src/search/mod.rs` (new) | Record producers, clause commands, syntax parser |
| `liquers-lib` | `src/commands.rs` | `register_command!` registrations |
| `specs` | `command_registry.yaml` | Regenerated |

**`liquers-core/src/store.rs` is untouched.** No dependency added: `serde_json` and `async_trait` are
already direct dependencies.

## Documentation Architecture

| Path | Kind | Audience | Change |
|---|---|---|---|
| `specs/reference/RECORDS.md` | reference (new) | contributor, agent | The record, source identity and **retrieval**, the batch/chunk/stream contract, field qualification, the predicate and its match semantics. Revision 2 makes this the primary new reference — records are the capability, search is its first consumer |
| `specs/reference/SEARCH.md` | reference (new) | contributor, agent | Clause semantics, the clause-per-action form, the human syntax, the ordering promise, the invariants, and what a record set says when a field is unavailable |
| `specs/reference/ASSETS.md` | reference | contributor | **`get_asset_info` never schedules** — the behaviour change, stated where the asset lifecycle is owned |
| `specs/reference/VALUE_TYPE_SYSTEM.md` | reference | contributor | The `Records` type identifier and its `TypeInfo` |
| `specs/guides/COMMAND_REGISTRATION_GUIDE.md` | guide | contributor | How a record-producing command is written, and the purity expectation on it |
| `specs/README.md` | map | all | Capability line `designing` → `built` |

**No change to `STORE_SEMANTICS.md`, `CONFORMANCE_TERMS.md` or
`STORE_IMPLEMENTATION_GUIDE.md`** — revision 2 touches no store trait, which is the clearest measure
of how much smaller the core change became.

`affects_docs`: `reference/RECORDS.md`, `reference/SEARCH.md`, `reference/ASSETS.md`,
`reference/VALUE_TYPE_SYSTEM.md`, `guides/COMMAND_REGISTRATION_GUIDE.md`.

## Relevant Commands — namespace `search`

| Command | Signature | Purpose |
|---|---|---|
| `records` | `async fn records(state, context) -> result` | Assets under the state's key become records |
| `command_records` | `async fn command_records(state, context) -> result` | The registry becomes records |
| `text` / `not_text` | `fn …(state, needle: String) -> result` | One text clause |
| `field` | `fn field(state, name: String, value: String) -> result` | One field clause |
| `limit` | `fn limit(state, n: i64 = 50) -> result` | Truncate, setting `truncated` |
| `search` | `fn search(state, query: String) -> result` | The one-box front end |

## Error Handling

All errors are `liquers_core::error::Error` via typed constructors. No `Error::new`, no new error
type, no `unwrap`/`expect`.

| Situation | Outcome |
|---|---|
| State is not a `Value::Records` | `Error::conversion_error` |
| Unreadable entry while producing records | Skipped, counted in `scanned` |
| Unresolvable field | Not an error — the clause does not match; the name lands in `unavailable_fields` |
| Ambiguous unqualified field name | `Error::general_error` naming every candidate — the one place the syntax is intolerant |
| Malformed search syntax | Treated as a literal term |

## Sync vs Async, Serialization, Concurrency

Record production is **async** (store access); clause application, syntax parsing and excerpting are
**sync** (pure). Every data type derives `Serialize, Deserialize`; `Query` uses the existing
`query_format` helper. The stream traits are not serializable and deliberately never cross a query
boundary. No shared mutable state, no lock held across an `.await`; batch production is sequential,
with concurrency deferred until there is something to measure.

## Compilation Validation

- `Stream` is object-safe and already boxed through the project's per-target alias pattern;
  `ChunkedRecordSource` is object-safe (`&self`, no generics, concrete types).
- `BoxStream`/`MaybeBoxedStream` are gated on `target_arch`, **never** on a Cargo feature —
  `maybe_send.rs` documents why: feature unification would silently strip `Send` from the native
  build workspace-wide.
- `Value::Records(Arc<_>)` keeps `size_of::<Value>()` unchanged; the `TypeInfo` entry is present.
- Every `match` on `Value` gains an explicit arm; no default arm anywhere.
- `liquers-core` gains no dependency on `liquers-lib`.
- No feature gate; the build matrix runs anyway because `Value` changed.

## Open Questions for Phase 3

1. Does `FieldValue::Object(Arc<RecordBatch>)` earn its place, or is nesting better expressed as a
   separate batch with a foreign-key field? Nesting is what JSON and Qdrant payloads do; a separate
   batch is what SQL and Arrow prefer.
2. Are `matches` parallel to the rows acceptable, or should a search result be a distinct type that
   *contains* a `RecordSet`? Parallel vectors let a search result compose as a record set; a wrapper
   is safer and costs one unwrap at every consumer.
3. Does `ChunkedRecordSource` earn its place in M0–M3, given nothing implements a non-trivial
   partition until M5? (`RecordBatchStream` is now a type alias and costs nothing, so the question
   narrows to the trait.)
4. Does `FieldType` need `Date`/`Decimal` for GlueSQL and Arrow fidelity, or is `Timestamp` plus
   `Float` enough until the SQL task actually starts?
5. Should `limit` default to `Some(50)` at the command layer while the type allows `None`, or should
   the type forbid `None` as revision 1 had it?
