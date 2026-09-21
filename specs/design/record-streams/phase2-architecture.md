# Phase 2: Solution & Architecture — Record streams

**Status:** Phase 1 approved 2026-09-20; this document is the proposed architecture, awaiting
approval. A dated changelog is at the end.

## Overview

A record stream is a **lazy sequence of columnar batches**, each laid out as Arrow specifies, each
carrying a schema that names its fields and declares what an index should do with them, and each
attributable to a **chunk** that owns its provenance and validity.

The whole feature lives in **`liquers-lib`, behind a `records` feature**: it is a data type, not a
core capability. `liquers-core` is untouched apart from two generic stream type aliases, and nothing
is added to `AsyncStore` or `AssetManager`. A build with the feature off is byte-for-byte the build
that exists today.

Three abstractions carry it — a **source** that can be asked repeatedly for a stream, a **stream**
that is one traversal, and a **chunk** that is a materialized table. Two of them are values
(`ExtValue::RecordChunk` and `ExtValue::RecordSource`); the stream deliberately is not.

The five requirements of [Phase 1](./phase1-high-level-design.md) map onto the architecture as
follows:

| Requirement | Mechanism |
|---|---|
| Arrow interoperability without a heavy dependency | Buffers in Arrow's layout; export via the C Data Interface, IPC bytes, or typed-array views in the browser — none of it requiring `arrow-rs` |
| A `Value` variant | `ExtValue::RecordChunk` and `ExtValue::RecordSource`, each with its `TypeInfo`, both feature-gated |
| A lightweight DataFrame without polars | Columns, masks, `select`/`filter`/`slice`/`concat` — usable from `liquers-web`, which cannot bundle polars |
| Multi-gigabyte lazy processing | `futures::Stream` of batches; the chunk is the refresh unit, the batch the memory unit, and only one batch is resident |
| Per-chunk provenance and validity, flyweighted to the record | `ChunkDescriptor` carries a `Metadata`; a record's provenance is its chunk's |

## Known-Issue Preflight

Searched `specs/index.csv` for non-terminal `issue`/`feature` records whose `area` intersects
`core/value`, `core/commands`, `core/context`, `core/query`, `lib/value`, `web`.

| Issue | Status | Pri | Relevance and solution impact | First? | Blocking? | Required action | Priority action |
|---|---|---|---|---|---|---|---|
| `NO-RECORD-STREAM-ABSTRACTION` | draft | P2 | This design *is* its resolution | n/a | no | Close in Phase 5 | keep P2 |
| `CORE-VALUE-ENUM-OVERSIZED` | draft | P2 | `Value` is 704 bytes. Drove fields to `FieldValue` and the new variant behind `Arc` | no | no | Honoured throughout | keep P2 |
| `VALUE-SERIALIZATION-HAS-NO-INCREMENTAL-WRITER` | draft | P2 | A multi-gigabyte record stream cannot be serialized through a `Vec<u8>`-returning writer. **The clearest motivating case yet filed for it**, now with a concrete consumer in `liquers-axum` | no | no | **Ad-hoc streaming in `liquers-axum` for this design**; issue updated with the HTTP motivation and the push-vs-pull finding | consider P1 when a second value type needs it |
| `CORE-METADATA-NO-APPLICATION-ATTRIBUTES` | draft | P2 | Front-matter fields have nowhere to live, so `attr.`-qualified columns have no source | no | no | Field qualification is designed to accept it as a pure upgrade | keep P2 |
| `COMMAND-CONTEXT-PARAM-ORDER` | accepted | P2 | `context` must be last in record-producing commands | no | no | Honoured | keep P2 |
| `CORE-SYNC-STORE-TRAIT-OBSOLETE` | — | — | Record sources read through `AsyncStore` only | no | no | — | — |
| `COMMAND-CACHE-FLAG-IS-DECLARED-BUT-NEVER-READ` | draft | P3 | Stream commands rely on `volatile`, which **is** wired; `cache` is a knob that does nothing. Found while answering Phase 1 | no | no | Monitor — `volatile` covers this design's need | keep P3 |

**No blocker.**

## Three abstractions: source, stream, chunk

Three things, not two — the distinction is `Iterable` vs `Iterator`, or in Rust `IntoIterator` vs
`Iterator`:

| | Role | Shareable | Serializable | Consumed by use |
|---|---|---|---|---|
| **`RecordSource`** | can be asked, repeatedly, for a stream of chunks | **yes** | **yes**, as a manifest | no |
| **`RecordStream`** | one traversal, in flight | **no** — may be partly consumed | not itself; its *data* can be drained to a table | **yes** |
| **`RecordChunk`** | a materialized table | **yes** | **yes** — csv, parquet, ndjson, json | no |

Conversions run cheaply in one direction:

```
RecordSource  --stream()-->  RecordStream  --collect()-->  RecordChunk
     ^                                                          |
     +---------------- from_chunk() (in-memory backing) --------+
```

A chunk becomes a stream with `stream::once`, and a source with an in-memory backing — both cheap,
as the brief requires. A stream does **not** become a source: the information is gone.

### Why there is no `rewind`

The obvious alternative — one stream type with a `rewind()` that works for some backings — gives a
fallible method whose success depends on how the value happened to be constructed. That is discovered
at runtime, by a user who did not read the documentation.

With three types there is nothing to rewind. **Hold the source and open another stream; hold only a
stream and you have one pass.** Phase 1 answer 2 asked for "optionally rewindable, cloneable in some
cases", and this delivers it as a property of *which type you hold*, checked by the compiler.

### Names

`RecordSource`, `RecordStream` and `RecordChunk`. `ChunkOrigin` is the per-chunk record of where its
rows came from — the asset query, the chunk query, an optional `AssetInfo` and an optional locator.
`IntoRecordStream` is available as the trait a type implements to be usable as a source.

### The types

```rust
// liquers-lib/src/records/mod.rs

/// Something that can be asked, repeatedly, for a stream of chunks.
/// The `Iterable` of this design: shareable, serializable, never consumed by use.
#[derive(Debug, Clone)]
pub struct RecordSource {
    backing: SourceBacking,
    /// Declared, not assumed — see §"Schema uniformity is declared".
    pub uniform_schema: Option<Arc<RecordSchema>>,
}

/// **Private.** All behaviour goes through `RecordSource`'s methods, so adding a backing later
/// is a change inside this module rather than a sweep across every match on it — which matters
/// because `CLAUDE.md` forbids default match arms.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
enum SourceBacking {
    /// Chunks already in memory. What `RecordChunk::into_source()` produces.
    Materialized(Vec<Arc<RecordBatch>>),
    /// Chunks named by queries. The explicit list and the template **combine**: `chunks` are
    /// the stream's first chunks in order, and `template` produces everything after them,
    /// rendered at the **global** chunk index. Either may be absent.
    /// See `manifest-format.md` §4a.
    Queried {
        /// The explicit prefix. Empty when every chunk is generated.
        #[serde(with = "query_format_seq")]
        chunks: Vec<Query>,
        /// The rule for chunks beyond the prefix. `None` — `chunks` is the whole stream, and
        /// `chunks()` returns `Known`. `Some` — the count is unknown and it returns `Unbounded`.
        /// **Not constructed in this version**; reserved for a SQL table paginated by offset.
        template: Option<ChunkTemplate>,
        /// Chunk naming, which is what makes chunks keyed and addressable.
        /// **Always `None` in this version.**
        keys: Option<ChunkKeys>,
        /// Whether keyed chunks are persisted. Keyed and stored are separate axes —
        /// `manifest-format.md` §4b. Irrelevant when `keys` is `None`.
        store: bool,
    },
}

/// How a stream's chunks are **named**, which is what makes them keyed assets in one folder —
/// the folder holding the manifest, which is also its `cwd`. Naming gives identity and
/// addressability; whether the bytes are persisted is `store`, a separate axis
/// (`manifest-format.md` §3 and §4b).
/// The folder is what makes chunks addressable (`-R/data/mystream/data_0042.csv`), makes the
/// stream listable (`-R-dir/data/mystream`), and makes cleanup possible — a manifest of bare
/// queries cannot remove what it names, a manifest that owns a folder can.
/// **Always `None` in this version.**
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChunkKeys {
    pub folder: Key,
    pub filename_prefix: String,   // "data"
    pub number_format: String,     // "{:04}"
    pub extension: String,         // "csv"
}

/// Enumeration is not always possible, so a consumer handles both cases from the start.
pub enum ChunkList<'a> {
    /// Every chunk is known, so reconciliation can diff a complete set — detecting
    /// additions, changes **and deletions**.
    Known(&'a [ChunkId]),
    /// The count is unknown; a walk ends at the first short chunk. Reconciliation is
    /// append-only, and **deletions cannot be detected** without a full walk.
    Unbounded { computed: &'a [ChunkId] },
}

impl RecordSource {
    /// Open a fresh traversal. Callable any number of times — this is what replaces `rewind`.
    pub async fn stream(&self, context: &Context<impl Environment>)
        -> Result<RecordBatchStream<'_>, Error>;
    /// Chunks without producing any records — the reconciliation primitive.
    pub fn chunks(&self) -> ChunkList<'_>;
}
```

`RecordStream` is **not a struct**. It stays the type alias it already was:

```rust
/// One traversal. Not `Clone`, not `Serialize` — and now it does not need to pretend to be,
/// because it is no longer a value anybody stores.
pub type RecordBatchStream<'a> = BoxStream<'a, Result<RecordBatch, Error>>;
```

**Its data is still serializable, which is the distinction Phase 1 answer 2 drew.** Draining a stream
to csv or parquet is a perfectly good operation — it is the *handle* that cannot be stored or shared,
not the rows. That is what `records_to_csv` does, and why it consumes its input.

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

**No new trait is introduced.** A `ChunkedRecordSource` trait with a `partition()` method would be
redundant against the manifest: `RecordSource::chunks()` is the partition, expressed **as data** —
which can be stored, cached, diffed and inspected, none of which a trait object can.

A source whose partition is **discoverable only incrementally** — a SQL table paginated by offset, a
remote API revealing the next page token after each page — is served by the reserved `template` field
rather than by a trait. That is why `chunks()` returns a `ChunkList` distinguishing `Known` from
`Unbounded` **now**: enumeration is not always possible, and a consumer written against a complete
`Vec` would have to be revisited. See
[`chunking-and-resumability.md`](./chunking-and-resumability.md).

### Three scales, unchanged

| Scale | Unit of | Bounded by |
|---|---|---|
| **Record** (row) | retrieval and identity | — |
| **Batch** | **memory** — what is resident at once | a row count or a byte budget |
| **Chunk** | **refresh** — what expires and is re-produced together | the source's own partitioning |

A multi-gigabyte table is one source, partitioned into chunks; each chunk streams as batches, and
**one batch at a time is resident**. A chunk is therefore not materialized on traversal, which is the
point: a single large parquet file is one chunk, and holding it in memory is what this design exists
to avoid. `RecordChunk` — the *materialized* form — is the deliberate exception, used when a chunk is
small enough to be a value.

### Provenance and validity: the chunk carries a `Metadata`

The brief asks records to trace provenance and validity per chunk, flyweighted to the record. The
mechanism exists already and is not reinvented:

```rust
/// A chunk's identity. The two variants are the two identity regimes of
/// `manifest-format.md` §5: an unkeyed stream identifies a chunk by the query that
/// produces it, a keyed stream by the key its chunk is stored under.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ChunkId {
    /// Unkeyed: the producing query, which is also the chunk's asset identity.
    Query(#[serde(with = "query_format")] Query),
    /// Keyed: the stored chunk's key, e.g. `data/sales/daily_0010.csv`.
    Key(Key),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChunkDescriptor {
    pub id: ChunkId,
    /// The query that re-produces this chunk.
    #[serde(with = "query_format")]
    pub query: Query,
    /// Provenance and validity. `Metadata` already carries `query`, `version`,
    /// `dependencies: Vec<DependencyRecord { key, version }>`, `status` and `updated`.
    pub metadata: Metadata,
    pub origin: ChunkOrigin,
    /// Optional and advisory; field roles are its valuable content.
    pub schema: Option<RecordSchema>,
}
```

**Enumerating chunks and describing one are separate calls, deliberately.** `chunks()` is cheap and
synchronous: it returns what the source already knows, which is ids. A `ChunkDescriptor` carries a
`Metadata`, and for a keyed stream that metadata lives in the chunk asset's own store entry — so
building one is I/O and cannot happen inside a synchronous enumeration:

```rust
impl RecordSource {
    /// Cheap, synchronous, no I/O. The reconciliation planning primitive.
    pub fn chunks(&self) -> ChunkList<'_>;
    /// Full provenance for one chunk. Reads metadata, so it is async.
    pub async fn describe_chunk(&self, id: &ChunkId, context: &Context<impl Environment>)
        -> Result<ChunkDescriptor, Error>;
}
```

This mirrors `get_asset_info`: listing is cheap, describing costs a read. Reconciliation uses both —
`chunks()` to learn what exists, `describe_chunk` to get the `(id, version)` pair for each one it
must compare.

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

### Value extension — `ExtValue`, not core's `Value`

Two forms are values: the materialized chunk and the source. **Both live on `ExtValue` in
`liquers-lib`, and neither could live on `liquers_core::value::Value`.**

`Value` derives `Serialize, Deserialize, Debug, Clone, PartialEq` and is `#[serde(untagged)]`
(`value.rs:20-21`). Every variant must satisfy all five. A stream handle satisfies none of the hard
ones:

| Requirement | a stream handle, however wrapped |
|---|---|
| `Clone` | `Mutex<T>` is not `Clone` |
| `PartialEq` | `BoxStream` is not comparable |
| `Serialize` | no meaningful bytes |
| **`Deserialize`** | **impossible in principle** — a generator cannot be reconstructed from bytes, at any amount of effort |

`Arc`-wrapping does not rescue it; the derive demands the bounds transitively. And `untagged` makes a
half-measure worse, since deserialization tries each variant in turn.

`ExtValue` is the right home and `CLAUDE.md` already prescribes it for new value types. It derives
**only `Debug, Clone`** (`mod.rs:23`) and already holds exactly these shapes — `Arc<dyn UIElement>`,
and `Arc<Mutex<dyn WidgetValue>>` behind the `egui` feature.

```rust
// liquers-lib/src/value/mod.rs
pub enum ExtValue {
    // … existing …
    /// A materialized table. Shareable, cacheable, serializable.
    #[cfg(feature = "records")]
    RecordChunk { value: Arc<RecordBatch> },
    /// Something that can be asked, repeatedly, for a stream. Shareable, serializable
    /// as a manifest, never consumed by use.
    #[cfg(feature = "records")]
    RecordSource { value: Arc<RecordSource> },
}
```

**Only two variants, and neither is hazardous.** The three-way model keeps the troublesome thing out
of the value system entirely: a stream is never an `ExtValue`, because a stream is a traversal in
flight and a value is something you can hold, clone and cache. `Clone` clones the `Arc` regardless of
contents, `PartialEq` is not required, and serialization stops being a derive.

**Where the partly-consumed hazard went.** Phase 1 answer 2 warned that a stream "may be already
partly consumed", that sharing it should be avoided, and that stream commands would therefore be
`volatile`. With sources as values and streams confined to the inside of a command, **the hazard has
nowhere to appear**: what a command receives and returns is a source, which is re-openable by
construction, and the stream it opens lives and dies inside that call.

`volatile` is therefore *not* the normal case for a record command. It is needed only when a result
genuinely should not be reused, which is a separate question from streaming.

**Serialization becomes a fallible method, which is exactly the semantics needed.**
`DefaultValueSerializer::as_bytes(&self, data_format: &str) -> Result<Vec<u8>, Error>`
(`value.rs:919-926`) is per-format and may refuse — and `ExtValue::UIElement` already refuses this
way with `ErrorType::SerializationError` (`mod.rs:309-316`):

| Value | `as_bytes` |
|---|---|
| `RecordChunk` | `json`, `ndjson`, `csv`, later `parquet` and `arrow` |
| `RecordSource` (manifest) | `json` — the query list |
| `RecordSource` (materialized) | `json`, `ndjson`, `csv` — it holds the chunks, so it can serialize as data |

Both variants serialize in every case, so **no refusal arm is needed at all** — another thing the
split removes. Where an error *is* constructed anywhere in this module it uses a **typed
constructor**, `Error::from_error(ErrorType::SerializationError, …)` as the neighbouring
`ExtValue::UIElement` arm does, never `Error::new`, which `CLAUDE.md` forbids. Worth stating because
`value.rs:956` and `:968` reach for `Error::new` for serialization errors today; this module does
not copy that.

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
(`RecordChunk` and `RecordSource`, bare CamelCase — Liquers owns both concepts); implement the
conversions in `ExtValueInterface` and `DefaultValueSerializer`; and add both `TypeInfo` entries to
`ExtValue::type_descriptions()` (`mod.rs:148`) — `CLAUDE.md`'s "four steps, not three; a type with no
`TypeInfo` cannot be stored".

**Everything lives in `liquers-lib`, behind the `records` feature** — see §"Integration Points".
Records are a data type rather than a core capability, so `liquers-core` is untouched apart from two
generic stream aliases.

**Serializing the multi-gigabyte case** still meets `VALUE-SERIALIZATION-HAS-NO-INCREMENTAL-WRITER`:
`as_bytes` returns a `Vec<u8>`, so NDJSON or CSV over a large stream cannot stream through it. A
`Manifest` sidesteps the problem — the manifest itself is small — which is one more reason to prefer
that form.


## Data Structures

New module `liquers-lib/src/records/`, behind the `records` feature.

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
    pub sources: Vec<ChunkOrigin>,
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

### How Arrow compatibility actually works

> **Specification references.** Everything in this section is against the Apache Arrow format
> specification; the documents are cited rather than paraphrased from memory.
>
> | Ref | Document | Used for |
> |---|---|---|
> | **[COLUMNAR]** | [Arrow Columnar Format](https://arrow.apache.org/docs/format/Columnar.html) | Buffer layouts, validity bitmaps, alignment and padding, the physical layout of each type |
> | **[CDATA]** | [Arrow C Data Interface](https://arrow.apache.org/docs/format/CDataInterface.html) | `ArrowSchema` / `ArrowArray` structs, format strings, release-callback semantics |
> | **[IPC]** | [Arrow IPC and serialization](https://arrow.apache.org/docs/format/Columnar.html#serialization-and-interprocess-communication-ipc) | The file and stream formats, for the deferred `.arrow` serialization |
>
> Format strings and section names below use the spec's own vocabulary. **They are to be verified
> against [CDATA] at implementation** rather than trusted from this document — a wrong format string
> fails at the boundary, loudly or silently depending on the consumer.

A fair objection has to be answered first: *Rust does not define a struct memory layout, so how can
a Rust struct be Arrow-compatible?*

**It cannot, and it does not have to — because Arrow does not standardize structs either.** Arrow
standardizes **buffers**: contiguous runs of bytes holding primitive values, offsets, or packed bits,
little-endian, with a recommended alignment. An Arrow "array" or "record batch" is a *logical*
description of how some buffers fit together. It has no canonical in-memory struct anywhere, in any
language.

Compatibility therefore lives at two separate levels, and conflating them is what makes informal
descriptions of it misleading:

| Level | What crosses | Why it is safe |
|---|---|---|
| **Data** | the *contents* of `Buffer<T>` — an `[i64]`, `[i32]`, `[f32]`, `[u8]` | Rust **does** define this. A slice of a primitive is N contiguous values of known size and alignment, with the target's endianness. `#[repr(Rust)]` never enters into it: we share `&[T]`, never a struct |
| **Structure** | which buffers, in what order, with what types | Rebuilt at the boundary as **`#[repr(C)]`** structs, whose layout *is* guaranteed |

That second row is precisely what the **Arrow C Data Interface** is for. It is two small `repr(C)`
structs ([CDATA] *Structure definitions*) — `ArrowSchema` (a type as a format string, a name,
children) and `ArrowArray` (length, null count, offset, a pointer array of buffers, children, a
`release` callback, `private_data`). Arrow
deliberately declined to standardize in-memory structs across languages and standardized a **handoff
ABI** instead. Our layout decision is what lets the buffer pointers in that ABI point straight at our
data rather than at a converted copy.

**Can a whole chunk be shared, not just a column?** Yes. `ArrowArray` is **recursive** — it carries
`n_children` and `children: *mut *mut ArrowArray`. A `RecordBatch` exports as a *struct array*: one
root `ArrowArray` whose children are the column arrays, each pointing at our buffers. Exporting
therefore allocates a small tree — one `ArrowArray` and one `ArrowSchema` per column, plus the root
and the buffer-pointer arrays — and copies **no data**.

> "Zero-copy" is true of the **data** and false of the **description**. The description is
> O(number of columns) tiny allocations. Worth saying plainly, because the earlier phrasing implied
> the whole thing was free.

**Ownership is the part that needs care, and it is where the `unsafe` lives.** [CDATA] *Release
callback semantics* puts the obligation on the consumer: it takes the struct and must call `release`,
which marks itself done by nulling the callback pointer and must release children before the parent. The producer boxes a private struct holding clones of the
`Arc`s behind every exported buffer, stashes it in `private_data`, and drops it in `release`. That
keeps our buffers alive exactly as long as pandas or polars holds them, and is the reason this code
belongs in `liquers-py` beside the existing pyo3 surface rather than in core.

#### Where our layout meets Arrow's, and three places it does not line up 1:1

| `Column` variant | Arrow type | [CDATA] format | Buffers | [COLUMNAR] section | Note |
|---|---|---|---|---|---|
| `Bool` | `Bool` | `b` | validity, values | *Fixed-size primitive layout*, *Validity bitmaps* | Both bitmaps, LSB-first — matches |
| `Int` | `Int64` | `l` | validity, values | *Fixed-size primitive layout* | Direct |
| `UInt` | `UInt64` | `L` | validity, values | as above | Direct |
| `Float` | `Float64` | `g` | validity, values | as above | Direct |
| `Date` | `Date32` (days) | `tdD` | validity, values (`i32`) | *Temporal types* | Direct |
| `Timestamp` | `Timestamp(µs)` | `tsu:` | validity, values (`i64`) | *Temporal types* | The trailing colon carries the (empty) timezone |
| `Text` | `Utf8` | `u` | validity, offsets, data | *Variable-size binary layout* | **Offsets hold `len + 1` entries starting at 0** — a spec invariant the builder must enforce, not an implementation detail |
| `Binary` | `Binary` | `z` | validity, offsets, data | as above | Same `len + 1` invariant |
| `Vector` | `FixedSizeList(Float32, dim)` | `+w:<dim>` | validity **only** | *Fixed-size list layout* | **Two levels.** The list node carries validity and *no* value buffer; the values live in a **child** `Float32` (`f`) array. Our flat `Buffer<f32>` is right, but the export emits a child node |
| `RecordBatch` | `Struct` | `+s` | validity **only** | *Struct layout* | The root of an exported chunk; the columns are its children |

Those three rows are called out because they are the ones that get discovered late otherwise.

**A useful coincidence on alignment.** [COLUMNAR] *Buffer Alignment and Padding* **recommends**
allocating buffers on 64-byte boundaries and padding their length to a multiple of 64; it **requires**
only 8. The `#[repr(align(64))]` backing-chunk approach chosen for `AlignedBuffer` over-allocates
to a 64-byte multiple anyway, so it produces Arrow's recommended padding for free — the logical length
is tracked separately, as it must be regardless.

**Endianness** is not a practical concern: [CDATA] is same-process, so producer and consumer agree by
construction, and [IPC] specifies little-endian as the default, which every target this project
builds for already is. One line, not a design problem.

#### The three routes, honestly rated

| Route | Data copied | Where | Status |
|---|---|---|---|
| **C Data Interface** | none | `liquers-py` | The real zero-copy path. A few hundred lines and the only `unsafe`. **`polars-arrow 0.55.2` is already in the lockfile** via polars, and exposes its own C Data Interface — so the native polars hand-off can go through it rather than hand-rolling FFI, whenever the `polars` feature is on |
| **Arrow IPC / Feather** | yes — it is serialization | `liquers-lib`, deferred | Not sharing. A flatbuffers encoder; also the natural `.arrow` file format for a chunk |
| **Typed arrays over wasm memory** | none | `liquers-web` | Needs a validity discipline — see below |

#### The wasm route: a dedicated safe mechanism, not Arrow

There is no C Data Interface in a browser — JavaScript cannot consume `repr(C)` structs — so the
browser gets **its own sharing mechanism**, which need not be Arrow-shaped. The goal stands: read the
data in place, in linear memory, without copying.

The route is often called fragile and left there. That is too pessimistic: there are *two* hazards,
not one, and they have completely different characters:

| | Hazard A — the heap grows | Hazard B — the buffer is freed or moved |
|---|---|---|
| What breaks | the JS `ArrayBuffer` is **detached** and `memory.buffer` returns a new object | the pointer **dangles** |
| Does the Rust data move? | **No.** wasm growth extends linear memory; it never relocates existing pages | Yes, or it is gone |
| Detectable from JS? | **Yes, exactly and in O(1)** | **No.** Reads return plausible garbage, silently |
| Answer | detect and refresh | **prevent structurally** |

**Hazard A is easy, and this is the key fact:** growth invalidates the JS *view object*, not the Rust
*pointer*. The detection is a reference comparison —

```js
if (view.buffer !== memory.buffer) { /* stale */ }
```

— and the refresh is re-creating the view at the **same pointer and length**, because the allocation
never moved:

```js
view = new Float64Array(memory.buffer, ptr, len);
```

**Hazard B is the dangerous one, and it is prevented rather than detected.** The handle holds an
`Arc<RecordBatch>`, which keeps every buffer alive; our buffers are `Arc<[T]>` and **immutable once
built** (§"Concurrency Considerations"), so they never reallocate. For as long as JS holds the
handle, the pointers are stable. This is the same ownership discipline as the C Data Interface's
`release` callback, expressed in a way wasm-bindgen already supports.

**The simplest discipline, which makes most of this moot: do not cache views.** Constructing
`new Float64Array(memory.buffer, ptr, len)` is O(1) — a small JS object wrapping a pointer, with no
data copy — so creating it at the point of use costs nothing measurable and makes staleness nearly
unreachable. A view only goes stale if it is held across a call into wasm or across an `await`.

**The API.** Following `liquers-web`'s existing handle convention (`#[wasm_bindgen(js_name = …)]` over
an inner value, as `LiquersQuery` and `LiquersKey` do):

```rust
// liquers-web/src/records.rs
#[wasm_bindgen(js_name = RecordChunk)]
pub struct LiquersRecordChunk {
    /// Keeps every buffer alive for the handle's lifetime — the answer to Hazard B.
    inner: Arc<RecordBatch>,
}

#[wasm_bindgen(js_class = RecordChunk)]
impl LiquersRecordChunk {
    #[wasm_bindgen(getter, js_name = numRows)]    pub fn num_rows(&self) -> usize;
    #[wasm_bindgen(getter, js_name = numColumns)] pub fn num_columns(&self) -> usize;
    #[wasm_bindgen(js_name = schemaJson)]         pub fn schema_json(&self) -> String;
    /// Descriptor for one column: kind, pointer, length, and the validity bitmap when present.
    /// Enough for JS to build a view; carries no data itself.
    pub fn column(&self, i: usize) -> Result<JsValue, JsValue>;
    /// The always-safe fallback: an owned typed array, detached from linear memory. O(n).
    #[wasm_bindgen(js_name = columnCopy)]
    pub fn column_copy(&self, i: usize) -> Result<JsValue, JsValue>;
    // `free()` is generated by wasm-bindgen and drops the Arc.
}
```

and a small JS companion in which the refresh *is* the getter:

```js
class RecordColumn {
  constructor(memory, desc) { this.memory = memory; this.desc = desc; this._view = null; }
  get view() {                       // one reference comparison per access
    if (this._view === null || this._view.buffer !== this.memory.buffer) {
      this._view = makeView(this.memory.buffer, this.desc);   // same ptr, same len
    }
    return this._view;
  }
  toCopy() { return this.view.slice(); }   // the fallback, explicit
}
```

That is the whole refresh mechanism: roughly ten lines, one reference comparison per access, no
copying on the fast path, and an explicit copy available whenever a caller wants a value that outlives
linear memory.

**Two constraints that must be documented, not discovered:**

1. **The views are read-only.** Writing through them into buffers Rust holds as immutable `Arc<[T]>`
   would violate the aliasing assumptions the rest of the design relies on. Single-threaded JS plus
   wasm means concurrent *reading* is fine; writing is not.
2. **A view must not outlive the handle.** `free()` drops the `Arc`, and a view held past that point
   is Hazard B with no detection. The `debug-handles` feature already used for `RUNTIME05` gives the
   test: assert the live chunk-handle count returns to zero after `free()`.

**Two escape hatches worth recording, neither relied on:**

- **`SharedArrayBuffer`.** If the wasm memory is created with `shared: true`, growth does **not**
  detach — a growable `SharedArrayBuffer` grows in place and existing views stay valid, removing
  Hazard A entirely. The cost is cross-origin isolation (COOP/COEP headers), a deployment burden that
  should not be a precondition for reading a table.
- **Pre-reserving the heap** so growth never occurs in a session. It makes the fast path the common
  path; it is not a correctness guarantee, and the identity check stays regardless.

**Test to write, because it is the one that would otherwise be skipped:** take a view, force
`memory.grow` by allocating in between, then read — and assert the wrapper refreshed transparently
and returned the right values.

**Copying is an accepted fallback, not the plan.** `columnCopy` exists and is always correct; the
design above means it is a caller's deliberate choice rather than the only safe option.

**Still no claim of full Arrow support.** A deliberate subset: the types above, no nested `Struct`
beyond the batch root, no `Union`, no dictionary encoding, no large (64-bit offset) variants. Enough
for a record batch and a pandas hand-off; extendable, and honest about not being arrow-rs.

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
compatibility: **Arrow's validity buffer *is* a bitmap** ([COLUMNAR] *Validity bitmaps*, LSB-first
within each byte), so a `Vec<bool>` cannot be handed over at
all. It would need converting, which destroys the zero-copy property that is the entire point of the
columnar layout.

**One simplification, which the spec sanctions:** validity is `Option<Bitmap>` and is **omitted
entirely when a column has no nulls** — [COLUMNAR] *Validity bitmaps* allows the buffer to be omitted
when the null count is zero, and [CDATA] carries `null_count` on `ArrowArray`, where `-1` means
"unknown" and forces the consumer to count. Most columns in
practice have none and pay nothing.

Size: roughly 150–200 lines with tests — `get`, a builder, `and`/`or`/`not`, `count_ones`. The one
fiddly part is slicing at a non-byte boundary, which the spec permits through `ArrowArray::offset`
([CDATA]); the first
version **requires byte-aligned slice offsets** and copies when a caller asks for anything else,
which removes the fiddly case at a cost paid only by an unusual slice.

### 64-byte alignment: worth doing, and cheaper than it looks

[COLUMNAR] *Buffer Alignment and Padding* requires 8-byte buffer alignment and **recommends** 64, so
a consumer can use aligned SIMD loads without a special case at the tail. This is a performance and
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

### RecordSchema — three axes, because two were not enough

A search engine cannot work from type alone, so the schema carries a **logical type** (for Arrow,
polars, GlueSQL) *and* an indexing **role** (for Tantivy, Lucene, Qdrant). The role must be
**composable**, not a single choice: the things an engine needs to know are independent capabilities.

The clearest case is the commonest field in any search index: a title you want to *search* and also
*display*. In Tantivy that is `TEXT | STORED`. A role modelled as a plain enum cannot say it, and
three more things go with it:

| Would be inexpressible as an enum | Why it matters |
|---|---|
| `TEXT \| STORED` — searchable *and* retrievable | The normal case for a title or a summary |
| Tokenizer / analyzer choice | "raw" vs "en_stem" vs a language-specific one changes what matches |
| Index granularity — docs / freqs / **positions** | The search syntax promises phrase queries (`"expiration safety"`), which **require positions** |
| Vector distance metric | Qdrant demands cosine / dot / euclidean at collection creation; `dim` alone does not say |

```rust
pub struct RecordSchema {
    pub fields: Vec<FieldSchema>,
    /// Liquers' own type identity for the thing the rows describe, when there is one.
    pub type_identifier: Option<String>,
}

/// The logical type. Maps one-to-one onto the `Column` variants and onto Arrow's `DataType`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FieldType { Bool, Int, UInt, Float, Text, Binary, Date, Timestamp, Vector }

pub struct FieldSchema {
    pub name: String,
    pub data_type: FieldType,
    pub nullable: bool,
    /// Structural role in the batch — nothing to do with indexing.
    pub key: KeyRole,
    /// What an index should do with this field. Portable *intent*; engines translate it.
    pub role: FieldRole,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum KeyRole {
    /// The row's identity. **Exactly one per schema.**
    Id,
    /// Index into `RecordBatch`'s origin dictionary. At most one.
    Source,
    /// Ordinary data.
    None,
}

/// The access paths this field **affords**. An engine takes the subset it can serve
/// and reports the rest as declined; nothing here is a command.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct FieldRole {
    /// How the field can be searched. Empty — not searchable at all.
    /// **Plural**: one field routinely has several access paths in one engine — a B-tree
    /// *and* a trigram index in Postgres, a `text` field with a `keyword` sub-field in
    /// Elasticsearch.
    pub indexed: Vec<IndexKind>,
    /// Retrievable from the engine. Lucene `stored`, Tantivy `STORED`, Qdrant payload.
    pub stored: bool,
    /// Available for sorting, faceting and cheap scans. Lucene docValues, Tantivy `FAST`.
    pub fast: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum IndexKind {
    /// Not tokenized: exact term match and faceting.
    Exact,
    /// Matching *inside* a token: `LIKE '%x%'`, trigram, fuzzy. Neither `Exact` nor
    /// `FullText`, and common in relational and search engines alike.
    Substring,
    /// Tokenized. `positions` is **required for phrase queries**.
    FullText { analyzer: Analyzer, positions: bool },
    /// Ordered comparisons.
    Range,
    /// Vector similarity.
    Similarity { metric: VectorMetric },
}

/// Portable analyzer intent. An engine maps it to its own tokenizer;
/// `Named` is the escape hatch for one this vocabulary cannot describe.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Analyzer { Raw, Simple, Stemming { language: String }, Named(String) }

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum VectorMetric { Cosine, Dot, Euclidean }
```

Constructors keep the old ergonomics, so the common cases stay one call —
`FieldRole::text()`, `::keyword()`, `::stored_only()`, `::numeric()`, `::vector(metric)`,
`::ignored()` — composing with `.and_stored()` and `.and_fast()`. `FieldRole::text().and_stored()`
is the title case that started this.

#### The identity field, specifically

The question "how does the engine know what an id is" has a sharper answer than "a role exists for
it", because reconciliation constrains it. Updating an engine means **delete-by-term followed by
insert** — Lucene's `updateDocument(Term, …)`, Tantivy's `delete_term`. You cannot delete by a
*tokenized* term. So `KeyRole::Id` **implies** its `FieldRole`, and `RecordSchema::new` enforces it
rather than documenting it:

```rust
// Enforced invariants, all checked once per schema:
//   exactly one field with KeyRole::Id
//   that field has indexed == Some(IndexKind::Exact)   — delete-by-term needs it
//   that field has stored == true                       — a hit must say which record it is
//   at most one field with KeyRole::Source
```

That is the same discipline Tantivy uses — the schema is validated when built and hands out field
handles — and it means a schema that *cannot* be reconciled is rejected at construction rather than
at the first refresh.

#### Roles are capabilities, not commands

A field declares the access paths its data **affords**; each engine takes the subset it can serve and
reports the rest, per the sink contract. So the same schema serves Postgres (B-tree plus trigram),
Tantivy (`STRING` plus `TEXT`), Qdrant (a keyword payload index) and a CSV export (none) without any
of them knowing about the others — and without the schema carrying per-engine role maps, which would
be engine configuration leaking into data.

Nothing enforces that an engine honours a role, and a record stream is perfectly usable with every
role left at default. **The one exception is the `Id` field**, whose `Exact` + stored requirement is
enforced in `RecordSchema::new`, because a schema that cannot be reconciled fails silently otherwise.

#### Access paths, not data structures — and what that excludes

The model says which *queries* are supported, never which structure supports them. The relational
distinction falls out on its own rather than needing to be named:

| Relational index | In this model |
|---|---|
| **B-tree** | `Exact` **and** `Range` — a B-tree is ordered, so it serves both |
| **Hash** | `Exact` only — precisely the functional difference from a B-tree |
| GIN/GiST over text | `FullText { … }` |
| `pg_trgm`, `LIKE '%x%'` | `Substring` |
| BRIN | `Range` with weaker selectivity — a statistics concern, not a capability one |
| Spatial (R-tree, PostGIS) | **not represented** — no geo type in the subset |

**A functional index is a derived column, not a role.** `CREATE INDEX ON t (soundex(name))` indexes a
derived *expression*, which is a property of a `(field, function)` pair rather than of the field —
admitting it would put expressions in the schema, at which point the schema is a query language.
The Liquers answer is better and already available: a command adds a `name_soundex` column with
`IndexKind::Exact` through `RecordBatch::with_columns`, and the index is then on a real field. Same
for `lower(email)` or any normalised form. The derivation becomes visible, cacheable data instead of
hidden engine configuration.

(This holds while Liquers owns the schema. Reading *from* a database that already has functional
indexes, no column can be added — those indexes are the database's business and do not appear here.
See `engine-survey.md` §3.)

#### What is portable, and what is not

The important architectural point, and the answer to "this can hardly be based just on type":
**indexing information travels in three layers, and only the first two belong to a record.**

| Layer | Owned by | Example | Why there |
|---|---|---|---|
| **1. Logical type** | `FieldType` | `Text`, `Int`, `Timestamp`, `Vector` | What the data *is*. Serves Arrow, polars, GlueSQL, which ignore roles entirely |
| **2. Portable intent** | `FieldRole` | searchable-how, stored, fast | What an index *should do*. Every engine has these concepts under different names |
| **3. Engine specifics** | **the sink's configuration, not the schema** | Tantivy tokenizer registration, Qdrant HNSW `m`/`ef_construct`, a Lucene custom `Analyzer` | Only one engine has them |

**Layer 3 must not enter the schema**, and this is a firm boundary rather than a preference: a CSV
reader producing records has no idea Tantivy exists, and a schema that accumulated every engine's
options would become their union. The interoperability layer already says an engine is **configured
by a Liquers query** — so engine-specific overrides live there, keyed by field name, layered over the
intent the schema declares. A field says "full-text, stemming, English"; the Tantivy sink's config
may say "for field `body`, use my registered `en_stem_custom` tokenizer".

#### Mapping the intent onto each target

| Intent | Tantivy | Lucene | Qdrant |
|---|---|---|---|
| `Exact` | `STRING` | `StringField`, `indexOptions=DOCS` | payload index `keyword` |
| `Substring` | via an ngram tokenizer | via an ngram/shingle analyzer | payload index `text` with a substring tokenizer |
| `FullText { positions: true }` | `TEXT` (freqs **and positions**) | `TextField`, `DOCS_AND_FREQS_AND_POSITIONS` | payload index `text` + tokenizer |
| `FullText { positions: false }` | `TextOptions` with `WithFreqs` | `DOCS_AND_FREQS` | as above, phrases unavailable |
| `Range` | `INDEXED \| FAST` numeric | `LongPoint`/`DoublePoint` + docValues | payload index `integer`/`float` |
| `Similarity { metric }` | **not supported** — Tantivy has no first-class vector index; pair it with a vector store | — | named vector, `Distance::{Cosine,Dot,Euclid}` |
| `stored` | `STORED` | `Field.Store.YES` | payload (all payload is retrievable) |
| `fast` | `FAST` | docValues | payload index |
| `KeyRole::Id` | `STRING \| STORED`, used with `delete_term` | `StringField` + `updateDocument(Term)` | the **point id** — see below |
| `indexed: None, stored: false` | omitted | omitted | omitted |

Two honest limits fall out of writing this table, neither of which the earlier enum exposed:

- **Tantivy cannot serve the `Similarity` intent.** An engine may decline part of a schema, so the
  sink contract must let it report what it did not index, rather than silently dropping a field. That
  is a requirement on the interoperability layer that this design has just generated.
- **Qdrant point ids must be unsigned integers or UUIDs** — an arbitrary string record id does not
  fit, so that sink must hash or map ids and keep the original in the payload. A per-sink concern,
  but one that has to be *somewhere*, and the schema is not it.

| Target ignoring roles entirely | Uses |
|---|---|
| Arrow / polars / pandas | `FieldType` → `DataType`; `Column` → the same buffers |
| GlueSQL | `FieldType` → SQL type |
| tinysearch | only fields whose `indexed` is `FullText` |
| Liquers type system | `RecordSchema::type_identifier` — the `TypeInfo` identity of the described value |

**Engine API names above are to be verified at implementation** against the crate versions in use,
the same caveat that applies to the Arrow format strings. The *shape* of the mapping is the design;
the exact constant names are not load-bearing here.

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

### ChunkOrigin — identity, description and **retrieval**

```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChunkOrigin {
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

A row must be **retrievable**, not merely identified, and retrieval is a **query** rather than a
procedure a consumer has to implement.

### `rec_id` — the guaranteed path, as a query

`ns-rec/rec_id-<id>` is a real command: it takes a record stream and yields the single record whose
`Id` field matches. So the guaranteed retrieval path for any row is its chunk query with that
appended:

```
<chunk query>/ns-rec/rec_id-42
```

This works for **every** record stream, because every schema has exactly one `Id` field — the
invariant that already exists for reconciliation now also makes single-record addressing universal.
It is better than the earlier formulation ("re-evaluate the chunk and index by the `Id` field")
because that described work a consumer must do, whereas this is a string anyone can evaluate, put in
a recipe, or hand over an HTTP boundary.

**Appending respects query semantics, and the failure is silent.** Against a query, append directly;
against a **key**, a `/-/` must start a transform segment first:

| Form | Means |
|---|---|
| `-R/data/sales/daily_0010.csv/-/ns-rec/rec_id-42` | `GetAsset[data, sales, daily_0010.csv]` then `Action{rec_id, 42}` — **correct** |
| `-R/data/sales/daily_0010.csv/ns-rec/rec_id-42` | **one** `GetAsset` over the whole path — a file literally named `…/ns-rec/rec_id-42` |

Both parse. Only the first does what is meant, which is why constructing the query belongs in code
(`ChunkOrigin::locator_query`) rather than in string concatenation by a caller.

### `locator` is the optimization, not the guarantee

With `rec_id` universal, `LocatorRule` no longer carries the guarantee — it carries the **shortcut**:
a projection that can address one row *without* producing its whole chunk
(`-R/f.csv/-/ns-csv/row-42` reading one line rather than parsing the file). Optional by definition,
and now clearly an optimization rather than a second mechanism competing with the first.

`info` is optional because **a CSV row has no `AssetInfo`; the file does**, and one per source rather
than per row also keeps 656 bytes from repeating.

### There is no collection type above the batch

A bounded set of rows is either a **`RecordChunk`** (one materialized batch) or a **`RecordSource`**
that happens to be finite. No third type sits between them: a `RecordSet { batches: Vec<RecordBatch> }`
would duplicate what a source already expresses, and would need its own answer to every question the
source has already answered about sharing, serializing and re-opening.

`truncated` therefore lives on `RecordSource`, as a producer's report that it stopped early.

**Diagnostics live on `Metadata`, not in the data.** "How many records were scanned" and "you named a
field no schema declares" are facts about an *evaluation*, not about *data*; carrying them in the
value would force every consumer of a batch to hold a field it does not want. `Metadata` already owns
per-evaluation reporting through `LogEntry`, with `Info`, `Debug`, `Warning` and `Error` kinds
(`metadata.rs:495-552`), so a producer logs a scan count as `Info` and each unresolvable field as
`Warning`.

The cost, stated: a log entry is a string, so a list of unavailable fields is not structured. That is
accepted because `LogEntry` is this project's answer for this class of information everywhere else,
and the reader is a person or an agent asking why nothing matched.

### Search evidence is columns, not a parallel vector

A search result needs per-row evidence — which clause matched, an excerpt, eventually a score. It
does **not** get a dedicated field on a record type, and it does not get a parallel vector alongside
one: a record type carrying a field only search fills is a record type that knows what a clause is,
which is precisely what separating these designs avoids.

Evidence is expressed the way every other per-row fact is — **as columns**, appended to the result
schema, stored but not indexed:

| Column | Type | Meaning |
|---|---|---|
| `match.clauses` | `UInt` | Bitmask; bit *i* set when clause *i* of the predicate admitted this row |
| `match.excerpt` | `Text`, nullable | The best excerpt, when a text clause produced one |
| `match.score` | `Float`, nullable | `Null` until a scoring clause exists |

So a search result composes with any record consumer without unwrapping, and the evidence serializes
as CSV or NDJSON like everything else. **The cost, stated:** the bitmask caps a predicate at **64
nodes**,
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

## Trait Implementations

| Trait | For | Note |
|---|---|---|
| `MaybeBoxedStream` | blanket over `Stream` | Mirrors the existing `MaybeBoxed` |
| `ExtValueInterface` conversions | `ExtValue::RecordChunk`, `ExtValue::RecordSource` | `from_*`/`as_*` arms, plus `into_stream` / `into_source`, per `TYPE_SYSTEM_GUIDE.md` |
| `DefaultValueSerializer` | `ExtValue::RecordChunk`, `ExtValue::RecordSource` | chunk: json / ndjson / csv. Stream: json for a manifest, `SerializationError` for an opaque one |

**No change to `AsyncStore` or `AssetManager`.** Record production is a *command* concern; a trait
method would be a push-down optimization, addable later without changing a consumer.

## Generic Parameters & Bounds

None on the data types — `FieldValue` is a dynamic enum precisely so `RecordBatch`, the stream and
the value variants stay concrete. The only bound in the module is `T: bytemuck::Pod` on `Buffer<T>`,
which is what makes the aligned cast safe.

## Function Signatures

### `liquers-lib/src/records/mod.rs`

```rust
impl RecordSchema {
    /// Fails unless exactly one field has `KeyRole::Id`, that field is `Exact`-indexed and
    /// stored (delete-by-term needs both), and at most one field has `KeyRole::Source`.
    /// Checked once per schema rather than per row.
    pub fn new(fields: Vec<FieldSchema>) -> Result<Self, Error>;
    pub fn id_field(&self) -> usize;
    pub fn source_field(&self) -> Option<usize>;
    /// Columns whose `indexed` is `FullText` — what an unqualified text query matches.
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

impl ChunkOrigin {
    /// Build the directly evaluable query for one row, when `locator` allows.
    pub fn locator_query(&self, id: &FieldValue) -> Option<Query>;
}

impl RecordSource {
    /// Chunks without producing records. `Unbounded` when the count is not known —
    /// see `chunking-and-resumability.md` for why the distinction is in the API now.
    pub fn chunks(&self) -> ChunkList<'_>;
    /// A fresh traversal; callable any number of times.
    pub async fn stream(&self, context: &Context<impl Environment>)
        -> Result<RecordBatchStream<'_>, Error>;
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

### `liquers-lib/src/records/buffer.rs`

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

### Crate placement: `liquers-lib`, behind a `records` feature

**The whole feature is `liquers-lib`, behind a `records` feature.** It is in essence a new data
type, not an essential capability — the same class as `image-support` and `polars`, and `CLAUDE.md`
already directs new value types to `liquers-lib/src/value/`.

**`liquers-core` is therefore untouched, with one small exception.** The design is purely additive to
one crate, and a build with `records` off is byte-for-byte the build that exists today.

| Crate | File | Change |
|---|---|---|
| `liquers-lib` | `src/records/buffer.rs` (new) | `AlignedBuffer`, `Buffer<T>`, `Bitmap` — the Arrow-layout primitives; the only place `bytemuck` is used |
| `liquers-lib` | `src/records/mod.rs` (new) | `FieldValue`, `RecordSchema`, `FieldSchema`, `FieldType`, `Column`, `RecordBatch`, `ChunkOrigin`, `LocatorRule`, `RecordSource`, `SourceBacking`, `ChunkKeys`, `ChunkId`, `ChunkList`, `ChunkDescriptor`, `RecordBatchStream` |
| `liquers-lib` | `src/records/commands.rs` (new) | The `ns-rec` command set |
| `liquers-lib` | `src/records/polars.rs` (new, `records` + `polars`) | `RecordBatch → polars::DataFrame` over the shared buffers |
| `liquers-lib` | `src/value/mod.rs` | `ExtValue::RecordChunk` and `ExtValue::RecordSource`, **cfg-gated**, with every exhaustive match gaining a gated arm; both `TypeInfo` entries; the `DefaultValueSerializer` arms |
| `liquers-lib` | `Cargo.toml` | the `records` feature and its optional `bytemuck` dependency |
| **`liquers-core`** | `src/maybe_send.rs` | **The one exception** — `BoxStream` + `MaybeBoxedStream`, ungated. Justified below |
| `liquers-web` | `src/records.rs` (new) | The `RecordChunk` handle, per-column descriptors, `columnCopy`; the JS companion that revalidates views |
| `liquers-web` | `Cargo.toml` | add `"records"` to the `liquers-lib` feature list |
| `liquers-axum` | `src/axum_integration.rs` | A streaming branch for `RecordSource` + `csv`/`ndjson`: `Body::from_stream`, eager first batch, uniform-schema check. No new dependency — axum 0.8.9 and `futures` are already there |
| `liquers-py` | later milestone | Arrow C Data Interface export — the only place `unsafe` FFI belongs |
| `specs` | `command_registry.yaml` | Regenerated |

### The one change to `liquers-core`, and why it is not records-specific

`BoxStream` and `MaybeBoxedStream` go in `liquers-core/src/maybe_send.rs`, **ungated**, beside the
existing `BoxFuture` and `MaybeBoxed`. That module exists precisely to hold per-target boxed-type
aliases and to document the E0225 reason they must be aliased rather than bounded. `StreamExt::boxed()`
has the same always-`Send` defect the module already warns about for `FutureExt::boxed()`, so the
alias is a general async utility that any crate may want — not a records concept.

Defining it in `liquers-lib` instead would duplicate the E0225 workaround and leave core's own
documented trap half-addressed. Two type aliases and one blanket trait, no dependency, no feature: a
smaller cost than the duplication.

### The feature

```toml
# liquers-lib/Cargo.toml
[features]
default = ["egui", "image-support", "polars", "records"]
# Columnar record streams: a tabular value type with an Arrow-compatible layout.
# Optional because it is a data type rather than an essential capability.
records = ["dep:bytemuck"]

[dependencies]
bytemuck = { version = "1.25", optional = true }
```

**In `default`, exactly as `polars` is** — so the routine loop
(`cargo test -p liquers-lib --lib --tests`) exercises it, which is the only way the tests actually
run. Being in `default` is not the same as being mandatory: the matrix below proves the feature is
cleanly optional, and `polars` is the established precedent for precisely this arrangement.

**`bytemuck` is optional and gated.** It is in `Cargo.lock` at 1.25.2 but is a direct dependency of
no workspace crate today — it arrives transitively through `egui`, so a `--no-default-features` build
does not have it at all. As an optional `liquers-lib` dependency reached only through `records`, a
build without the feature adds no dependency.

### Feature-gating discipline

A cfg-gated enum variant is the classic way to break a build that was not tested, and `ExtValue`
already carries the scar tissue — its `as_bytes` match has a comment recording that a previous
catch-all "silently absorbed new variants". Every exhaustive match on `ExtValue` therefore needs a
`#[cfg(feature = "records")]` arm, in `type_name`, `type_identifier`, `as_bytes`,
`type_descriptions` and the `ExtValueInterface` conversions.

`scripts/check-build-matrix.sh` gains the rows that prove it, mirroring the existing per-feature rows:

```
--no-default-features --features records --tests
--no-default-features --features records,polars --tests     # the polars bridge
--no-default-features --features webui,records --tests
--target wasm32-unknown-unknown --no-default-features --features webui,records
```

and the existing `--no-default-features --tests` row already proves the build with `records` **off**.
Test files that need the feature carry `#![cfg(feature = "records")]` at file level, as the existing
optional-dependency test files do.

### What this settles, and what it costs

This also places **the search design's predicate**: `SearchPredicate` operates on `RecordBatch`, so
with records in `liquers-lib` the predicate cannot live in `liquers-core` either. That is no loss — the search commands were always
`liquers-lib` — and it means the search design too becomes additive to one crate.

The cost, stated: a consumer wanting records without the rest of `liquers-lib` cannot have them,
because the crate is the unit of dependency. That is acceptable while `liquers-lib` is where command
libraries live anyway, and if a `liquers-records` crate is ever wanted, the module is already
self-contained enough to lift out.

## Streaming a record source over HTTP (`liquers-axum`)

Serializing a record source to CSV or NDJSON over HTTP must not build the
whole document in memory — which is the entire point of the chunked design, and would otherwise be
undone at the last step. This is **a new pattern for Liquers**, and it is taken ad-hoc for now, with
the general mechanism deferred (below).

### What it breaks in the normal flow

| | Normal flow | Streaming |
|---|---|---|
| Serialization | `as_bytes(format) -> Vec<u8>`, whole document | per batch, incrementally |
| Body | `Body::from(bytes)` | `Body::from_stream(…)` |
| `Content-Length` | known | **absent** — chunked transfer-encoding |
| Errors | status chosen after serializing | **status is already sent** |
| Caching | serialized result cached | nothing to cache |

`Body::from_stream` is available in axum 0.8.9 (already the dependency), and `futures 0.3.34` is
already a direct `liquers-axum` dependency, so **no new dependency is required**.

### Why a *source* makes this work, and a stream would not

The handler receives an `ExtValue::RecordSource` from the asset layer — shareable, cacheable, not
consumed by use — and **opens the stream itself, inside the response body**. Had streams been values,
the value reaching the handler could already be partly consumed by whatever touched it first, and two
concurrent requests for the same URL would race for one traversal.

**The value is re-openable, so every request gets its own traversal of the same source** — the
clearest practical argument for separating source from stream.

```rust
// liquers-axum — sketch
let source: Arc<RecordSource> = /* from the evaluated value */;
let mut stream = source.stream(&context).await?;      // fails BEFORE headers, see below
let first = stream.next().await.transpose()?;          // pull one batch eagerly

let body = Body::from_stream(encode_batches(format, schema, first, stream));
Response::builder()
    .status(StatusCode::OK)
    .header(header::CONTENT_TYPE, format.mime_type())
    .body(body)
```

### The hard part: an error after the first byte

Once the status line and headers are sent, a failure **cannot** become a 500. Three mitigations, in
order of how much they buy:

1. **Pull the first batch before sending headers** — the sketch above. Most failures are front-loaded:
   a bad query, a missing store entry, a schema that fails validation, a non-uniform stream asked for
   as CSV. Converting those into a proper 4xx/5xx costs one batch of latency and one batch of memory,
   and is the single highest-value thing here.
2. **NDJSON can carry an error in band.** A final line `{"error": "…"}` is machine-readable, and a
   client that parses each line sees it. This is a good reason to prefer NDJSON for large exports.
3. **CSV cannot.** There is no in-band error representation, so a mid-stream failure **truncates**,
   and a truncated CSV is indistinguishable from a complete one. An HTTP trailer
   (`Trailer: X-Liquers-Error`) is emitted where possible, but client support is poor enough that it
   cannot be relied on. **This is a real, documented limitation, not an oversight** — and it is the
   reason the reference must say so plainly rather than leave a caller to discover it.

Every mid-stream failure is logged server-side regardless, since the client may never learn of it.

### Format constraints

**CSV requires a uniform schema** — §"Schema uniformity is declared" establishes there is no single
header otherwise. So the handler checks `RecordSource::uniform_schema` *before* sending headers and
refuses with `409 Conflict` naming NDJSON as the alternative when it is `None`. The eager first-batch
pull makes this check natural rather than bolted on.

| Format | Uniform schema | Header | Mid-stream error |
|---|---|---|---|
| `ndjson` | not required | — | in-band error line |
| `csv` | **required** | once, before the first batch | truncation |
| `json` | not required | `[` … `]` framing | truncation of an unterminated array — detectable |

### Caching, cancellation, backpressure

- **Caching:** a streamed response never materializes, so there is nothing to cache — but the
  **source is still a cacheable value**, so the expensive part (resolving the manifest, evaluating
  chunk queries) is cached as normal and only the *encoding* is repeated per request. That is the
  right trade and it means streaming does not defeat the asset layer.
- **Cancellation:** a client disconnect drops the body, which drops the stream. Everything is `Arc`-
  held, so `Drop` releases it; no special handling.
- **Backpressure:** hyper polls the body as the client reads, so a slow client slows production
  rather than filling a buffer. Correct by construction — worth stating because it is the property
  that makes streaming multi-gigabyte exports safe at all.

### Deferred: the general mechanism, and a finding about its shape

`VALUE-SERIALIZATION-HAS-NO-INCREMENTAL-WRITER` is the general gap, and this design is its clearest
motivating case. Writing this section produced a finding that **changes what that issue should ask
for**:

> The issue currently proposes a **writer-based** counterpart, `serialize_to_writer<W: Write>`,
> modelled on `liquers-lib`'s existing `serialize_dataframe_to_writer`. That is **push**-based: the
> serializer drives, writing when it chooses. An HTTP body is **pull**-based: hyper polls the stream
> and the producer must yield. Bridging push to pull needs a bounded channel or a duplex pipe, plus a
> task — real machinery, not an adapter.
>
> So a writer-based serializer alone would **not** straightforwardly serve HTTP. The general
> mechanism wants either a pull-based form (`serialize_to_stream(&self, format) -> BoxStream<Bytes>`)
> or both, with the writer form built on the stream form rather than the reverse.

Recorded on the issue so the eventual design starts from the right shape. Until then the ad-hoc path
above is a deliberate special case, confined to `liquers-axum`, that a general mechanism can replace
without changing any URL or any caller.

## Documentation Architecture

Two new documents, fully specified here so the authoring is mechanical.

**They are written in Phase 5, not now.** `CLAUDE.md` defines `specs/reference/` as "how the system
is; **must be true at HEAD**" — a reference document describing an unimplemented API would be false
the moment it lands. The specification below is the contract they are written against; per §9.2 each
carries a `## History` row and a `reviewed:` date from its first commit.

### Reference: `specs/reference/RECORD_STREAMS.md`

`kind: reference` · audience contributor **and agent** · area `lib/value` · status follows the design.

| § | Content |
|---|---|
| **Concepts** | The three abstractions and *why* they are three: `RecordSource` (asked repeatedly, shareable, serializable as a manifest), `RecordStream` (one traversal, never a value), `RecordChunk` (materialized table). The conversion diagram. The `Iterable`/`Iterator` analogy stated once, plainly |
| **Scales** | Record / batch / chunk — unit of retrieval, of memory, of refresh — and why conflating them breaks either memory or refresh |
| **Schema** | `RecordSchema`, `FieldSchema`, `FieldType`, `FieldRole`. The two orthogonal axes and which integration target each serves. The exactly-one-`Id` rule and that it is checked in `RecordSchema::new` |
| **Field naming** | The `meta.` / `attr.` / `key.` qualification, why `status` forced it, and that ambiguity is an error naming every candidate |
| **Identity and retrieval** | `(chunk id, record id)`. `ChunkOrigin`: `chunk` as the guaranteed path, `locator` as the direct one, `info` absent for a non-asset source. **Why a CSV row has no `AssetInfo` and the file does** |
| **Provenance and validity** | The `Metadata` per chunk; provenance as "the query and dependency versions this came from"; validity as the existing staleness check; the flyweight to record level |
| **Memory layout** | The columnar form, `Column` variants, `Bitmap`'s three uses, `AlignedBuffer` and 64-byte alignment. **Cites [COLUMNAR] per claim** |
| **Arrow interoperability** | The two-level model — data as `&[T]`, structure rebuilt as `repr(C)` — the exact type/format mapping table, the three export routes and their real costs, and the three places the layout is not 1:1. **Cites [COLUMNAR] and [CDATA]**; states that format strings are verified against the spec, not this document |
| **Browser sharing** | Hazards A and B, the identity check, the refresh rule, read-only views, the handle lifetime, and `columnCopy` as the fallback |
| **Methods** | Every public method with its contract and failure mode — `RecordSchema::{new, id_field, source_field, text_fields, index_of}`, `RecordBatch::{select, filter, slice, concat, value, with_columns}`, `RecordSource::{stream, chunks, manifest}`, `Bitmap::{get, and, or, not, count_ones}`, `ChunkOrigin::locator_query` |
| **Serialization** | What each value writes in each format, and that a manifest source writes its query list while a materialized one writes data |
| **Limits** | The Arrow subset supported and what is excluded; uniformity not promised and the two operations that need it; the feature gate |

Links out to `VALUE_TYPE_SYSTEM.md`, `STORE_SEMANTICS.md` for the key semantics it inherits, and the
design folder for *why*.

### Guide: `specs/guides/RECORD_STREAM_GUIDE.md`

`kind: guide` · audience contributor · area `lib/value` · workflow **"produce records from a new
source"**.

| § | Content |
|---|---|
| **Choose your shape first** | A decision table: one row per asset → return a chunk; a directory of files → a manifest source; a huge single file → a source whose stream yields batches. Getting this wrong is the expensive mistake, so it comes first |
| **Walkthrough: a command producing records** | **The guide's spine.** End to end, from an empty file to a passing test: define the schema (with its `Id` field and roles), build the batch with `RecordBatchBuilder`, fill `ChunkOrigin` so results are retrievable, return `ExtValue::RecordChunk`, then register with `register_command!` — `context` last, `async fn` taking owned `State` — and regenerate `command_registry.yaml` |
| **Second walkthrough: a manifest source** | The CSV-directory case: one query per file, what `uniform_schema` to declare, and why a manifest is preferred over a generator (rewindable, cacheable, checkpointable) |
| **Choosing a batch size** | Rows vs bytes, and the memory arithmetic |
| **Using a batch as a DataFrame** | `select`/`filter`/`slice`/`concat` with masks; what is deliberately absent and where it lives instead |
| **Handing a batch to pandas or polars** | The polars path via `polars-arrow`; the pyo3 path; what "zero-copy" does and does not cover |
| **Reading a chunk from JavaScript** | The handle, the view-refresh rule, and when to reach for `columnCopy` |
| **Pitfalls** | The `len + 1` offsets invariant; `Vector` needing a child node; forgetting the `TypeInfo` entry (the type then cannot be stored); forgetting a `#[cfg(feature = "records")]` match arm (a build with the feature off fails); holding a JS view across a wasm call |
| **Testing** | Unit tests beside the code; the round-trip test per format; the `debug-handles` release assertion; the build-matrix rows |

Every snippet is taken from a real test in the implementation, so the guide cannot drift from
behaviour without a test failing.

### Existing documents to update

| Path | Change |
|---|---|
| `specs/reference/VALUE_TYPE_SYSTEM.md` | The `RecordChunk` and `RecordSource` identifiers, their `TypeInfo`s, and that both are feature-gated |
| `specs/guides/TYPE_SYSTEM_GUIDE.md` | Both variants in the worked list; the gated-variant case as a worked example, since it is the first optional value type after `polars` |
| `specs/guides/COMMAND_REGISTRATION_GUIDE.md` | A pointer to the record-producing walkthrough rather than a duplicate of it |
| `specs/README.md` | The capability-map entry, `designing` → `built` |
| `specs/guides/LANGUAGE-INTEGRATION_GUIDE.md` | **Already updated** — VALUE's third bridging category (lent buffers) and RECIPE's corrected listing/containment rule |
| `CLAUDE.md` | The `records` feature in the feature-matrix section, and the new matrix rows |

**Discarded candidates:** `STORE_SEMANTICS.md`, `STORE_IMPLEMENTATION_GUIDE.md` and
`CONFORMANCE_TERMS.md` — this design touches no store trait. `ASSETS.md` — the `get_asset_info`
repair belongs to the search design.

`affects_docs`: `reference/RECORD_STREAMS.md`, `guides/RECORD_STREAM_GUIDE.md`,
`reference/VALUE_TYPE_SYSTEM.md`, `guides/TYPE_SYSTEM_GUIDE.md`,
`guides/COMMAND_REGISTRATION_GUIDE.md`, `guides/LANGUAGE-INTEGRATION_GUIDE.md`.

## Relevant Commands

Deliberately thin: this design owns the *mechanism*, and each consumer brings its own producers.

| Command | Signature | Purpose |
|---|---|---|
| `rec_id` | `fn rec_id(state, id: String) -> result` | **The single-record selector.** Yields the one record whose `Id` field matches. Universal, because every schema has exactly one `Id` |
| `records_to_csv` | `fn records_to_csv(state) -> result` | Serialize a record set as CSV |
| `records_to_ndjson` | `fn records_to_ndjson(state) -> result` | Serialize a record set as NDJSON |
| `records_schema` | `fn records_schema(state) -> result` | The schema as a value — how an agent discovers field names |
| `records_head` | `fn records_head(state, n: i64 = 20) -> result` | Slice, for inspection |

Namespace `rec`, written `ns-rec` in a query. Producers (`ns-search/records`, a CSV projection, a
parquet projection) are owned by the designs that need them.

**`rec_id` is deliberately in the record namespace rather than a projection's.** It works on any
record stream, so a projection does not have to supply its own selector — and a projection that can
do better supplies a `locator` instead of a competing command.

## Error Handling

All errors are `liquers_core::error::Error` via typed constructors. No `Error::new`, no new error
type, no `unwrap`/`expect`.

| Situation | Outcome |
|---|---|
| A schema without exactly one `Id` field | `Error::general_error` from `RecordSchema::new` |
| An `Id` field that is not `Exact`-indexed and stored | `Error::general_error` — it could not be reconciled by delete-by-term |
| Column count or length disagrees with the schema or `len` | `Error::general_error` |
| `concat` of batches with different schemas | `Error::general_error` naming the first differing field |
| A mask whose length differs from the batch | `Error::general_error` |
| State is not an `ExtValue::RecordChunk` or `ExtValue::RecordSource` | `Error::conversion_error` |
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
`RecordSource`. A `RecordSource` serializes as a **manifest**, whose format —
`<filename_prefix>.manifest.yaml`, mirroring `recipes.yaml`'s `arguments` and `links` — is specified
in [`manifest-format.md`](./manifest-format.md). Three of its rules bear on this design:

- **Identity has two regimes.** For an *unkeyed* stream the query is the chunk's identity, so a value
  varying per chunk must live in the query. For a *keyed* stream — one with a `ChunkKeys` — the
  chunk's key distinguishes it, so per-chunk `arguments` and `links` are usable, exactly as in
  `recipes.yaml`. A manifest using them without a cache is **invalid**, because the failure is
  silent aliasing rather than an error.
- **The explicit form is a `RecipeList`.** A chunk entry is a `Recipe` field for field, so a manifest
  is a stream header plus a recipe list — inheriting planning, arguments, links, `volatile` and
  `expires` rather than restating them.
- **No string interpolation.** A command hydrates its own statement, keeping the format free of
  templating syntax and of an injection story. `expires` bounds each chunk, and the dependency
  cascade (`dependencies.rs`) expires whatever derives from it.

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
- `liquers-core` gains no dependency and **no `unsafe`**; its only change is two ungated type aliases
  in `maybe_send.rs`.
- Every exhaustive `match` on `ExtValue` has a `#[cfg(feature = "records")]` arm, so
  `--no-default-features` still compiles — the failure mode a gated enum variant causes, and what the
  new build-matrix rows exist to catch.
- `bytemuck` is `optional = true` and reached only through `records`, so a build without the feature
  resolves an unchanged dependency graph.
- `Buffer<T>: bytemuck::Pod` holds for `i32`, `i64`, `u64`, `f32`, `f64`.
- The `records` feature gates the module, the variants, the matches and the dependency together —
  a partially-gated feature is the classic way to break a configuration nobody built.

## References to liquers-patterns.md

Async-by-default with `#[async_trait]`; `MaybeSend`/`MaybeSync` for wasm; typed error constructors;
explicit match arms with no default; `Arc` for shared payloads in `Value`; `TypeInfo` registration
as the fourth step of adding a value type; `context` last in a command signature.

## Open Questions for Phase 3

0. **Should record selection be a *view*?** `rec_id` as specified is eager: it consumes the stream
   and yields one record, so selecting row 42 of a billion-row source produces every chunk to find
   it. A **view** would instead carry the selection as a predicate the source may push down — to the
   one chunk whose id range contains 42, or to a SQL `WHERE`, or to nothing at all if the source
   cannot help.

   The machinery for this is already half-designed elsewhere and should not be reinvented:

   - The search design's `SearchPredicate` is exactly "a selection carried rather than applied", and
     `rec_id` is its simplest case — an equality on the `Id` field.
   - The three-state pushdown adopted in `interoperability-layer.md` — **exact / inexact /
     unsupported** — is the right report for what a source did with a pushed selection, because an
     `Inexact` narrowing still needs the caller to re-check.
   - `ChunkOrigin::locator` is the per-projection fast path when a source *can* answer directly.

   So the shape is visible, and three things are not: whether a view is a distinct value form or a
   `RecordSource` carrying a predicate; whether pushdown is attempted for `rec_id` alone or for any
   predicate (which would make this the record-level half of the search design); and what a view
   costs when nothing can be pushed down, which is the eager behaviour plus the indirection.

   **This is a design task, not a Phase 3 question**, and it should not hold up Phase 4: `rec_id`
   eager is correct, just not always cheap, and a view can replace its implementation without
   changing the query that names a record.

1. **Can a `RecordSource` front a relational database?** `engine-survey.md` §3 finds the read path
   fits well — sqlx's `fetch()` is already a row stream, and keyset chunking works *because* the
   schema already requires exactly one ordered unique `Id`. Three things do not fit, and the first
   is structural:
   - A SQL source needs chunk queries **generated**, not listed, since the count is unknown and
     `COUNT(*)` is expensive. [`chunking-and-resumability.md`](./chunking-and-resumability.md) works
     this through: the `template` and `keys` fields of `SourceBacking::Queried` are reserved for it,
     and `ChunkList::Unbounded` exists so consumers are written for it now. **No trait revival is
     needed** — but the reserved fields must be filled in, and store-backed chunks with them.
   - **`Decimal` stops being deferrable.** Reading a `NUMERIC` column as `Float` is a corruption bug,
     not an approximation. Also absent: `uuid`, `jsonb`, arrays, intervals.
   - **Writing is entirely undesigned.** An access layer implies `INSERT`/`UPDATE`, transactions and
     conflict handling; this design is read-only throughout.

   Filed as `NO-RELATIONAL-DATABASE-ACCESS-LAYER` rather than absorbed here.
2. What is the default batch size, and is it a row count or a byte budget? A byte budget is the
   honest answer for the multi-gigabyte case but needs a size estimate per column.
3. Is `with_columns` the right extension point for derived fields? It now has two users: the search
   design's evidence columns, and the derived columns that replace functional indexes.
4. ~~Cleanup of stored chunks~~ — **answered** by the folder convention: a stream's chunks live in
   one folder, so removing the folder removes the stream. See
   [`chunking-and-resumability.md`](./chunking-and-resumability.md) §4a.
5. Diagnostics are `Metadata` log entries rather than a structured field, so a list of unavailable
   fields loses its structure. Enough for a UI that wants "did you mean…"?
6. Does the 64-node cap implied by a `UInt` evidence bitmask belong here or in the search design?
7. **Declared projection depths.** OpenViking gives every entry three loading tiers — a one-sentence
   abstract, an overview, then full detail. That is a better articulation of "return enough to judge
   a record and to address it" than the Level 0 / Level 1 split in `record-model.md`. Should a source
   declare depths a consumer can request?
8. **A per-record content hash.** mem0 carries one for deduplication. This design versions a *chunk*;
   a per-record hash would let reconciliation skip unchanged records inside a changed chunk. Worth it
   only if chunk granularity proves too coarse in measurement.
9. Should `ChunkDescriptor` carry **statistics** (row count, per-column min/max)? DataFusion uses them
   for optimization, and min/max per chunk would let a predicate skip chunks entirely — the same trick
   as parquet row-group pruning. Not needed now; the place for it later is clear.

**Settled, and recorded so they are not reopened without new information:**

| Question | Answer |
|---|---|
| Which crate owns records | `liquers-lib`, behind a `records` feature — a data type, not a core capability |
| Which enum owns the value variants | `ExtValue`, because `Value`'s `Deserialize` bound is unsatisfiable for a stream |
| Whether a `ChunkedRecordSource` trait is needed | Not for the view direction — `Manifest` is a partition as data. **Reopened by question 1** for the relational direction |
| Whether a `RecordSet` type survives | No — a batch or a source, not a third name |
| How far the DataFrame surface goes | `select`/`filter`/`slice`/`concat`; group-by and join are a query engine |
| Whether a stream can be rewound | Not applicable — re-open the source instead |
| Whether roles can differ per engine | No per-engine maps. A field declares the access paths it **affords**; each engine projects |
| Whether functional indexes (soundex) are roles | No — they are derived columns |

## Changelog

The design reached this shape through seven rounds of review. Recorded because the *reversals* carry
information — each is a position that was argued for and then abandoned on evidence.

| Date | Change | Driven by |
|---|---|---|
| 2026-09-19 | **Split out of `store-and-asset-search`.** Six architecture revisions there established that a search is a predicate over a record stream, and that the stream serves four consumers of which search is one. Records stabilize first | The search design outgrowing its own subject |
| 2026-09-19 | `RecordSet` and `Diagnostics` **removed**. A bounded sequence of batches is a finite stream or a concatenated batch, not a third name; `scanned` and unresolvable field names are evaluation facts, and `Metadata`'s `LogEntry` already owns those | A conformity review finding `RecordSet` contradicted the value variants |
| 2026-09-19 | `ChunkedRecordSource` **retired before implementation** | `Manifest(Vec<Query>)` is a partition as data — storable, cacheable, diffable, which a trait object is not |
| 2026-09-19 | Value variants moved from core's `Value` to `ExtValue` | `Value` derives `Deserialize`, which a stream cannot satisfy **in principle** |
| 2026-09-19 | Search evidence became **columns** rather than a parallel match vector | A record set carrying a field only search fills is a record set that knows what a clause is |
| 2026-09-20 | **Three abstractions** — source, stream, chunk — replacing two forms and a backing enum. `rewind()` deleted | `Iterable` vs `Iterator`; a fallible rewind whose success depended on construction |
| 2026-09-20 | Arrow compatibility **restated**: data crosses as `&[T]`, structure is rebuilt as `repr(C)`. A whole chunk *can* be shared, at O(columns) small allocations | "Rust does not define struct layout" — a fair objection the earlier text hid |
| 2026-09-20 | wasm sharing **specified** rather than dismissed as fragile: two hazards, one detectable in O(1) and one prevented by ownership | Asking whether invalidation can be detected — it can |
| 2026-09-20 | Whole feature moved to **`liquers-lib` behind a `records` feature**; `bytemuck` became optional | Records are a data type, not an essential capability |
| 2026-09-20 | `FieldRole` became **composable intent** plus a separate `KeyRole`; the id's indexing is now enforced | `TEXT \| STORED` was inexpressible as an enum, and delete-by-term constrains the id |
| 2026-09-20 | **HTTP streaming** in `liquers-axum`, ad-hoc, with the general mechanism deferred | A chunked design undone by materializing at the last step |

| 2026-09-20 | `indexed` became **plural**, and roles were reframed as **capabilities the data affords** rather than instructions | One field routinely has several access paths in one engine — a B-tree and a trigram index; a `text` field with a `keyword` sub-field |
| 2026-09-20 | `IndexKind::Substring` added; functional indexes declared **out of scope as roles** | `LIKE`/trigram is neither exact nor tokenized. Soundex indexes an expression, so it becomes a derived column instead |
| 2026-09-20 | The sink report became **three-valued** — exact / inexact / unsupported | DataFusion's filter pushdown: "narrowed but you must re-check" is a state two outcomes cannot express, and it is filter-then-verify |

| 2026-09-20 | `chunks()` returns **`ChunkList { Known, Unbounded }`** rather than a complete `Vec` | A SQL source's chunk count is unknown and `COUNT(*)` is expensive. A consumer written against a complete `Vec` assumes enumeration, and retrofitting touches reconciliation |
| 2026-09-20 | `SourceBacking` became `Materialized` / `Queried { chunks, cache }` / `QueriedTemplated { template, first_offset, step, cache }`, the last two reserved | A SQL source's chunk count is unknown, so chunk queries must be *generated*. **The manifest is a persisted format**, so its shape must anticipate or stored manifests break. `ChunkKeys` puts a stream's chunks in one folder, which makes them keyed assets and makes cleanup possible |

| 2026-09-20 | Audit after two silent deletions: `RecordSchema` and `FieldType` restored, `ChunkId` defined, `ChunkDescriptor` made reachable through `describe_chunk` | Mechanical rewrites had dropped definitions while the rest of the document kept referencing them |

**Corrections worth keeping visible**, because each was stated wrongly first:

- `bytemuck` was described as free because it is in `Cargo.lock`. It is a direct dependency of no
  workspace crate, arriving transitively via `egui`.
- "Zero-copy" was applied to the whole Arrow export. It is true of the data and false of the
  description, which costs O(columns) allocations.
- The browser route was called fragile without checking whether invalidation is detectable.
- `Value::Recipe` was cited as having 17 occurrences; it has 27, 8 of them in `value.rs`.

**Open external checks:** Arrow format strings against [CDATA], and Tantivy/Lucene/Qdrant API names
against the crate versions in use. Both are stated from the specifications rather than verified in
this repository, and both fail loudly at the boundary if wrong.
