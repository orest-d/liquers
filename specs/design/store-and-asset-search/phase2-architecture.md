# Phase 2: Solution & Architecture — Store and asset search

> **Revision 6.** Revision 1 put the source inside the predicate, added `select` to two core traits
> and gave every hit an `AssetInfo`; revision 2 corrected those; revision 3 replaced the `Record`
> struct with rows plus a schema; revision 4 makes the batch **columnar in Arrow's layout**; revision 5 replaced the filter pipeline
> with a predicate **expression**; and revision 6 expresses that expression as **syntax parsed by the
> command**, so no `Value` variant is added for it. §0 records every change.

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
| **Rev. 3: field names live in the schema, once per batch** | A map per row re-creates every field name for every record. The schema stores them once, and a field name resolves to an index once per batch instead of per row. |
| **Rev. 5: the predicate is an expression, not a filter pipeline** | Revision 2's clause chain mixed building a stream with progressively reducing it. A pipeline cannot express `OR` or grouping, never produces the predicate as a whole — so an external engine has a sequence of steps to reverse-engineer — and is eager, which forecloses filter-then-verify and push-down. |
| **Rev. 6: the expression is syntax, parsed by the command — no `Value::Predicate`** | Revision 5 reached for link parameters, which would require adding a predicate variant to the core value enum for a capability nothing yet needs. A syntax string in one parameter answers all three objections *better*: grouping lives in the grammar, the predicate travels as text an engine can parse with the same parser, and it is parsed before the stream is touched. The conditions under which the variant would earn its place are recorded rather than guessed. |
| **Rev. 4: the batch is columnar, in Arrow's layout** | Reverses revision 3's row-major storage. "The consumer is a predicate walking one row at a time" does not survive scrutiny — a predicate over a column yields a boolean mask and clauses AND their masks, which is how polars and DuckDB filter. Decisively, the cheap routes to Arrow (the C Data Interface, and typed arrays over wasm memory) are **only** possible if the data is already laid out Arrow's way. It also makes the batch a usable minimal DataFrame where polars cannot be bundled. |
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

### Columns, not rows: the batch is Arrow-laid-out

**Revision 3 removed the `Record` struct; revision 4 reverses its row-major storage.** The
justification for `Vec<Row>` was that "the consumer is a predicate walking one row at a time", and
that does not survive scrutiny: a predicate over a **column** produces a boolean mask, and clauses
combine by ANDing masks — which is how polars and DuckDB actually filter, and is faster than a row
walk. Row-major also forecloses the cheap Arrow path entirely (below), which is the decisive
argument.

```rust
/// A batch of rows in Arrow's memory layout. The unit of memory, a table, and a minimal DataFrame.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RecordBatch {
    /// Field names, types and roles — once per batch.
    pub schema: Arc<RecordSchema>,
    /// One column per schema field, in order. `len` rows each.
    pub columns: Vec<Column>,
    pub len: usize,
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
    /// Arrow `Timestamp(Microsecond, None)`.
    Timestamp { validity: Option<Bitmap>, values: Buffer<i64> },
    /// Arrow `FixedSizeList(Float32, dim)` — the embedding case, contiguous.
    Vector { validity: Option<Bitmap>, dim: usize, data: Buffer<f32> },
}
```

**Compactness is not incidental.** An `i64` column costs 8 bytes per value against `FieldValue`'s 24;
a text column costs its bytes plus 4 per row, against a 16-byte `Arc<str>` per cell plus the
allocation; nulls cost one bit rather than a whole slot. For a corpus of a few hundred thousand rows
that is the difference between comfortable and not, in the browser especially.

**`FieldValue` survives as the scalar type, not as storage** — it is what a predicate compares
against, what a single-cell read returns, and what a builder appends. Storage is columns.

### Being Arrow-compatible cheaply

There are three ways to hand a batch to pandas, polars or DuckDB, and the important fact is that
**two of them require the data to already be in Arrow's layout**:

| Mechanism | Cost | Where it lives |
|---|---|---|
| **Arrow C Data Interface** — two small C structs plus a release callback, designed precisely for exporting columnar data *without* depending on `arrow-rs` | Zero-copy pointer hand-off; a few hundred lines, and `unsafe` | `liquers-py`, beside the existing pyo3 surface |
| **Arrow IPC / Feather bytes** | No `unsafe`, but a flatbuffers encoder; a real chunk of work | `liquers-lib`, deferred — it is also the natural `.arrow` serialization for a record batch |
| **Typed arrays over wasm memory** | Zero-copy; a `Float32Array`/`BigInt64Array` view onto the buffer | `liquers-web` — the browser equivalent of the same trick |

Neither the first nor the third is possible from `Vec<Row>` without a full transpose and re-encode.
Laying the buffers out Arrow's way makes all three a hand-off. **This is the cheap way, and it is
cheap only because of the layout decision.**

**The safety split matters and is deliberate.** `liquers-core` contains essentially no `unsafe` today
— one occurrence in the whole crate — and that should stay true. So core owns the **layout**, which
is nothing but `Vec`s and bitmaps and is entirely safe; the **export** owns the FFI and lives in
`liquers-py`, which already has the pyo3 machinery and the right place for a release callback.

**No claim of full Arrow support.** A deliberate subset: the types above, no nested `Struct`, no
`Union`, no dictionary encoding, no large (64-bit offset) variants. Enough for a record batch and for
a pandas hand-off; extendable, and honest about not being arrow-rs.

### Bitmap — three real uses, not completeness

```rust
/// Bit-packed booleans, LSB-first within each byte, as Arrow specifies.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Bitmap { bits: AlignedBuffer, len: usize }
```

It is used for three different things, all of them load-bearing:

1. **Validity (nulls).** Not an edge case here: `meta.file_size` is null for a directory,
   `meta.description` is absent for most documents, and `attr.status` is absent for anything that is
   not a spec document. A batch drawing on heterogeneous sources has nulls as the norm, and for text
   an empty string is a *different* answer from "absent", so there is no free sentinel.
2. **Filter masks.** The predicate evaluates each clause to a boolean mask and ANDs them. Same
   representation, different meaning.
3. **Boolean columns.** Arrow stores `Bool` as a bitmap, one bit per value — so `Column::Bool`
   carries two of them, validity and values.

**Could it just be `Vec<bool>`?** On space, the bitmap is 8× smaller — 125 KB against 1 MB per
nullable column at a million rows, which matters most in the browser. But the decisive reason is
compatibility: **Arrow's validity buffer *is* a bitmap**, so a `Vec<bool>` cannot be handed over at
all. It would need converting, which destroys the zero-copy property that is the entire point of the
columnar layout. The bitmap is not there for completeness; without it there is no cheap Arrow path.

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

**Recommended: the third.** `bytemuck` **is already in the lockfile** (1.25.2, pulled transitively),
is tiny, `no_std`-capable and wasm-safe, and `cast_slice` turns the aligned backing store into `&[u8]`
**with no `unsafe` in our code**. That matters concretely: `liquers-core`'s library code is currently
unsafe-free — the crate's single `unsafe` is in a *test fixture*
(`store_conformance/fixture.rs:181`) — and this keeps it that way.

```rust
/// A byte buffer whose start is 64-byte aligned, as Arrow recommends.
/// Backed by `#[repr(align(64))]` chunks; `bytemuck::cast_slice` reads it as bytes.
#[derive(Debug, Clone, PartialEq)]
pub struct AlignedBuffer { /* … */ }
```

**Day one, not later** — which is why it was flagged as a question rather than left implicit.
Retrofitting alignment means reallocating every buffer in the system, and the fallback (a copy at the
export boundary) stays available if the dependency is ever unwelcome.

### The minimal DataFrame role

The brief asks the record mechanism to double as a simplistic DataFrame where polars is too expensive
to bundle — which is the wasm build, where polars is not an option at all. Columns give that almost
for free:

| Operation | Implementation |
|---|---|
| select columns | clone `Arc`s into a new batch; no data copied |
| filter | evaluate clauses to a boolean mask, then gather — the same path the predicate already takes |
| slice | offset + length on each buffer, `Arc`-shared |
| concat | schema equality check, then buffer append |
| column stats | a pass per column |

So `liquers-web` gets a usable tabular value with **no new dependency**, and a polars-enabled native
build converts to a real `DataFrame` when it wants one.

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
`RecordBatch<V>`, the stream, the predicate and `Value::Records(Arc<RecordSet<V>>)` — circular, since
`Value` is the obvious `V`. Arrow, GlueSQL, Tantivy and Qdrant all chose a dynamic type enum over
generics for the same reason.

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
pub enum FieldType { Bool, Int, UInt, Float, Text, Binary, Timestamp, Vector }

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FieldRole {
    /// The row's identity within its source. Exactly one per schema.
    Id,
    /// Index into `RecordBatch::sources`. At most one per schema.
    Source,
    /// Tokenized and matched by a text clause. **Any number** — a title, a body and a comment
    /// are all text, and none is privileged.
    Text,
    /// Exact match and facet; never tokenized.
    Keyword,
    /// Returned, never searched.
    Stored,
    /// Range clauses.
    Numeric,
    /// Similarity clauses.
    Vector,
    /// Carried and ignored.
    Ignored,
}
```

`FieldType` maps one-to-one onto the `Column` variants, and both map onto Arrow's `DataType`.

**The guarantee moves from the type to the schema, and is checked once.** Phase 1's F1 required
identity to be structural; with columns it cannot live in a row type, so `RecordSchema::new` **fails**
unless exactly one field has role `Id`, and accessors (`id_field()`, `source_field()`,
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
    /// Present when this set is a search result; parallel to the surviving rows, batch by batch.
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

The predicate is a **boolean expression tree** and a **value in its own right** — see
§"The predicate is a value, produced by an expression" for the type and the three ways to write one.
Its leaves are:

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
what survives, and it is applied to the stream as a **link parameter** rather than being spread
across a chain of filtering commands. The `Key` clause of revision 1 becomes `Field { name: "key.path", test: Glob(..) }` —
a key is a field like any other.

**A `Text` clause has no privileged target.** It matches against **every column whose role is
`Text`**, which is the review's point: a title, a body and a comment are all text and none is
special. `Field { name }` resolves against the schema **once per batch**, yielding a column index; a
name no schema declares lands in `unavailable_fields` and the clause does not match.

**Evaluation is mask-based.** Each clause is evaluated against its column to produce a boolean
`Bitmap` of length `len`; clauses combine by ANDing masks; the surviving rows are gathered once. This
is the polars/DuckDB shape, it is faster than a row walk, and it makes `ClauseMatch` cheap to attribute
because the mask says exactly which clause admitted which row.

Neither `Predicate` nor `FieldTest` is `#[non_exhaustive]`: a consumer that silently ignores a node
it does not understand is a correctness bug, so an exhaustive match making it a compile error is the
signal `CLAUDE.md` exists to preserve. No match uses a default arm.

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

**One variant.** `Value::Predicate` was proposed in revision 5 and is **not** taken: the predicate
travels as syntax text in a parameter, which carries the same information without a permanent
addition to the core value enum. §"When the `Value` variant would earn its place" records the
conditions under which to revisit it.

One variant, `Arc`-wrapped, so `Value` does not grow past 704 bytes. `RecordSet` is the only
record-shaped thing that crosses a query boundary; `RecordBatch`, `Row` and the stream stay internal.
Type identifier `Records`
(bare CamelCase — `liquers-core` owns the concept), default extension `json`, media type
`application/json`. A `TypeInfo` entry in `Value::type_descriptions()` (`value.rs:407`) is
**required** — `CLAUDE.md`'s "four steps, not three; a type with no `TypeInfo` cannot be stored" —
and `type_descriptions_match_identifier` checks it. Every existing `match` on `Value` gains an
explicit arm; a comparable variant (`Value::Recipe`) is named at 17 sites, 8 of them in `value.rs`.

## The predicate is a value, produced by an expression

**Revision 5 corrects revision 2's clause syntax.** That draft offered

```
-R-key/specs/issues/-/ns-search/records/text-expiration/not_text-expired/field-meta.status-draft
```

and called it "the query language already has a syntax for clauses". It does not. That chain mixes
two different things — building a record stream, then **progressively reducing it** — and a filter
pipeline is not an expression that evaluates to a predicate. Three consequences, each real:

1. **It cannot express `OR` or grouping.** Sequential filters are AND-only. `a AND (b OR c)` would
   need `union`/`intersect` commands over materialized record sets.
2. **The predicate never exists as a value.** It is only a side effect of the chain — so it cannot be
   handed to an external engine, stored, or reused. That directly undermines execution path (b),
   where what the engine needs *is the predicate*, not a sequence of steps to reverse-engineer.
3. **It is eager.** Each step reduces a set, so nothing can know the whole predicate before touching
   data — which is exactly what filter-then-verify and push-down both require.

### A syntax and a parser, not a Value variant

The obvious fix — make the predicate a value and pass it through a **link parameter**
(`~X~<query>~E`, which does exist and would work) — requires `Value::Predicate`. That is a permanent
addition to the core value enum in exchange for a capability nothing yet needs, so it is **not
taken**. Instead the predicate is an ordinary Rust type, and a **suggested syntax with a parser**
produces it inside the command.

```
-R-key/specs/issues/-/ns-search/records/select-<expression>
```

**All three objections to the pipeline are answered better this way than by links:**

| Objection | Answer |
|---|---|
| Cannot express `OR` or grouping | The *syntax* has `\|` and parentheses. Validated below |
| The predicate is never a value | It **is** one — as **text in a single parameter**. That is a perfectly good serialized form: storable in a recipe, inspectable in the plan, and handed to an external engine as one string it parses with the same parser. Strictly better than reverse-engineering a chain of steps, and it costs no `Value` variant |
| Eager evaluation | The whole expression is parsed **before** the stream is touched, so filter-then-verify and push-down both remain open |

### The predicate type and its expression tree

`SearchPredicate` stays a `liquers-core` type — `Serialize`/`Deserialize`, so an HTTP or MCP caller
that would rather send a structured predicate than a string can — but it is **not** a `Value`
variant.

```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct SearchPredicate {
    pub expr: Predicate,
    pub limit: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub enum Predicate {
    /// Matches everything — the identity of `All`, and what an empty expression parses to.
    #[default]
    Always,
    Never,
    All(Vec<Predicate>),
    Any(Vec<Predicate>),
    Not(Box<Predicate>),
    Text { needle: String, case_sensitive: bool },
    Field { name: String, test: FieldTest },
}
```

Evaluation is bottom-up over columns: each leaf yields a `Bitmap` over the batch, `All` ANDs its
children, `Any` ORs them, `Not` complements. That is how a vectorized engine evaluates a filter tree.
`ClauseMatch::clause_index` is the node's index in a pre-order walk of `expr`, so evidence still names
exactly which part of the expression admitted a row.

### The syntax

Now the *only* way to write a predicate, so it carries grouping rather than deferring it:

```ebnf
expr    = or ;
or      = and , { "|" , and } ;          (* lowest precedence *)
and     = unary , { unary } ;            (* implicit AND by juxtaposition *)
unary   = [ "-" ] , atom ;               (* leading "-" negates *)
atom    = "(" , expr , ")"
        | field , ":" , value
        | '"' , phrase , '"'
        | term ;
```

The vocabulary is the intersection of what Google, GitHub and Lucene users already expect:

```
expiration                          a term
"expiration safety"                 a phrase
-expired                            negation
meta.status:draft                   a field test
expiry | expiration                 disjunction
(expiry | expiration) -expired      grouping
```

Unrecognised input is a **literal term**, not an error — except an ambiguous unqualified field name,
which is reported with every candidate. An empty expression is `Predicate::Always`.

### Ergonomics, honestly

Every operator character must be escaped inside a query parameter. Checked: `(`, `)`, `|`, `:` and
`>` all **fail to parse raw**; only alphanumerics, `.`, `_` and escapes survive. So

```
ns-search/select-~nlpar~expiry~.~nverbar~~.expiration~nrpar~~.~_expired~.meta.status~ncolon~draft
```

decodes to exactly `(expiry | expiration) -expired meta.status:draft` — verified. A hand-written
search URL is therefore essentially unwritable, and that is fine: the user types into a box, and a
UI, an MCP tool or `ActionRequest::encode` builds the query. Note `~_` for the ASCII hyphen, **not**
`~nminus~` (U+2212) — the trap the escaping guide warns about, and one this design fell into once
already.

### When the `Value` variant would earn its place

Recorded so the decision can be revisited on evidence rather than taste. Add `Value::Predicate` when
one of these actually arrives:

- a predicate must be **built by one query and consumed by another** — a saved-search asset composed
  into a larger search;
- a predicate must be **produced by a command** rather than written, for instance derived from a
  user's profile or from another record set;
- **partial predicates need reuse** across several searches without repeating their text.

Until then a string in a parameter carries the same information at no cost to the value system.

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

/// A predicate bound to one schema: every field name already resolved to a column index.
/// Built once per batch, evaluated column-wise.
pub struct BoundPredicate<'a> { /* … */ }

impl SearchPredicate {
    /// Resolve names against a schema. Names no schema declares are returned for
    /// `unavailable_fields` rather than failing.
    pub fn bind(&self, schema: &RecordSchema) -> (BoundPredicate<'_>, Vec<String>);
    /// True when no clause needs a `Text`-role field — the producer may skip projecting bodies.
    pub fn needs_text(&self) -> bool;
}

impl<'a> BoundPredicate<'a> {
    /// One mask per clause, over the whole batch. Pure; no I/O.
    pub fn clause_masks(&self, batch: &RecordBatch) -> Result<Vec<Bitmap>, Error>;
    /// AND the clause masks and gather the surviving rows, with per-row evidence.
    pub fn apply(&self, batch: &RecordBatch) -> Result<(RecordBatch, Vec<Vec<ClauseMatch>>), Error>;
}

impl RecordBatch {
    /// Zero-copy column projection.
    pub fn select(&self, columns: &[usize]) -> Result<RecordBatch, Error>;
    /// Gather by mask — the filter primitive.
    pub fn filter(&self, mask: &Bitmap) -> Result<RecordBatch, Error>;
    /// Offset + length on every buffer; `Arc`-shared, no copy.
    pub fn slice(&self, offset: usize, len: usize) -> Result<RecordBatch, Error>;
    pub fn concat(batches: &[RecordBatch]) -> Result<RecordBatch, Error>;
    /// Single-cell read, for a consumer that wants one value rather than a column.
    pub fn value(&self, row: usize, column: usize) -> Result<FieldValue, Error>;
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

/// Parse the search syntax into a predicate. Unrecognised input is a literal term, not an
/// error; an ambiguous unqualified field name IS an error, naming every candidate.
pub fn parse_search_syntax(input: &str) -> Result<Predicate, Error>;

/// Parse `expr` and apply it to the record stream in the state. The only search command.
pub fn select(state: &State<Value>, expr: String, limit: i64) -> Result<Value, Error>;
```

Clause commands are **sync and borrow** — pure transformations of a value in hand. `records` and
`command_records` are **async with owned `State`**, per the macro's rule, with `context` last.

## Integration Points

| Crate | File | Change |
|---|---|---|
| `liquers-core` | `src/records/buffer.rs` (new) | `AlignedBuffer`, `Buffer<T>`, `Bitmap` — the Arrow-layout primitives; the only place `bytemuck` is used |
| `liquers-core` | `src/records/mod.rs` (new) | `FieldValue`, `RecordSchema`, `Column`, `RecordBatch`, `SourceInfo`, `RecordSet`, predicate and binding, `RecordBatchStream`, `ChunkedRecordSource` |
| `liquers-lib` | `src/records/arrow.rs` (new, `polars` feature) | `RecordBatch → polars::DataFrame` over the shared buffers; Arrow IPC bytes, deferred |
| `liquers-py` | Arrow C Data Interface export (later milestone) | The only place `unsafe` FFI belongs; core stays safe |
| `liquers-core` | `src/maybe_send.rs` | `BoxStream` + `MaybeBoxedStream`, mirroring `BoxFuture`/`MaybeBoxed` |
| `liquers-core` | `src/lib.rs` | `pub mod records;` |
| `liquers-core` | `src/value.rs` | `Value::Records(Arc<RecordSet>)`, every match arm, the `TypeInfo` entry |
| `liquers-core` | `src/assets.rs` | `get_asset_info` repair (two sites) |
| `liquers-lib` | `src/search/mod.rs` (new) | Record producers, clause commands, syntax parser |
| `liquers-lib` | `src/commands.rs` | `register_command!` registrations |
| `specs` | `command_registry.yaml` | Regenerated |

**Dependencies:** `bytemuck` only — already in the lockfile at 1.25.2, tiny, `no_std`-capable,
wasm-safe, and confined to `records/buffer.rs`. Nothing else is added.

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
| `select` | `fn select(state, expr: String = "", limit: i64 = 50) -> result` | Parse the expression and apply it to the record stream in the state. **The only search command** |

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
- `liquers-core` gains no dependency on `liquers-lib`, **and no `unsafe`** — the crate has one
  occurrence today and the columnar layout adds none. The C Data Interface's `unsafe` lives in
  `liquers-py`.
- Buffers are `Arc<[T]>`, so `select`, `slice` and a column hand-off copy nothing.
- No feature gate; the build matrix runs anyway because `Value` changed.

## Open Questions for Phase 3

1. ~~Alignment and `Bitmap`~~ — **both resolved above.** 64-byte alignment is day-one work via
   `#[repr(align(64))]` chunks plus `bytemuck` (already in the lockfile, no `unsafe` in our code);
   `Bitmap` is hand-rolled, ~150–200 lines, with byte-aligned slice offsets only.
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
