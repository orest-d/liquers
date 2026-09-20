# Phase 2: Solution & Architecture — Record streams

> **Provisional.** This document carries over the record material developed across revisions 3–6 of
> [`store-and-asset-search`](../store-and-asset-search/phase2-architecture.md), which is where it was
> written and reviewed. It is recorded here at Phase 2 depth so the reasoning is not lost, but
> **this design's Phase 1 is not yet approved**, and Phase 2 is not approved by the carry-over.
> §0 lists what changed in the move, and §0.1 what the Phase 1 answers of 2026-09-19 changed on
> top of it.

## Overview

A record stream is a **lazy sequence of columnar batches**, each laid out exactly as Arrow specifies,
each carrying a schema that names its fields and assigns them roles, and each attributable to a
**chunk** that owns its provenance and validity. `liquers-core` gains a `records` module; `Value`
gains one `Arc`-wrapped variant so a record set is an ordinary Liquers value. Nothing is added to
`AsyncStore` or `AssetManager`.

The five requirements of [Phase 1](./phase1-high-level-design.md) map onto the structures as follows:

| Requirement | Mechanism |
|---|---|
| Arrow interoperability without a heavy dependency | Buffers in Arrow's layout; export via the C Data Interface, IPC bytes or wasm typed arrays — all outside core |
| A `Value` variant | `Value::RecordChunk` and `Value::RecordStream`, each with its `TypeInfo` |
| A lightweight DataFrame without polars | Columns, masks, `select`/`filter`/`slice`/`concat` — usable in `liquers-web` |
| Multi-gigabyte lazy processing | `futures::Stream` of batches; the chunk is the refresh unit, the batch the memory unit |
| Per-chunk provenance and validity, flyweighted to the record | `ChunkDescriptor` carries a `Metadata`; a record's provenance is its chunk's |

## 0. What the split changed

| Change | Why |
|---|---|
| **`ClauseMatch` and search evidence leave** | Evidence for *why a row matched a predicate* is a search concept. Records do not know what a clause is. The search design re-expresses it as columns — see §"Search evidence is columns, not a parallel vector", which **resolves open question 2** of the search design's revision 6 |
| **`SearchPredicate`, `Predicate`, `FieldTest`, `BoundPredicate` leave** | The filter is search's; the *maskable column* is records' |
| **`Bitmap` stays** | It has two uses that are not search at all — Arrow validity buffers and `Column::Bool` storage — and a third, filter masks, that any consumer produces. See §"Bitmap" |
| **The stale row-major `RecordBatch` is deleted** | The search design's stream section still carried revision 3's `RecordBatch { sources, records: Vec<Record> }`, contradicting revision 4's columnar definition eight sections earlier. One definition survives: the columnar one |
| **`Diagnostics` leaves entirely** | Resolved during review: `scanned` and unresolvable field names are *evaluation* facts, and `Metadata`'s `LogEntry` already owns those. See §"No `RecordSet`" |

## 0.1 What the Phase 1 answers changed

| Answer | Change here |
|---|---|
| 2 + 3 — lazy handle; two forms; the prototype's manifest | **The largest change.** One variant becomes **two** — `RecordChunk` and `RecordStream` — with a `Manifest` or `Opaque` backing. Rewind, serialization and cacheability become properties of the backing rather than blanket prohibitions. Review then moved both onto `ExtValue`, and retired `ChunkedRecordSource` as redundant against `Manifest` |
| 3 — provenance | A manifest chunk's provenance is the `Metadata` of its own query, already computed by the asset layer; only an opaque chunk carries one inline. The 704-byte concern now applies solely to the form that is never cached |
| 4 — `Date` | `FieldType::Date` and `Column::Date` added, as Arrow `Date32`. `Decimal` still deferred |
| 5 — uniformity | `RecordStream::uniform_schema: Option<…>` — declared by the producer, checked by the consumer. Non-uniform streams are legal and lose exactly two operations |
| 6 — DataFrame baseline | Closes open question; no change |
| 7 — identity vs addressability | `RecordBatch::chunk_id`, stored once per batch. Identity is (chunk id, record id); the `LocatorRule` stays optional |
| 1 — `openbin` out of scope | No change — `open_chunk` already returns a *stream of batches*, so a row-group reader is a later implementation rather than a redesign |

**Open questions 2, 3 and 4 of the original list are closed by these answers**; the remainder are
restated at the end.

## Known-Issue Preflight

Searched `specs/index.csv` for non-terminal `issue`/`feature` records whose `area` intersects
`core/value`, `core/commands`, `core/context`, `core/query`, `lib/value`, `web`.

| Issue | Status | Pri | Relevance and solution impact | First? | Blocking? | Required action | Priority action |
|---|---|---|---|---|---|---|---|
| `NO-RECORD-STREAM-ABSTRACTION` | draft | P2 | This design *is* its resolution | n/a | no | Close in Phase 5 | keep P2 |
| `CORE-VALUE-ENUM-OVERSIZED` | draft | P2 | `Value` is 704 bytes. Drove fields to `FieldValue` and the new variant behind `Arc` | no | no | Honoured throughout | keep P2 |
| `VALUE-SERIALIZATION-HAS-NO-INCREMENTAL-WRITER` | draft | P2 | A multi-gigabyte record stream cannot be serialized through a `Vec<u8>`-returning writer. **The clearest motivating case yet filed for it** | no | no | Monitor; streaming serialization is a later milestone, and this issue is its prerequisite | consider P1 when that milestone starts |
| `CORE-METADATA-NO-APPLICATION-ATTRIBUTES` | draft | P2 | Front-matter fields have nowhere to live, so `attr.`-qualified columns have no source | no | no | Field qualification is designed to accept it as a pure upgrade | keep P2 |
| `COMMAND-CONTEXT-PARAM-ORDER` | accepted | P2 | `context` must be last in record-producing commands | no | no | Honoured | keep P2 |
| `CORE-SYNC-STORE-TRAIT-OBSOLETE` | — | — | Record sources read through `AsyncStore` only | no | no | — | — |
| `COMMAND-CACHE-FLAG-IS-DECLARED-BUT-NEVER-READ` | draft | P3 | Stream commands rely on `volatile`, which **is** wired; `cache` is a knob that does nothing. Found while answering Phase 1 | no | no | Monitor — `volatile` covers this design's need | keep P3 |

**No blocker.**

## Data Structures

New module `liquers-core/src/records/`.

### Columns, not rows: the batch is Arrow-laid-out

A predicate — or any filter — over a **column** produces a boolean mask, and filters combine by
ANDing masks. That is how polars and DuckDB filter, and it is faster than a row walk. Decisively,
the cheap routes to Arrow (the C Data Interface, and typed arrays over wasm memory) are **only**
possible if the data is already laid out Arrow's way.

```rust
/// A batch of rows in Arrow's memory layout. The unit of memory, a table, and a minimal DataFrame.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RecordBatch {
    /// Field names, types and roles — once per batch.
    pub schema: Arc<RecordSchema>,
    /// One column per schema field, in order. `len` rows each.
    pub columns: Vec<Column>,
    pub len: usize,
    /// Identity of the chunk these rows came from — once per batch, not per row.
    /// With the `Id`-role column this makes a record identifiable as (chunk, id).
    pub chunk_id: Option<ChunkId>,
    /// Dictionary of sources; the `Source`-role column indexes it.
    pub sources: Vec<SourceInfo>,
}

/// Column storage. Buffers are laid out exactly as Arrow specifies — 64-byte aligned, validity
/// omitted when there are no nulls — so an export is a pointer hand-off rather than a conversion.
/// `Buffer<T>` is an `Arc`-shared, 64-byte-aligned `[T]`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Column {
    Bool { validity: Option<Bitmap>, values: Bitmap },
    Int { validity: Option<Bitmap>, values: Buffer<i64> },
    UInt { validity: Option<Bitmap>, values: Buffer<u64> },
    Float { validity: Option<Bitmap>, values: Buffer<f64> },
    /// Arrow `Utf8`: i32 offsets plus a contiguous byte buffer. No per-cell allocation.
    Text { validity: Option<Bitmap>, offsets: Buffer<i32>, data: AlignedBuffer },
    Binary { validity: Option<Bitmap>, offsets: Buffer<i32>, data: AlignedBuffer },
    /// Arrow `Date32` — days since the epoch. `chrono` converts; storage stays primitive.
    Date { validity: Option<Bitmap>, values: Buffer<i32> },
    /// Arrow `Timestamp(Microsecond, None)` — the date-time case.
    Timestamp { validity: Option<Bitmap>, values: Buffer<i64> },
    /// Arrow `FixedSizeList(Float32, dim)` — the embedding case, contiguous.
    Vector { validity: Option<Bitmap>, dim: usize, data: Buffer<f32> },
}
```

**Compactness is not incidental.** An `i64` column costs 8 bytes per value against `FieldValue`'s 24;
a text column costs its bytes plus 4 per row, against a 16-byte `Arc<str>` per cell plus the
allocation; nulls cost one bit rather than a whole slot. For a corpus of a few hundred thousand rows
that is the difference between comfortable and not, in the browser especially — and for the
multi-gigabyte case it sets how many rows fit in one resident batch.

**`FieldValue` survives as the scalar type, not as storage** — it is what a filter compares against,
what a single-cell read returns, and what a builder appends. Storage is columns.

### Being Arrow-compatible cheaply

There are three ways to hand a batch to pandas, polars or DuckDB, and the important fact is that
**two of them require the data to already be in Arrow's layout**:

| Mechanism | Cost | Where it lives |
|---|---|---|
| **Arrow C Data Interface** — two small C structs plus a release callback, designed precisely for exporting columnar data *without* depending on `arrow-rs` | Zero-copy pointer hand-off; a few hundred lines, and `unsafe` | `liquers-py`, beside the existing pyo3 surface |
| **Arrow IPC / Feather bytes** | No `unsafe`, but a flatbuffers encoder; a real chunk of work | `liquers-lib`, deferred — it is also the natural `.arrow` serialization for a record batch |
| **Typed arrays over wasm memory** | Zero-copy; a `Float32Array`/`BigInt64Array` view onto the buffer | `liquers-web` — the browser equivalent of the same trick |

None of the three is possible from `Vec<Row>` without a full transpose and re-encode. Laying the
buffers out Arrow's way makes all three a hand-off. **This is the cheap way the brief asks for, and
it is cheap only because of the layout decision.**

**The safety split matters and is deliberate.** `liquers-core` contains essentially no `unsafe` today
— one occurrence in the whole crate, in a test fixture (`store_conformance/fixture.rs:181`) — and
that should stay true. So core owns the **layout**, which is nothing but `Vec`s and bitmaps and is
entirely safe; the **export** owns the FFI and lives in `liquers-py`, which already has the pyo3
machinery and the right place for a release callback.

**No claim of full Arrow support.** A deliberate subset: the types above, no nested `Struct`, no
`Union`, no dictionary encoding, no large (64-bit offset) variants. Enough for a record batch and for
a pandas hand-off; extendable, and honest about not being arrow-rs.

### Bitmap

```rust
/// Bit-packed booleans, LSB-first within each byte, as Arrow specifies.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Bitmap { bits: AlignedBuffer, len: usize }
```

Three uses, all load-bearing, and **only the second has anything to do with search**:

1. **Validity (nulls).** Not an edge case: a column drawn from heterogeneous sources is absent for
   most of them, and for text an empty string is a *different* answer from "absent", so there is no
   free sentinel.
2. **Filter masks.** Any filter — a search predicate, a DataFrame `filter`, a SQL `WHERE` pushed
   down from GlueSQL — evaluates to a boolean mask over a column. Same representation, different
   meaning.
3. **Boolean columns.** Arrow stores `Bool` as a bitmap, one bit per value — so `Column::Bool`
   carries two of them, validity and values.

**Could it just be `Vec<bool>`?** On space, the bitmap is 8× smaller — 125 KB against 1 MB per
nullable column at a million rows, which matters most in the browser. But the decisive reason is
compatibility: **Arrow's validity buffer *is* a bitmap**, so a `Vec<bool>` cannot be handed over at
all. It would need converting, which destroys the zero-copy property that is the entire point of the
columnar layout.

**One simplification, which Arrow sanctions:** validity is `Option<Bitmap>` and is **omitted
entirely when a column has no nulls** (Arrow's `null_count == 0`, no buffer). Most columns in
practice have none and pay nothing.

Size: roughly 150–200 lines with tests — `get`, a builder, `and`/`or`/`not`, `count_ones`. The one
fiddly part is slicing at a non-byte boundary, which Arrow permits via a bit offset; the first
version **requires byte-aligned slice offsets** and copies when a caller asks for anything else,
which removes the fiddly case at a cost paid only by an unusual slice.

### 64-byte alignment: worth doing, and cheaper than it looks

Arrow requires 8-byte buffer alignment and *recommends* 64 for SIMD. This is a performance and
compatibility matter rather than a correctness one — an unaligned buffer still works, but a strict
consumer may copy, and SIMD paths may be disabled.

Natural Rust gives 8-byte alignment for `Vec<i64>` and 1-byte for `Vec<u8>`, so the text and binary
buffers are the ones that fall short. Four ways to get 64:

| Approach | Cost |
|---|---|
| Custom allocation via `std::alloc` with a 64-byte `Layout` | `unsafe` in core, and manual deallocation |
| Over-allocate and offset | Safe but wasteful, and the offset has to travel with the buffer |
| **`#[repr(align(64))]` backing chunks, cast to `&[u8]`** | **~60 lines, and `bytemuck` makes the cast safe** |
| Copy once at the export boundary | No core change; degrades "zero-copy" to "one copy per hand-off" |

**Recommended: the third.** `bytemuck` is tiny, `no_std`-capable and wasm-safe, and `cast_slice`
turns the aligned backing store into `&[u8]` **with no `unsafe` in our code** — which keeps
`liquers-core`'s library code unsafe-free.

**It is a genuinely new dependency, contrary to an earlier claim in this document.** Checked during
review: `bytemuck` appears in `Cargo.lock` at 1.25.2, but it is **not a direct dependency of any
workspace crate** — it arrives transitively through `egui` (`half → emath → epaint → bytemuck`), so a
`--no-default-features` build does not have it in the graph at all. Adding it to `liquers-core` is
therefore a real new direct dependency, not a free one. It is still the right call — one small,
well-audited, `no_std` crate confined to `records/buffer.rs` against the alternative of `unsafe` in
core — but the cost is one dependency, not zero.

```rust
/// A byte buffer whose start is 64-byte aligned, as Arrow recommends.
/// Backed by `#[repr(align(64))]` chunks; `bytemuck::cast_slice` reads it as bytes.
#[derive(Debug, Clone, PartialEq)]
pub struct AlignedBuffer { /* … */ }

/// A typed, `Arc`-shared, 64-byte-aligned slice. A `bytemuck::Pod` view over an `AlignedBuffer`.
#[derive(Debug, Clone, PartialEq)]
pub struct Buffer<T: bytemuck::Pod> { /* … */ }
```

**Day one, not later.** Retrofitting alignment means reallocating every buffer in the system, and the
fallback (a copy at the export boundary) stays available if the dependency is ever unwelcome.

### The minimal DataFrame role

The brief asks the record mechanism to double as a simplistic DataFrame where polars is too expensive
to bundle — which is the wasm build, where polars is not an option at all. Columns give that almost
for free:

| Operation | Implementation |
|---|---|
| select columns | clone `Arc`s into a new batch; no data copied |
| filter | evaluate a predicate to a boolean mask, then gather |
| slice | offset + length on each buffer, `Arc`-shared |
| concat | schema equality check, then buffer append |
| column stats | a pass per column |

So `liquers-web` gets a usable tabular value with **no new dependency**, and a polars-enabled native
build converts to a real `DataFrame` over the shared buffers when it wants one.

### FieldValue — the scalar type

```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum FieldValue {
    Null, Bool(bool), Int(i64), UInt(u64), Float(f64),
    Text(Arc<str>), Bytes(Arc<[u8]>), Timestamp(i64), Vector(Arc<[f32]>),
}
```

**Measured 24 bytes**, against `serde_json::Value`'s 32 — smaller *and* able to carry bytes without
base64, a timestamp as a type, and a vector compactly. **Not a type parameter**: that would infect
`RecordBatch<V>`, the stream and the value variants — circular, since `Value` is the
obvious `V`. Arrow, GlueSQL, Tantivy and Qdrant all chose a dynamic type enum over generics for the
same reason.

`FieldValue::Object` is dropped: with columns, nesting is Arrow's `Struct`, which the subset above
deliberately excludes for now.

### RecordSchema — modelled on the systems to be integrated

Two orthogonal axes, because no single target has both: a **logical type** (Arrow, polars, GlueSQL)
and a **role** (Tantivy, Lucene, Qdrant).

```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RecordSchema {
    pub fields: Vec<FieldSchema>,
    /// Liquers' own type identity for the thing the rows describe, when there is one.
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
pub enum FieldType { Bool, Int, UInt, Float, Text, Binary, Date, Timestamp, Vector }

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FieldRole {
    /// The row's identity within its source. Exactly one per schema.
    Id,
    /// Index into `RecordBatch::sources`. At most one per schema.
    Source,
    /// Tokenized and matched by a text query. **Any number** — a title, a body and a comment
    /// are all text, and none is privileged.
    Text,
    /// Exact match and facet; never tokenized.
    Keyword,
    /// Returned, never searched.
    Stored,
    /// Range comparisons.
    Numeric,
    /// Similarity comparisons.
    Vector,
    /// Carried and ignored.
    Ignored,
}
```

`FieldType` maps one-to-one onto the `Column` variants, and both map onto Arrow's `DataType`.

**The identity guarantee lives in the schema and is checked once.** Phase 1's requirement that a
record be identifiable cannot live in a row type once storage is columnar, so `RecordSchema::new`
**fails** unless exactly one field has role `Id`, and accessors (`id_field()`, `source_field()`,
`text_fields()`) keep consumers from indexing by string. That is how Tantivy does it: the schema is
validated when built and hands out field handles.

| Target | Mapping |
|---|---|
| **Tantivy / Lucene** | `FieldRole` *is* the field options — `Text`→TEXT, `Keyword`→STRING, `Stored`→STORED, `Numeric`→fast field |
| **Arrow / polars / pandas** | `FieldSchema`→`Field`, `FieldType`→`DataType`, `Column`→the same buffers; export is a hand-off |
| **GlueSQL** | `FieldSchema`→column, `FieldType`→SQL type |
| **Qdrant** | role `Vector`→a named vector; everything else→payload |
| **tinysearch** | only `Text`-role columns feed the per-document filter |
| **Liquers type system** | `RecordSchema::type_identifier` carries the `TypeInfo` identity of the described value; `FieldType` is about *fields*, the registry about *values* |

### Schema uniformity is declared, not assumed

**Phase 1 answer 5.** Chunks of one stream are usually but not necessarily alike, so
`RecordStream::uniform_schema` is `Some` when the producer promises it and `None` otherwise. A
consumer checks rather than assumes. The prototype takes the same position in the weakest possible
way — on a column-count mismatch it **warns and continues** — and this design keeps the tolerance
while making the promise inspectable.

Non-uniformity is legal with consequences rather than an error:

| Operation | Non-uniform stream |
|---|---|
| iterate, filter per chunk | fine |
| serialize as NDJSON | fine — each row carries its own keys |
| `concat` into one batch | **fails**, naming the first differing field |
| serialize as one CSV | **fails** — there is no single header |

The motivating case is "every CSV file in a folder becomes one chunk", which is a `Manifest` with one
query per file and no guarantee the files agree. That is useful rather than illegal: it works for
everything except the two operations that genuinely need one schema.

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

**Identity is (chunk id, record id); addressability is separate and optional.** Phase 1 answer 7
separated two things the original question conflated. *Identity* is always available and cheap: a
`ChunkId`, stored **once per batch** rather than per row, plus the `Id`-role field. *Addressability* —
constructing a query that returns exactly one record — is the optional half.

A row must be **retrievable**, not merely identified. `chunk` is the guaranteed path — re-evaluate
and index by the `Id` field — and `locator` is the direct one when a projection can offer it
(`-R/f.csv/-/ns-csv/row-42`). `info` is optional because **a CSV row has no `AssetInfo`; the file
does**, and one per source rather than per row also keeps 656 bytes from repeating.

### No `RecordSet`: the materialized form is a batch

**Removed during the Phase 2 review.** The draft carried a `RecordSet { batches: Vec<RecordBatch>,
truncated, diagnostics }` described as "the bounded result that a command returns and a `Value`
carries" — while the value variants carry a single `RecordBatch` and a `RecordStream`. Both cannot be
true, and `RecordSet` is the one that loses: a bounded sequence of batches is either a
`RecordStream` that happens to be finite, or a single batch after `concat`. A third name for it earns
nothing.

`truncated` moves onto `RecordStream` as a producer's report that it stopped early.

**`Diagnostics` is removed too, and its content goes where Liquers already puts evaluation facts.**
"How many records were scanned" and "you named a field no schema declares" are facts about an
*evaluation*, not about *data*; putting them in the value forces every consumer of a batch to carry a
field it does not want. `Metadata` already owns per-evaluation reporting through `LogEntry`, with
`Info`, `Debug`, `Warning` and `Error` kinds (`metadata.rs:495-552`). So a producer logs `scanned`
as `Info` and each unresolvable field as `Warning`.

The cost, stated: a log entry is a string, so `unavailable_fields` stops being a structured list. That
is accepted because `LogEntry` is already this project's answer for this class of information
everywhere else, and the consumer is a person or an agent reading why nothing matched. **The search
design must be updated to match** — it referenced `Diagnostics::unavailable_fields`.

### Search evidence is columns, not a parallel vector

Revision 6 of the search design left this open: should a search result be a `RecordSet` with a
parallel `matches: Vec<Vec<ClauseMatch>>`, or a distinct type wrapping one? **The split answers it:
neither.** A record set that carries a field only searches fill is a record set that knows what a
clause is, and the whole point of extracting this design is that it does not.

Search evidence is expressed the way every other per-row fact is — **as columns**, added to the
result schema with role `Stored`:

| Column | Type | Meaning |
|---|---|---|
| `match.clauses` | `UInt` | Bitmask; bit *i* set when clause *i* of the predicate admitted this row |
| `match.excerpt` | `Text`, nullable | The best excerpt, when a text clause produced one |
| `match.score` | `Float`, nullable | `Null` until a scoring clause exists |

This is strictly better than either option that was on the table: a search result composes with any
record consumer with no unwrapping, the evidence serializes as CSV or NDJSON like everything else,
and `RecordSet` loses a field. **The cost, stated:** the bitmask caps a predicate at **64 clauses**,
which is far past anything a person or an MCP tool writes, and the cap is a documented error rather
than a silent truncation. Multiple excerpts per row are not representable — one is kept — which is
the same trade every search UI makes.

### Field resolution: qualified names, ambiguity is an error

`status` is the asset lifecycle on `MetadataRecord` and a document's lifecycle in front-matter. So a
record's field names are **qualified at projection time**: `meta.` (`meta.status`,
`meta.type_identifier`, `meta.file_size`, `meta.updated`), `attr.` (application attributes, when they
exist), `key.` (`key.path`, `key.name`, `key.extension`). A consumer matches a qualified name
**exactly**, so a field reference arriving from HTTP or MCP is unambiguous by construction.
Unqualified names are a front-end convenience that a consumer's parser may expand, **failing with an
error naming every candidate** when more than one source could supply it.

This convention is owned here because it is a property of *projection* — of how an asset becomes
rows — not of any one consumer.

## The record stream: batches and chunks

Three scales, and they are not the same thing:

| Scale | Unit of | Bounded by |
|---|---|---|
| **Record** (row) | retrieval and identity | — |
| **Batch** | **memory** — what is resident at once | a row count or a byte budget |
| **Chunk** | **refresh** — what expires and is re-produced together | the source's own partitioning |

A multi-gigabyte table is one source, partitioned into chunks; each chunk is opened as a stream of
batches, and **one batch at a time is resident**. A chunk is therefore *not* materialized, which is
the point: a single large parquet file is one chunk, and holding it in memory is exactly what this
design exists to avoid.

### The stream is `futures::Stream`, not a bespoke trait

`futures = "0.3.34"` is **already a direct dependency of `liquers-core`** (`Cargo.toml:77`, used in
`assets.rs`), so the standard trait costs nothing to adopt and a hand-rolled `next_batch` would be a
worse version of it. The whole combinator vocabulary comes with it: applying a filter is `filter_map`,
a limit is `take`, and concurrency is later `buffer_unordered` rather than a rewrite.

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
// liquers-core/src/records/mod.rs

/// An in-process stream of batches. A **type alias, not a trait** — there is nothing to add to
/// `Stream` that a combinator does not already give. Not a `Value`: neither cloneable nor
/// cacheable, which is why it stops at the query boundary (`record-model.md` §4).
pub type RecordBatchStream<'a> = BoxStream<'a, Result<RecordBatch, Error>>;

```

**`ChunkedRecordSource` is not introduced.** The draft had a trait with `partition()` (chunk
descriptors, no records) and `open_chunk()`. Phase 1 answer 3 makes it redundant before it is
written: **`StreamBacking::Manifest(Vec<Query>)` *is* a partition**, expressed as data rather than as
a method — and data is strictly better here, because a manifest can be stored, cached, diffed and
inspected, none of which a trait object can. The reconciliation contract in particular becomes a
set-diff over two query lists rather than a comparison of `ChunkDescriptor`s.

What the trait would still buy is a source whose partition is **only discoverable incrementally** — a
paginated remote API that reveals the next page token only after reading a page. That is real, it is
not in scope, and the trait can be added then without disturbing anything, because `Manifest` and
such a source are two implementations of "where do chunks come from" rather than competing designs.

**Three consequences of using the standard trait**, each a simplification:

1. **`RecordStream` is not a trait.** One fewer concept, and no object-safety question: `Stream` is
   object-safe and `BoxStream` is the established boxed form.
2. **`schema()` moves off the stream** onto `ChunkDescriptor`, which is where it belongs — a schema
   describes a *source*, not an iteration, and a consumer needs it *before* opening the stream in
   order to configure an engine (`record-model.md` §5).
3. **Filtering is a combinator**, not a hand-written loop.

`ChunkedRecordSource` uses the project's `MaybeSend`/`MaybeSync` supertrait markers rather than bare
`Send`/`Sync`, as `maybe_send.rs` requires, and `#[async_trait]` at each site needs the usual
`cfg_attr(…, async_trait(?Send))` pair.

### Provenance and validity: the chunk carries a `Metadata`

The brief asks records to trace provenance and validity per chunk, flyweighted to the record. The
mechanism exists already and is not reinvented:

```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChunkDescriptor {
    pub id: ChunkId,
    /// The query that re-produces this chunk.
    #[serde(with = "query_format")]
    pub query: Query,
    /// Provenance and validity. `Metadata` already carries `query`, `version`,
    /// `dependencies: Vec<DependencyRecord { key, version }>`, `status` and `updated`.
    pub metadata: Metadata,
    pub source: SourceInfo,
    /// Optional and advisory; field roles are its valuable content.
    pub schema: Option<RecordSchema>,
}
```

So **provenance is "the query and the dependency versions this chunk was produced from"**, and
**validity is the staleness check the dependency manager already performs**. A record's provenance is
its chunk's — that is the flyweight, and it costs one `Source`-role column index per row rather than
a `Metadata` per record.

**Reconciliation, not push.** A consumer holding a copy of a chunk compares `(ChunkId, Version)`
pairs against a fresh `partition()` and re-opens what differs. Correctness comes from the set
difference; any notification mechanism is a latency optimization on top, never the thing correctness
depends on. This is what the search design's
[`interoperability-layer.md`](../store-and-asset-search/interoperability-layer.md) §3 builds on, and
why `partition()` must not produce records.

### Value extension — `ExtValue` in `liquers-lib`, not `Value` in core

**Revised by Phase 1 answer 3, then corrected during the Phase 2 review.** Answer 3 replaced the
single `Value::Records` variant with two forms, on the Python prototype's evidence that a stream
whose chunk sequence is **known in advance** is perfectly cloneable, cacheable and serializable,
because what travels is a list of queries rather than any data.

That part stands. Where the draft was wrong was the crate: **it put both variants on
`liquers_core::value::Value`, where they cannot compile.**

`Value` derives `Serialize, Deserialize, Debug, Clone, PartialEq` and is `#[serde(untagged)]`
(`value.rs:20-21`). Every variant must satisfy all five, and the opaque backing satisfies none of the
hard ones:

| Requirement | `StreamBacking::Opaque(Mutex<Option<BoxStream<…>>>)` |
|---|---|
| `Clone` | `Mutex<T>` is not `Clone` |
| `PartialEq` | `BoxStream` is not comparable |
| `Serialize` | no meaningful bytes |
| **`Deserialize`** | **impossible in principle** — a generator cannot be reconstructed from bytes, at any amount of effort |

`Arc`-wrapping does not rescue it; the derive demands the bounds transitively. And `untagged` makes a
half-measure worse, since deserialization tries each variant in turn.

**The right home was already prescribed.** `CLAUDE.md` says new value types are `ExtValue` variants
in `liquers-lib/src/value/mod.rs`, and `ExtValue` derives **only `Debug, Clone`** (`mod.rs:23`). It
already holds exactly these shapes — `Arc<dyn UIElement>`, and `Arc<Mutex<dyn WidgetValue>>` behind
the `egui` feature.

```rust
// liquers-lib/src/value/mod.rs
pub enum ExtValue {
    // … existing …
    /// A materialized table. Shareable, cacheable, serializable.
    RecordChunk { value: Arc<RecordBatch> },
    /// A lazy sequence of chunks. Shareable only when manifest-backed.
    RecordStream { value: Arc<RecordStream> },
}
```

Three problems dissolve at once: `Clone` clones the `Arc` regardless of contents, `PartialEq` is not
required, and serialization stops being a derive.

```rust
// liquers-core/src/records/mod.rs

pub struct RecordStream {
    pub backing: StreamBacking,
    /// Declared, not assumed — see §"Schema uniformity is declared".
    pub uniform_schema: Option<Arc<RecordSchema>>,
}

pub enum StreamBacking {
    /// One query per chunk. The preferred form: rewindable, serializable as the query list,
    /// cacheable, and checkpointable while it is still being built.
    Manifest(Vec<Query>),
    /// A generator. One-shot. The command producing it must be registered `volatile`.
    Opaque(Mutex<Option<BoxStream<'static, Result<RecordBatch, Error>>>>),
}
```

| Form | Shareable | Cacheable | Serializable | Rewindable |
|---|---|---|---|---|
| `RecordChunk` | yes | yes | yes | n/a |
| `RecordStream(Manifest)` | yes | yes | as the query list | yes |
| `RecordStream(Opaque)` | **no** | **no** | **no** | **no** |

**How `Opaque` behaves as an `ExtValue` without lying.** `Clone` clones the `Arc`, so two holders
share **one** stream; whichever consumes it first gets the batches and the other gets an error rather
than silently empty or duplicated data. This is the "unique limitation of a lazy stream" made
explicit: the hazard is real, it is reported rather than hidden, and the `volatile` marker keeps such
a value out of the cache and off the asset registry in the first place — which `assets.rs` already
enforces, propagating volatility to every downstream step.

**Serialization becomes a fallible method, which is exactly the semantics needed.**
`DefaultValueSerializer::as_bytes(&self, data_format: &str) -> Result<Vec<u8>, Error>`
(`value.rs:919-926`) is per-format and may refuse — and `ExtValue::UIElement` already refuses this
way with `ErrorType::SerializationError` (`mod.rs:309-316`):

| Value | `as_bytes` |
|---|---|
| `RecordChunk` | `json`, `ndjson`, `csv` |
| `RecordStream(Manifest)` | `json` — the query list |
| `RecordStream(Opaque)` | an error naming the manifest alternative |

The refusal is built with a **typed constructor** —
`Error::from_error(ErrorType::SerializationError, …)`, exactly as the neighbouring `ExtValue::UIElement`
arm does — never `Error::new`, which `CLAUDE.md` forbids. Worth stating explicitly because
`value.rs:956` and `:968` reach for `Error::new` for serialization errors today; this module does not
copy that.

So "a manifest serializes and an opaque stream does not" needs **no new mechanism**; it is one match
arm in a method that already exists. The arms are enumerated rather than caught by `_ =>`, matching
the comment already in that match about adding a variant being a compile error.

**wasm is unaffected.** `liquers-web` depends on `liquers-lib` with
`default-features = false, features = ["webui"]` (`liquers-web/Cargo.toml:15`), so `ExtValue` is
reachable from the browser without polars — which is the requirement that drove the DataFrame role in
the first place. Neither variant is feature-gated: their only dependency is `bytemuck`, so unlike
`PolarsDataFrame` they need no `#[cfg]`, and no `match` on `ExtValue` needs a gated arm.

**What the prototype adds that this design did not have.** `_store_batches` rewrites its manifest
after *every* batch, so a run that dies partway leaves its finished chunks usable. A generator cannot
be checkpointed; a growing `Manifest` can. The equivalent here is a `store_record_stream` command
that appends a query to the manifest and rewrites it per chunk — cheap, and the difference between a
failed six-hour job being worthless and being resumable.

**What it does not carry over.** The prototype's manifest holds store *keys*, so it can `contains`,
list and clean its own directory before rewriting. Queries are more general — they cover a computed
chunk, which keys cannot — and the cost is that **cleanup is no longer automatic**. A manifest whose
queries point at stored chunks knows nothing about removing them. That is accepted, and the
`store_record_stream` command owns its own directory hygiene instead.

**Registration** is the unchanged four-step procedure: extend `ExtValue`; choose the identifiers
(`RecordChunk` and `RecordStream`, bare CamelCase — Liquers owns both concepts); implement the
conversions in `ExtValueInterface` and `DefaultValueSerializer`; and add both `TypeInfo` entries to
`ExtValue::type_descriptions()` (`mod.rs:148`) — `CLAUDE.md`'s "four steps, not three; a type with no
`TypeInfo` cannot be stored".

**What stays in `liquers-core`.** The *data* types — `RecordBatch`, `Column`, `Bitmap`, `Buffer`,
`RecordSchema`, `FieldValue`, `SourceInfo` and `RecordStream` itself — remain in
`liquers-core/src/records/`. They are plain, depend only on `bytemuck`, and embed core types
(`Query`, `Metadata`, `Version`); the search design's predicate, also core, operates on them. Only the
*value wrapping* moves up. Stated as a judgment rather than a certainty — the alternative is moving
the data types to `liquers-lib` as well and taking the search predicate with them, which keeps core
smaller at the cost of putting a foundational abstraction above it. Carried to Phase 3 as an open
question rather than settled by assertion.

**Serializing the multi-gigabyte case** still meets `VALUE-SERIALIZATION-HAS-NO-INCREMENTAL-WRITER`:
`as_bytes` returns a `Vec<u8>`, so NDJSON or CSV over a large stream cannot stream through it. A
`Manifest` sidesteps the problem — the manifest itself is small — which is one more reason to prefer
that form.

## Trait Implementations

| Trait | For | Note |
|---|---|---|
| `MaybeBoxedStream` | blanket over `Stream` | Mirrors the existing `MaybeBoxed` |
| `ExtValueInterface` conversions | `ExtValue::RecordChunk`, `ExtValue::RecordStream` | `from_*`/`as_*` arms, per `TYPE_SYSTEM_GUIDE.md` |
| `DefaultValueSerializer` | `ExtValue::RecordChunk`, `ExtValue::RecordStream` | chunk: json / ndjson / csv. Stream: json for a manifest, `SerializationError` for an opaque one |

**No change to `AsyncStore` or `AssetManager`.** Record production is a *command* concern; a trait
method would be a push-down optimization, addable later without changing a consumer.

## Generic Parameters & Bounds

None on the data types — `FieldValue` is a dynamic enum precisely so `RecordBatch`, the stream and
the value variants stay concrete. The only bound in the module is `T: bytemuck::Pod` on `Buffer<T>`,
which is what makes the aligned cast safe.

## Function Signatures

### `liquers-core/src/records/mod.rs`

```rust
impl RecordSchema {
    /// Fails unless exactly one field has role `Id`. Checked once per schema rather than per row.
    pub fn new(fields: Vec<FieldSchema>) -> Result<Self, Error>;
    pub fn id_field(&self) -> usize;
    pub fn source_field(&self) -> Option<usize>;
    pub fn text_fields(&self) -> &[usize];
    pub fn index_of(&self, name: &str) -> Option<usize>;
}

impl RecordBatch {
    /// Zero-copy column projection.
    pub fn select(&self, columns: &[usize]) -> Result<RecordBatch, Error>;
    /// Gather by mask — the filter primitive every consumer builds on.
    pub fn filter(&self, mask: &Bitmap) -> Result<RecordBatch, Error>;
    /// Offset + length on every buffer; `Arc`-shared, no copy.
    pub fn slice(&self, offset: usize, len: usize) -> Result<RecordBatch, Error>;
    pub fn concat(batches: &[RecordBatch]) -> Result<RecordBatch, Error>;
    /// Single-cell read, for a consumer that wants one value rather than a column.
    pub fn value(&self, row: usize, column: usize) -> Result<FieldValue, Error>;
    /// Append columns to the schema and the batch — how a consumer adds derived fields.
    pub fn with_columns(&self, fields: Vec<FieldSchema>, columns: Vec<Column>)
        -> Result<RecordBatch, Error>;
}

impl SourceInfo {
    /// Build the directly evaluable query for one row, when `locator` allows.
    pub fn locator_query(&self, id: &FieldValue) -> Option<Query>;
}

pub struct RecordBatchBuilder { /* … */ }

impl Bitmap {
    pub fn get(&self, i: usize) -> bool;
    pub fn and(&self, other: &Bitmap) -> Result<Bitmap, Error>;
    pub fn or(&self, other: &Bitmap) -> Result<Bitmap, Error>;
    pub fn not(&self) -> Bitmap;
    pub fn count_ones(&self) -> usize;
}
```

### `liquers-core/src/records/buffer.rs`

```rust
impl AlignedBuffer {
    pub fn from_slice(bytes: &[u8]) -> Self;
    pub fn as_bytes(&self) -> &[u8];
    /// 64-byte aligned, as Arrow recommends. Asserted in a test.
    pub fn alignment() -> usize;
}

impl<T: bytemuck::Pod> Buffer<T> {
    pub fn from_slice(values: &[T]) -> Self;
    pub fn as_slice(&self) -> &[T];
    pub fn as_bytes(&self) -> &[u8];
}
```

## Integration Points

| Crate | File | Change |
|---|---|---|
| `liquers-core` | `src/records/buffer.rs` (new) | `AlignedBuffer`, `Buffer<T>`, `Bitmap` — the Arrow-layout primitives; the only place `bytemuck` is used |
| `liquers-core` | `src/records/mod.rs` (new) | `FieldValue`, `RecordSchema`, `Column`, `RecordBatch`, `SourceInfo`, `RecordStream`, `StreamBacking`, `RecordBatchStream`, `ChunkDescriptor` |
| `liquers-core` | `src/maybe_send.rs` | `BoxStream` + `MaybeBoxedStream`, mirroring `BoxFuture`/`MaybeBoxed` |
| `liquers-core` | `src/lib.rs` | `pub mod records;` |
| `liquers-lib` | `src/value/mod.rs` | `ExtValue::RecordChunk` and `ExtValue::RecordStream`, every match arm, both `TypeInfo` entries, the `DefaultValueSerializer` arms |
| `liquers-lib` | `src/records/mod.rs` (new) | Record-producing commands and source adapters |
| `liquers-lib` | `src/records/polars.rs` (new, `polars` feature) | `RecordBatch → polars::DataFrame` over the shared buffers |
| `liquers-py` | later milestone | Arrow C Data Interface export — the only place `unsafe` FFI belongs |
| `liquers-web` | later milestone | Typed-array views over the same buffers |
| `specs` | `command_registry.yaml` | Regenerated |

**Dependencies:** `bytemuck` only — **a new direct dependency of `liquers-core`**, confined to
`records/buffer.rs`. It is already in `Cargo.lock` at 1.25.2, but only transitively via `egui`, so
this genuinely adds an edge to core's dependency graph. `futures` (0.3.34, `Cargo.toml:77`), `chrono`
(`Cargo.toml:73`), `serde` and `async_trait` **are** already direct dependencies and cost nothing.

## Documentation Architecture

### Reference Plan

`specs/reference/RECORD_STREAMS.md` — **new**. Audience contributor and agent; area `core/value`.
The record, batch, chunk and stream contract; identity as (source, id) and the two retrieval paths;
the schema, its types and its roles; the Arrow layout and exactly which subset is supported; field
qualification; provenance and validity per chunk.

### Guide Plan

`specs/guides/RECORD_STREAM_GUIDE.md` — **new**. Audience contributor; workflow "produce records from
a new source". Writing a record-producing command; choosing a batch size; when to implement
writing a manifest-backed stream; using a batch as a DataFrame; handing a batch to pandas or polars.

### Existing Documents to Review or Update

| Path | Change |
|---|---|
| `specs/reference/VALUE_TYPE_SYSTEM.md` | The `Records` type identifier and its `TypeInfo` |
| `specs/guides/TYPE_SYSTEM_GUIDE.md` | `Records` in the worked list of variants |
| `specs/guides/COMMAND_REGISTRATION_GUIDE.md` | How a record-producing command is written |
| `specs/README.md` | A capability-map entry for record streams |

`affects_docs`: `reference/RECORD_STREAMS.md`, `guides/RECORD_STREAM_GUIDE.md`,
`reference/VALUE_TYPE_SYSTEM.md`, `guides/TYPE_SYSTEM_GUIDE.md`,
`guides/COMMAND_REGISTRATION_GUIDE.md`.

## Relevant Commands

Deliberately thin: this design owns the *mechanism*, and each consumer brings its own producers.

| Command | Signature | Purpose |
|---|---|---|
| `records_to_csv` | `fn records_to_csv(state) -> result` | Serialize a record set as CSV |
| `records_to_ndjson` | `fn records_to_ndjson(state) -> result` | Serialize a record set as NDJSON |
| `records_schema` | `fn records_schema(state) -> result` | The schema as a value — how an agent discovers field names |
| `records_head` | `fn records_head(state, n: i64 = 20) -> result` | Slice, for inspection |

Namespace `ns-records`. Producers (`ns-search/records`, a CSV projection, a parquet projection) are
owned by the designs that need them.

## Error Handling

All errors are `liquers_core::error::Error` via typed constructors. No `Error::new`, no new error
type, no `unwrap`/`expect`.

| Situation | Outcome |
|---|---|
| A schema without exactly one `Id` field | `Error::general_error` from `RecordSchema::new` |
| Column count or length disagrees with the schema or `len` | `Error::general_error` |
| `concat` of batches with different schemas | `Error::general_error` naming the first differing field |
| A mask whose length differs from the batch | `Error::general_error` |
| State is not an `ExtValue::RecordChunk` or `ExtValue::RecordStream` | `Error::conversion_error` |
| A field name no schema declares | Not an error — a `Warning` log entry on the evaluation's `Metadata` |
| Unreadable entry while producing records | Skipped, counted in an `Info` log entry |

## Sync vs Async Decisions

| Operation | Choice | Rationale |
|---|---|---|
| `RecordBatch` select/filter/slice/concat/value | sync | Pure, in-memory |
| `Bitmap` operations | sync | Pure |
| Schema construction and validation | sync | Pure |
| Opening a chunk from a manifest query | async | Evaluation and store access |
| Record-producing commands | async | Store access |
| Serialization to CSV / NDJSON | sync today | Becomes streaming once an incremental writer exists |

## Serialization Strategy

Every data type derives `Serialize, Deserialize`; `Query` uses the existing `query_format` helper.
`AlignedBuffer` and `Buffer<T>` serialize as their bytes — alignment is a memory property, not a
wire one, and is re-established on deserialization. The stream types are **not** serializable and
deliberately never cross a query boundary; the serializable forms are a `RecordBatch` and a
manifest-backed `RecordStream`.

## Concurrency Considerations

No shared mutable state. Buffers are `Arc`-shared and immutable once built, so `select`, `slice` and
a column hand-off copy nothing and are safe to share. Batch production is sequential; concurrency is
`buffer_unordered` over the chunk stream when there is something to measure. No lock is held across
an `.await`.

## Compilation Validation

- `Stream` is object-safe and already boxed through the project's per-target alias pattern;
  no new trait is introduced, so there is no object-safety question to answer.
- `BoxStream`/`MaybeBoxedStream` are gated on `target_arch`, **never** on a Cargo feature —
  `maybe_send.rs` documents why: feature unification would silently strip `Send` from the native
  build workspace-wide.
- Both variants are `Arc`-wrapped, so `size_of::<ExtValue>()` is unchanged and `Value` is untouched.
- `ExtValue` derives only `Debug, Clone`, so the opaque stream needs no `Serialize`, `Deserialize` or
  `PartialEq` — the reason these variants cannot live on `Value`.
- Every `match` on `Value` gains an explicit arm; no default arm anywhere.
- `liquers-core` gains no dependency on `liquers-lib`, **and no `unsafe`**.
- `Buffer<T>: bytemuck::Pod` holds for `i32`, `i64`, `u64`, `f32`, `f64`.
- No feature gate; the build matrix runs anyway because `Value` changed.

## References to liquers-patterns.md

Async-by-default with `#[async_trait]`; `MaybeSend`/`MaybeSync` for wasm; typed error constructors;
explicit match arms with no default; `Arc` for shared payloads in `Value`; `TypeInfo` registration
as the fourth step of adding a value type; `context` last in a command signature.

## Open Questions for Phase 3

1. **Do the record *data* types belong in `liquers-core` or `liquers-lib`?** The value variants are
   settled (`ExtValue`, in lib). The data types are currently in core because they are plain, depend
   only on `bytemuck`, embed core types and are what the search predicate operates on. The
   alternative keeps core smaller and moves the predicate up with them. **The sharpest remaining
   question.**
2. What is the default batch size, and is it a row count or a byte budget? A byte budget is the
   honest answer for the multi-gigabyte case but needs a size estimate per column.
3. Is `with_columns` the right extension point for derived fields? The search design's evidence
   columns are its only user, and they apply to a `RecordChunk`.
4. `Manifest(Vec<Query>)` cannot clean up the chunks it points at, unlike the prototype's key list.
   Does `store_record_stream` need a companion that removes a manifest's stored chunks, or is that
   the caller's business?
5. Dropping `Diagnostics` costs `unavailable_fields` its structure — it becomes a `Warning` log
   entry. Is that enough for a UI that wants to offer "did you mean…", or does that case want
   structure back?
6. Does the 64-clause cap implied by a `UInt` evidence bitmask belong here or in the search design?

**Closed by the Phase 1 answers or by this review:** `FieldType` date types (answer 4 — `Date` now,
`Decimal` later); whether `ChunkedRecordSource` earns its place (**no** — `Manifest` is a partition
as data); whether `RecordSet` survives (**no** — a batch or a stream, not a third name); which crate
owns the value variants (**`liquers-lib`**, because `ExtValue` derives only `Debug, Clone`).
