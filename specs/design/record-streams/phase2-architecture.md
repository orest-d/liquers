# Phase 2: Solution & Architecture — Record streams

**Status:** Phase 1 approved 2026-09-20. Phase 2 approved 2026-09-20, revised 2026-09-24/25, and
**re-approved 2026-09-25**. A dated changelog is at the end.

## Overview

A record stream is a **lazy sequence of tables**, each carrying a schema that names its fields and
declares what an index should do with them, and each attributable to a **chunk** that owns its
provenance and validity. Tables at rest are **columnar batches laid out as Arrow specifies**.

The feature is its own crate, **`liquers-records`**, depending on `liquers-core` alone:
`liquers-core ← liquers-records ← liquers-lib`. It holds the data model, the traits, the views, the
formats and the manifest. `liquers-lib` adds only the **glue** — the `ExtValue` variants, the `ns-rec`
commands, the conversions from arbitrary inputs, and the polars bridge — behind its `records` feature.
`liquers-core` gains only two **general** features that keyed chunks need — a recipe provider chain,
and `stored`/`cached` flags the asset manager honours (§"Keyed chunks") — and nothing is added to
`AsyncStore`. With `records` off in `liquers-lib`, the records crate is not built, and the two core
features behave exactly as today by default.

Three **traits** carry it — a **`RecordSource`** that can be asked repeatedly for a stream, a
**`RecordStream`** that is one traversal, and a **`RecordView`**, any finite table with random
access — plus one struct under them, **`RecordBatch`**, the materialized table. Views make
projections, filters, a pointer at one cell, derived columns and generated tables cheap to write;
the batch holds data, exports to Arrow, and is what fast code works against. Two of the traits are
values (`ExtValue::RecordView` and `ExtValue::RecordSource`, each holding a trait object); the stream
deliberately is not.

The five requirements of [Phase 1](./phase1-high-level-design.md) map onto the architecture as
follows:

| Requirement | Mechanism |
|---|---|
| Arrow interoperability without a heavy dependency | Buffers in Arrow's layout; export via the C Data Interface, IPC bytes, or typed-array views in the browser — none of it requiring `arrow-rs` |
| A `Value` variant | `ExtValue::RecordView` and `ExtValue::RecordSource`, trait objects, each with its `TypeInfo`, both feature-gated. A single-cell view reads as a scalar |
| A lightweight DataFrame without polars | Views over columns, masks and typed kernels — `select_columns`/`filter`/`slice`/`with_column`/`concat` — usable from `liquers-web`, which cannot bundle polars |
| Multi-gigabyte lazy processing | `RecordStream`, a `futures::Stream` of views; the chunk is the refresh unit, the batch the memory unit, and only one batch is resident |
| Per-chunk provenance and validity, flyweighted to the record | `ChunkDescriptor` carries a `Metadata`; a record's provenance is its chunk's |

## Known-Issue Preflight

Searched `specs/index.csv` for non-terminal `issue`/`feature` records whose `area` intersects
`core/value`, `core/commands`, `core/context`, `core/query`, `lib/value`, `web`. Re-run 2026-09-24
for the trait revision.

| Issue | Status | Pri | Relevance and solution impact | First? | Blocking? | Required action | Priority action |
|---|---|---|---|---|---|---|---|
| `NO-RECORD-STREAM-ABSTRACTION` | draft | P2 | This design *is* its resolution | n/a | no | Close in Phase 5 | keep P2 |
| `CORE-VALUE-ENUM-OVERSIZED` | draft | P2 | `Value` is 704 bytes. Drove fields to `FieldValue` and the new variant behind `Arc` | no | no | Honoured throughout | keep P2 |
| `VALUE-SERIALIZATION-IS-SYNCHRONOUS-AND-WHOLE-VALUE` | draft | P2 | Filed 2026-09-24 from this design's HTTP section. `liquers-axum` cannot name `ExtValue`, so streaming a source over HTTP needs a core-level asynchronous serialization hook rather than an axum branch | no | **yes, for streaming large exports only** | Design separately. Meanwhile a source's rows are serialized by `ns-rec/materialize`, bounded by `max_rows` | keep P2 |
| `METADATA-ONLY-ENTRY-RELOADS-AS-CORRUPTED` | draft | P3 | Every non-manifest source is stored as metadata only; reloading one goes through the fast-track's corrupted-data branch before re-deriving. Found while checking this design | no | no | Accept — the outcome is correct; the log is misleading | keep P3 |
| `TYPE-INFO-CANNOT-DECLARE-WRITE-ONLY-FORMATS` | draft | P3 | HTML is written but cannot be read back, and `supported_data_formats` means "written and read". Found while specifying table formats | no | no | Declare `html` anyway; a stored `.html` table is recomputed | keep P3 |
| `MEDIA-TYPES-MISSING-FOR-TABULAR-FORMATS` | draft | P3 | `ndjson` has no media type; Arrow and Parquet are served as `application/octet-stream`. Found while specifying table formats | no | no | Fix alongside, or accept octet-stream until then | keep P3 |
| `VALUE-SERIALIZATION-HAS-NO-INCREMENTAL-WRITER` | draft | P2 | A multi-gigabyte record stream cannot be serialized through a `Vec<u8>`-returning writer. **The clearest motivating case yet filed for it**, now with a concrete consumer in `liquers-axum` | no | no | **Ad-hoc streaming in `liquers-axum` for this design**; issue updated with the HTTP motivation and the push-vs-pull finding | consider P1 when a second value type needs it |
| `CORE-METADATA-NO-APPLICATION-ATTRIBUTES` | draft | P2 | Front-matter fields have nowhere to live, so `attr.`-qualified columns have no source. **Also where a stored table's schema would live**, making CSV written by Liquers lossless without a manifest | no | no | Field qualification and the schema-aware reader are both designed to accept it as a pure upgrade | keep P2 |
| `SIMPLE-VALUE-CANNOT-READ-JSON` | draft | P2 | `liquers-lib`'s base value writes `json` but cannot read it, so a stored JSON document cannot feed `from_json`. Found while designing JSON shapes | no | only `from_json` on a stored document | Fix before or with Phase 4; small | keep P2 |
| `COMMAND-CONTEXT-PARAM-ORDER` | accepted | P2 | `context` must be last in record-producing commands | no | no | Honoured | keep P2 |
| `CORE-SYNC-STORE-TRAIT-OBSOLETE` | — | — | Record sources read through `AsyncStore` only | no | no | — | — |
| `EXTENDED-VALUES-CANNOT-BIND-TO-SCALAR-ARGUMENTS` | draft | P2 | A single-cell view must bind to a scalar argument through a recipe `links:` entry. A resolved link binds through `TryFrom<Value>`, and every scalar `TryFrom` refuses an extended value — `String` included. Found while designing scalar reading | **yes** | only that one use | **Fix as the first Phase 4 step**: `ValueExtension` hooks for every scalar conversion, delegated from both paths | keep P2 |
| `RECORD-SELECTION-IS-EAGER-NOT-A-VIEW` | draft | P2 | **Largely resolved here.** A view is an implementation of `RecordView`, not a distinct value form. Composition and pushdown are declared out of scope for the generic mechanism | n/a | no | Update the issue: views designed; pushdown left to specialized sources | lower to P3 once this design is approved |
| `VALUE-CONVERSION-CAPABILITY` | draft | P2 | Scalar reading of a view delegates to the base `Value`, so it inherits that value's lossy `i64 → f64` rather than choosing its own rule | no | no | None here — the question is that issue's | keep P2 |
| `ASSETS-CANNOT-BE-DECLARED-NON-PERSISTENT` | draft | P2 | Keyed chunks need `stored`/`cached` honoured by the asset manager | yes | yes, for keyed chunks | **Resolved in this project** — §"Keyed chunks" C | keep P2 |
| `NO-RECIPE-PROVIDER-CHAIN` | draft | P3 | Keyed chunks are served by a manifest provider composed with `recipes.yaml`'s | yes | yes, for keyed chunks | **Resolved in this project** — §"Keyed chunks" B | raise to P2 |
| `RECIPE-CONTAINS-DEFAULT-ASSUMES-ENUMERABILITY` | draft | P2 | A template is unbounded, so the manifest provider must override `contains` | no | no | **Resolved for the manifest provider**; the default stays | keep P2 |
| `COMMAND-CACHE-FLAG-IS-DECLARED-BUT-NEVER-READ` | draft | P3 | Stream commands rely on `volatile`, which **is** wired; `cache` is a knob that does nothing. Found while answering Phase 1 | no | no | Monitor — `volatile` covers this design's need | keep P3 |

**No blocker for the design.** `EXTENDED-VALUES-CANNOT-BIND-TO-SCALAR-ARGUMENTS` blocks one use — a
single-cell view as a linked scalar argument — and is small; it is the first step of Phase 4.

## Three abstractions: source, stream, view

Three **interfaces**, each a trait, so that a view, an on-the-fly filter or a generated table is a
new *implementation* rather than a new data structure. The distinction between them is still
`Iterable` vs `Iterator` — in Rust, `IntoIterator` vs `Iterator` — plus random access:

| | Role | Shareable | Serializable | Consumed by use | Access |
|---|---|---|---|---|---|
| **`RecordSource`** | can be asked, repeatedly, for a stream | **yes** | **only as a manifest** — its rows need `materialize` first | no | **async** |
| **`RecordStream`** | one traversal, in flight | **no** — may be partly consumed | not itself; its *data* can be drained to a table | **yes** | **async** |
| **`RecordView`** | a finite table with random access | **yes** | **yes** — materialized on the way out | no | **sync** |

Under them sits one concrete struct:

**`RecordBatch`** — the **materialized** table, in Arrow's memory layout. It implements
`RecordView`, it is what most streams yield, and it is the form that exports to Arrow as a whole and
that high-performance code works against. Views are for flexibility — projections, row selections,
pointing at a cell, derived columns, generated tables; the batch is for data at rest and for speed.

Conversions, with their costs:

| From → to | How | Cost |
|---|---|---|
| `RecordBatch` → `RecordView` | it is one: `Arc<RecordBatch>` coerces to `Arc<dyn RecordView>` | free |
| `RecordView` → `RecordBatch` | `materialize()` | a shallow clone for a batch; `Arc`s for a projection or row range over a batch; a copy of the selected rows for a filter; computation for a generator — §"Views" |
| `RecordView` → `RecordSource` | `InMemorySource::new(vec![view])` | free |
| `RecordSource` → `RecordStream` | `source.stream(resolver).await` | opening the first chunk |
| `RecordSource` → `RecordView` | `source.materialize(resolver, max_rows).await`, or the `ns-rec/materialize` command | the whole source in memory; **refused past `max_rows`** |
| `RecordStream` → `RecordView` | `stream.materialize(max_rows).await` — drain and concatenate | the same, for a traversal already open |

A stream does **not** become a source: the information is gone.

### Why traits

As data structures — a `RecordSource` struct over a closed backing enum, and `RecordBatch` as the only
table — a view, a filter applied on the fly, or a table computed by a closure would each be a new
variant of an enum inside this module, rather than something a command author can write. As traits
each is one `impl`: this design owns the interfaces and a small set of reference implementations, and
anyone may add more.

What the data-structure form would offer is kept:

- **The partition as data.** `chunks()` still returns ids, and `ManifestSource` — the implementation
  a manifest deserializes to — is a plain `serde` struct that can be stored, cached and diffed.
- **Exhaustive matching.** What would be a closed backing enum is two implementations,
  `ManifestSource` and `InMemorySource`, and no code matches over implementations — so `CLAUDE.md`'s
  rule against default match arms has nothing to bite on.
- **Serialization.** A derive on each concrete type; one trait method at the value boundary.

### Why there is no `rewind`

The obvious alternative — one stream type with a `rewind()` that works for some backings — gives a
fallible method whose success depends on how the value happened to be constructed. That is discovered
at runtime, by a user who did not read the documentation.

With a source separate from its streams there is nothing to rewind. **Hold the source and open
another stream; hold only a stream and you have one pass.** Phase 1 answer 2 asked for "optionally
rewindable, cloneable in some cases", and this delivers it as a property of *which interface you
hold*, checked by the compiler.

### Names

| Name | Kind | Why this name |
|---|---|---|
| `RecordView` | trait, **and** the value variant and its type identifier | Any finite table. It is the interface every table satisfies, so it names the value regardless of how the rows are held |
| `RecordBatch` | struct | Arrow's own name for exactly this structure (arrow-rs, pyarrow), so "export a `RecordBatch`" means the same thing on both sides. And *batch* is already this design's word for the unit of memory. **Not `RecordChunk`**: a chunk is the unit of refresh, and one chunk streams as several batches |
| `RecordSource` | trait, and the second value variant | Unchanged |
| `RecordStream` | trait | Unchanged; never a value |
| `ChunkOrigin` | struct | Per-chunk record of where rows came from — the asset query, the chunk query, an optional `AssetInfo` and an optional locator |

### The types

```rust
// liquers-records/src/lib.rs
use liquers_core::maybe_send::{BoxFuture, MaybeSend, MaybeSync};

/// Something that can be asked, repeatedly, for a stream of views.
/// The `Iterable` of this design: shareable, never consumed by use.
pub trait RecordSource: Debug + MaybeSend + MaybeSync + 'static {
    /// Open a fresh traversal. Callable any number of times — this is what replaces `rewind`.
    /// `Arc<Self>` and an owned resolver make the stream `'static`, which an HTTP body needs:
    /// it outlives the handler that opened it (§"Streaming a record source over HTTP").
    fn stream(self: Arc<Self>, resolver: Arc<dyn ChunkResolver>)
        -> BoxFuture<'static, Result<BoxRecordStream, Error>>;

    /// Chunks without producing any records — the reconciliation primitive. Sync, no I/O.
    fn chunks(&self) -> ChunkList<'_>;

    /// Full provenance for one chunk. Reads metadata, so it is async.
    fn describe_chunk<'a>(&'a self, id: &'a ChunkId, resolver: &'a dyn ChunkResolver)
        -> BoxFuture<'a, Result<ChunkDescriptor, Error>>;

    /// The schema every view will have, when the producer promises one.
    /// Declared, not assumed — §"Schema uniformity is declared".
    fn schema(&self) -> Option<Arc<RecordSchema>> { None }

    /// A producer's report that it stopped early.
    fn truncated(&self) -> bool { false }

    /// The manifest, when this source has one — the **only** byte form a source has.
    /// `Some` for a `ManifestSource`; `None` for every other source, which is then stored as
    /// metadata only and re-derived from its recipe (§"A source serializes only as its manifest").
    fn manifest(&self) -> Option<&ManifestSpec> { None }

    /// Every row, as one table: open a stream and drain it. Refused past `max_rows`, and when
    /// the chunks' schemas differ. Provided; a source that can do better — one batch already in
    /// memory, a database that returns a result set whole — overrides it.
    fn materialize(self: Arc<Self>, resolver: Arc<dyn ChunkResolver>, max_rows: usize)
        -> BoxFuture<'static, Result<Arc<RecordBatch>, Error>> { /* stream(), then materialize */ }
}

/// One traversal: a `futures::Stream` of views, plus the one thing a consumer needs **before**
/// the first item — the schema, when promised. A CSV header needs it, and so does the Arrow C
/// Stream Interface, whose `get_schema` is called before any array is pulled.
pub trait RecordStream:
    Stream<Item = Result<Arc<dyn RecordView>, Error>> + MaybeSend + 'static
{
    fn schema(&self) -> Option<Arc<RecordSchema>>;
}

/// No per-target alias is needed: `MaybeSend` is a **supertrait**, so `dyn RecordStream` is
/// `Send` on native and not on wasm — the same transitivity `ForeignValue` already relies on
/// (`liquers-lib/src/value/foreign.rs`).
pub type BoxRecordStream = Pin<Box<dyn RecordStream>>;

/// Gives any stream of views the `RecordStream` interface — how a combinator chain
/// (`map`, `filter_map`, `then`) becomes a record stream again. `S: Unpin` is met by
/// `Box::pin(stream)`.
pub fn record_stream<S>(inner: S, schema: Option<Arc<RecordSchema>>) -> BoxRecordStream
where
    S: Stream<Item = Result<Arc<dyn RecordView>, Error>> + Unpin + MaybeSend + 'static;

/// `materialize` for a traversal already open. An extension trait rather than a method of
/// `RecordStream`, because draining consumes the stream and it is held as a boxed trait object.
/// `max_rows` is a limit rather than a promise: a source whose chunk count is unknown cannot
/// say in advance whether it is small.
pub trait RecordStreamExt {
    fn materialize(self, max_rows: usize) -> BoxFuture<'static, Result<Arc<RecordBatch>, Error>>;
}
impl RecordStreamExt for BoxRecordStream { /* … */ }
```

`RecordView` is defined in §"Views" below, with its implementations.

The two reference sources:

```rust
/// Chunks named by queries — what a manifest deserializes to, and the only source whose byte
/// form is a manifest. Serialized through `ManifestSpec`, the plain file shape: `try_from`
/// is where load-time validation runs (name collisions among explicit chunks), and where
/// the chunk ids `chunks()` lends out are derived — a `&[ChunkId]` cannot be borrowed from the
/// `Vec<Query>` the file holds.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(try_from = "ManifestSpec", into = "ManifestSpec")]
pub struct ManifestSource {
    spec: ManifestSpec,
    /// Where the manifest lives — its folder is the `cwd` and holds the chunk keys. `None` until
    /// known: `deserialize_from_bytes` receives no metadata, so a manifest read back from the
    /// store arrives without it and the glue supplies it (`with_key`) from the state's metadata.
    /// A manifest built by a command, never stored, stays `None`: its chunks are unkeyed.
    key: Option<Key>,
    /// Derived at construction: each explicit chunk's id, and the naming of template chunks.
    ids: Vec<ChunkId>,
    naming: Option<ChunkNaming>,
}

/// The manifest file. The explicit list and the template **combine**: `chunks` are the
/// stream's first chunks in order, and `template` produces everything after them, rendered at
/// the **global** chunk index. Either may be absent. See `manifest-format.md` §4a.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ManifestSpec {
    /// The discriminator, always written. `to_record_source` recognizes a plain YAML or JSON
    /// document by it; here, absent is accepted (a bare spec) and any other value is refused.
    #[serde(default)]
    pub manifest: ManifestKind,
    /// Absent or unknown reads as the latest (`manifest-format.md` §8.3); written as the latest.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<u32>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub title: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    /// Shared by every **keyed** chunk: merged under each explicit chunk's own `arguments` (the
    /// chunk wins) and given as-is to every template chunk. As with per-chunk arguments, a
    /// manifest that has shared `arguments` or `links` and an unkeyed chunk is refused by
    /// `with_key` / at stream open — an unkeyed chunk is a bare query and cannot carry them.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub arguments: HashMap<String, serde_json::Value>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub links: HashMap<String, String>,
    /// Copied onto every chunk recipe by `ManifestRecipeProvider`, as `recipes.yaml` fields are.
    #[serde(default)]
    pub volatile: bool,
    #[serde(default)]
    pub expires: Expires,
    /// The explicit prefix, as recipes — `recipes.yaml`'s own entry type, so a chunk carries its
    /// own `arguments`, `links`, `title`. A chunk whose query ends in a filename is **keyed** by
    /// it, exactly as a `recipes.yaml` entry is (`Recipe::filename`).
    #[serde(default)]
    pub chunks: Vec<Recipe>,
    /// The rule for chunks beyond the prefix. `None` — `chunks` is the whole stream, and
    /// `chunks()` returns `Known`. `Some` — the count is unknown and it returns `Unbounded`; a
    /// walk ends at the first chunk shorter than the template's `batch_size`.
    pub template: Option<ChunkTemplate>,
    /// The extension of template-generated chunk keys, which is also their stored format.
    /// Default `csv`.
    #[serde(default)]
    pub extension: Option<String>,
    /// Whether keyed chunks are written to the store. Default `true`.
    #[serde(default = "true_default")]
    pub stored: bool,
    /// Whether keyed chunks are registered with the asset manager for reuse. Default `true`.
    /// Both `false` means "not kept, and **not volatile**" (`manifest-format.md` §4b).
    #[serde(default = "true_default")]
    pub cached: bool,
    pub uniform_schema: Option<Arc<RecordSchema>>,
}

/// The only kind this design defines; a folder may hold other YAML documents.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum ManifestKind {
    #[default]
    #[serde(rename = "record-stream")]
    RecordStream,
}

/// Written by hand, not derived: `stored` and `cached` default to `true`, and a derived
/// `Default` would make them `false` (the pitfall §C records for `Recipe`).
impl Default for ManifestSpec { /* manifest: RecordStream, stored: true, cached: true, the rest empty */ }

/// Views already in memory — what a view becomes when a source is wanted. It has no byte
/// form: `materialize` it (free when it holds one batch) to serialize its rows.
#[derive(Debug, Clone)]
pub struct InMemorySource {
    views: Vec<Arc<dyn RecordView>>,
    ids: Vec<ChunkId>,
    uniform_schema: Option<Arc<RecordSchema>>,
}

/// How **template-generated** chunks are named: chunk *n* of `<folder>/<prefix>.manifest.yaml` is
/// the key `<folder>/<prefix>_{n:04}.<extension>`. Derived, never written: the prefix is the
/// manifest's own file name, `_{n:04}` is the convention (configurable later), and `extension`
/// comes from the manifest, default `csv`. Explicit chunks name themselves (their query's
/// filename). The folder is what makes chunks addressable (`-R/data/sales/daily_0042.csv`),
/// listable, and removable together.
#[derive(Debug, Clone, PartialEq)]
pub struct ChunkNaming {
    pub folder: Key,
    pub prefix: String,      // "daily", from daily.manifest.yaml
    pub extension: String,   // "csv" unless the manifest says otherwise
}
impl ChunkNaming {
    pub fn key(&self, n: u64) -> Key;          // <folder>/<prefix>_{n:04}.<extension>
    /// The inverse, for the recipe provider: `Some(n)` when `name` is this naming's chunk n.
    pub fn index_of(&self, name: &str) -> Option<u64>;
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
```

### A source serializes only as its manifest; its rows need `materialize`

**The rule.** A `RecordSource` has one byte form, its **manifest**, and only a `ManifestSource` has
one. Every other source — in memory, filtering, mapping — has none. To get a source's **rows** as
bytes, materialize it into a `RecordView` and serialize the view:

```
-R/data/sales/daily.manifest.yaml/-/ns-rec/materialize/daily.csv
```

`materialize` reads the chunks — asynchronously, inside a command — and returns a `RecordBatch`; the
trailing `daily.csv` selects the format, and the batch serializes synchronously through the ordinary
path. The same holds for a stream, which is never a value: `stream.materialize(max_rows)` in Rust.

**Why this and not asynchronous serialization.** Liquers has two stages of different nature.
*Evaluation* is asynchronous — commands await, open streams, evaluate queries. *Serialization* is
synchronous — `DefaultValueSerializer::as_bytes` returns a `Vec<u8>`. Producing a source's rows needs
awaits, so it cannot happen in the second stage. Making serialization asynchronous is a change across
core, the asset manager, the store and `liquers-axum`
(`VALUE-SERIALIZATION-IS-SYNCHRONOUS-AND-WHOLE-VALUE`). Moving the await into the first stage, as a
command, needs none of it: that stage already knows how to await.

| What it gets right | |
|---|---|
| **No change to serialization, the store or `liquers-axum`** | `/q/…/ns-rec/materialize/daily.csv` is served by the existing `BinaryResponse` path, and stored under a key by the existing persistence path |
| **The cost is written in the query** | Materializing is a step a person writes, like `collect()` in a lazy DataFrame API — never something serialization does behind the caller's back |
| **The result is an ordinary asset** | Cached, stored when keyed, and — read through a `ContextResolver` — dependent on its chunks, so it expires when one does |
| **It settles when a source becomes data on disk** | Only when someone writes `materialize`. The persistence step never turns a manifest into a table on its own, so `stored: false` keeps meaning what it says |
| **One verb at every level** | `RecordView::materialize` (sync), `RecordSource::materialize` and `RecordStreamExt::materialize` (async), and the command — which on a view drops a pinned base or freezes a computed view |

**What it costs, stated:**

1. **Memory.** The whole table is resident, then its encoding, then the asset's cached copy of that
   encoding (`SERIALIZED-BINARY-RETAINED-WITH-NO-DISPOSAL-POLICY`) — about three times the data at
   the moment of serving. Bounded by `max_rows`, **1 000 000 by default** and raised explicitly
   (`materialize-5000000`). There is deliberately no "unlimited" value: an export too large to hold
   waits for streaming serialization rather than taking a server down from an innocent URL.
2. **Latency.** Nothing is sent until every chunk has been read.
3. **A non-uniform source cannot be materialized.** One table has one schema, so `materialize` fails,
   naming the first differing field. NDJSON of a non-uniform source — which a streaming encoder handles
   naturally, each row carrying its own keys — is therefore **not available** on this path; exporting
   chunk by chunk is (`<chunk query>/data.ndjson`).
4. **It does not solve large exports.** It is right for bounded tables; the multi-gigabyte export
   still needs `VALUE-SERIALIZATION-IS-SYNCHRONOUS-AND-WHOLE-VALUE`.

**The query form should outlive the workaround.** A materialization immediately serialized —
`…/ns-rec/materialize/daily.csv` — is precisely the case a streaming encoder can serve without
building the table. Once streaming serialization exists, an implementation may serve that query as a
stream **without changing the URL or its meaning**. Recorded as the intended path, not promised.

**Getting a source from a manifest file.** A hand-written `*.manifest.yaml` loads as a plain YAML
document, not a source: a store infers a *data format* from an extension, never a *type*.
`ns-rec/to_record_source` makes the conversion explicit, recognizing the document by its
`manifest: record-stream` discriminator and taking the folder of its key as the `cwd`
(`manifest-format.md` §3); every record command applies the same conversion to its input
(§"What a record command accepts"), which is why the query above needs no extra step. A manifest
written *by Liquers* carries `type_identifier: RecordSource` and deserializes directly as a
`ManifestSource`. **One dependency:** a type-less YAML document must load as a value at all, and
`liquers-lib`'s base value reads neither YAML nor JSON today — `SIMPLE-VALUE-CANNOT-READ-JSON`, which
covers both, is therefore a prerequisite for hand-written manifests.

### Keyed chunks: naming, a recipe provider, and `stored` / `cached`

A manifest's `stored` and `cached` flags, and per-chunk `arguments` and `links`, all act on **keyed**
chunk assets — an unkeyed chunk is a query result, which is never stored and whose identity is its
query. The asset manager creates a keyed asset only through `get(key)`, which asks the **recipe
provider** for the key's recipe (`assets.rs:3846`); `apply` is ad hoc — never keyed, cached or stored
(`:3817`). So keyed chunks take three pieces, all in this project. Two of them are general
`liquers-core` features, not records features, and are designed as such.

#### A. Chunk keys

| Chunk | Key | Example |
|---|---|---|
| **Explicit**, query ending in a filename | that filename, in the manifest's folder — exactly as a `recipes.yaml` entry is keyed | `query: ns-sql/sql_query-0-1000/orders_eu.csv` → `data/sales/orders_eu.csv` |
| **Explicit**, no filename | none — unkeyed; its identity is its query | `query: ns-sql/sql_query-0-1000` |
| **Template-generated**, chunk *n* | `<prefix>_{n:04}.<extension>` in the manifest's folder — `ChunkNaming` | `daily.manifest.yaml`, *n* = 42 → `data/sales/daily_0042.csv` |

- **The convention applies to template chunks only.** Explicit chunks name themselves, as recipes
  do; a manifest merging tables from several sources gives each its own name.
- **`_{n:04}` is fixed for now** (configurable later); it pads to at least four digits, so chunk
  10 000 is `daily_10000.csv` — correct, but past 9 999 names no longer sort in chunk order.
- **`extension` is configurable, default `csv`**, and it is also the stored format. **`arrow` is the
  better choice once `records-ipc` is on**: a keyed chunk re-read by the asset manager outside the
  manifest is deserialized schema-less, so a CSV chunk comes back with inferred types, while an Arrow
  IPC chunk carries its schema. (Read *through* the manifest, both are parsed with `uniform_schema`.)
- **Per-chunk `arguments` or `links` on an unkeyed explicit chunk are refused** — they would alias
  silently — as soon as the manifest's key is known (`with_key`), and at the latest when a stream
  is opened on a manifest that is still keyless. Not at deserialization: a manifest read back from
  the store arrives without its key (`deserialize_from_bytes` receives no metadata), and refusing it
  there would make a stored manifest with per-chunk arguments unreadable. Template chunks share the template's, by construction.
- **Collisions are refused at load:** two explicit chunks with one filename, an explicit filename
  matching the template's pattern, or a chunk name that a sibling `recipes.yaml` also defines.
- **A manifest with no key** — built by a command rather than stored — has no folder, so all its
  chunks are unkeyed.

#### B. A recipe provider that serves chunk keys from manifests

The manifest *is* a recipe list, so the chunks are served the way `recipes.yaml` entries are — by a
recipe provider — which is what makes `-R/data/sales/daily_0042.csv` evaluable from anywhere, not
only through the source.

**`RecipeProviderChain`**, in `liquers-core/src/recipes.rs` (`NO-RECIPE-PROVIDER-CHAIN`): an
ordered list of `Arc<dyn AsyncRecipeProvider<E>>` that is itself a provider.

| Method | Chain behaviour |
|---|---|
| `recipe_opt(key)` | the first provider returning `Some` |
| `contains(key)` | any provider |
| `has_recipes(folder)` | any provider |
| `assets_with_recipes(folder)` | the union, in provider order, without duplicates |
| `get_asset_info(key)` | from the provider that has the recipe |

The environment's provider becomes a chain: `recipes.yaml` first, then the manifest provider when
`records` is on. `RecipeProviderChoice` is **unchanged** — a choice is data in a configuration
document and cannot name a provider living in another crate. Instead `EnvironmentBuilder` and
`GenericEnvironment` gain `with_appended_recipe_provider`, and `liquers-lib`'s `LibKind` returns
the chain as its default when `records` is on, so an integration adds its own generative provider
the same way (Phase 4 decision 1).

**`ManifestRecipeProvider`**, in `liquers-records/src/provider.rs` — it implements core's
`AsyncRecipeProvider<E>` for any `E`, since it produces recipes and no values, with the same
per-target `async_trait` attributes the trait itself uses (`?Send` on wasm, `recipes.rs:464-465`): for
`recipe_opt(<folder>/<name>)` it reads the folder's `*.manifest.yaml` (parsed manifests cached by key
and stored version) and answers with

- the explicit chunk whose query filename is `name`, or
- template chunk *n* when `ChunkNaming::index_of(name)` gives an *n* at or beyond the explicit
  prefix — the template query rendered at *n*, with the filename appended —

each with `cwd` set to the folder and the manifest's `stored`, `cached`, `expires` and `volatile`
copied onto the recipe.

- **`contains` is overridden**, by the same matching and without enumerating — a template is
  unbounded, which is exactly the case `RECIPE-CONTAINS-DEFAULT-ASSUMES-ENUMERABILITY` records.
- **`assets_with_recipes` lists explicit chunks only.** Generated chunk names are addressable but not
  listed; a stored one appears in a directory listing because the store has it. Listing and `contains`
  therefore disagree on purpose, which the language integration guide's `RECIPE` section already
  allows for a generative provider.

#### C. `stored` and `cached` in the asset manager

Two fields on `Recipe`, `MetadataRecord` and `AssetInfo`, both **meaning `true` when absent** so every
existing recipe, stored metadata record and struct literal keeps today's behaviour
(`ASSETS-CANNOT-BE-DECLARED-NON-PERSISTENT`). They are **`Option<bool>`, read through accessors**,
not `bool`: all three types derive `Default`, and `MetadataRecord::new()` builds from
`..Self::default()` (`metadata.rs:1080-1083`), so a plain `bool` would default to `false` — every
record built that way would silently be "not stored, not cached".

```rust
// on Recipe, MetadataRecord and AssetInfo
#[serde(default, skip_serializing_if = "Option::is_none")]
pub stored: Option<bool>,
#[serde(default, skip_serializing_if = "Option::is_none")]
pub cached: Option<bool>,

pub fn stored(&self) -> bool { self.stored.unwrap_or(true) }
pub fn cached(&self) -> bool { self.cached.unwrap_or(true) }
```

A manifest's own `stored`/`cached` stay plain `bool`s with a `serde` default of `true` — `ManifestSpec`
does not derive `Default` — and the provider writes them into each chunk's recipe as `Some(…)`.

| Flag | Honoured where | Meaning |
|---|---|---|
| `stored: false` | every store write for the key — the metadata saver during evaluation (`AssetData::save_metadata_to_store`, `assets.rs:971`), `save_to_store` (`:2902`); `set_state` / `set_binary` read the supplied metadata's flag | **The produced value is not written, and no metadata-only entry is left either.** Nothing else changes: an existing stored copy is still read, and **preferred to recomputation** — it may be `Override` data, and saving disk (not duplicating a database) is the purpose, not freshness |
| `cached: false` | every site registering a keyed asset on the `get(key)` path — `get_nonvolatile_resource_asset` (`:4884`) and `ImmediateAssetManager::get_resource_asset` (`:6493`) | **The asset is not registered for reuse.** It is evaluated for the request and dropped; a later request evaluates again, or reads the stored copy. `save_to_store`'s "not the registered owner" warning is skipped for it, as for a volatile asset |
| both `false` | — | Evaluated on every request, never kept — and **not volatile**: volatility is contagious (`assets.rs:169`), and an uncached value is not an impure one |

The flags reach the asset from its recipe — here, the manifest provider copies them — **when the
manager creates the keyed asset**, which is where it already resolves the recipe for volatility
(not in `resolve_volatility_before_evaluation`, which runs before the provider's recipe replaces
the ad-hoc key recipe). They are recorded in the stored metadata and reported in `AssetInfo`, so a consumer can see why a key has no
stored data. **Reconciliation of a `stored: false` chunk** reads its version from the cached asset
when `cached` is set, and otherwise re-evaluates — there is no stored metadata to read.

**How the source reads a keyed chunk** follows §"Two readers": if the store holds the key, the bytes
are read (`read_resource`) and parsed with `uniform_schema`; otherwise the key is evaluated through
the asset manager (a pure key query, so the chunk becomes a keyed asset with the manifest's flags) and
its view is checked against the schema.

**The cost, stated.** This puts two core changes into a records project, and every struct literal
building a `Recipe`, `MetadataRecord` or `AssetInfo` — `liquers-py`'s wrappers included — gains two
fields or a `..Default::default()`. Both features are general: the chain is how any generative
provider plugs in (the `stockplottertest` prototype is one), and the flags are how any asset declines
persistence.

### `ChunkResolver` — how a source reaches evaluation

A source backed by queries must evaluate them, and the obvious signature —
`stream(&self, context: &Context<impl Environment>)` — makes the method **generic**, which cannot be
called through `dyn RecordSource`. The environment therefore reaches a source through a small
object-safe trait:

```rust
/// What a source needs from the environment. Object-safe, so `dyn RecordSource` can take it,
/// and free of any concrete value type, which `liquers-records` cannot name.
pub trait ChunkResolver: MaybeSend + MaybeSync + 'static {
    /// Evaluate a chunk query and wait for its value, as the records crate can use it.
    fn evaluate(&self, query: Query) -> BoxFuture<'static, Result<ChunkValue, Error>>;
    /// Read a chunk's metadata without producing its value — the store's metadata for a keyed
    /// chunk, the asset manager's for an unkeyed one.
    fn metadata(&self, query: Query) -> BoxFuture<'static, Result<Metadata, Error>>;
    /// A stored chunk's bytes and metadata, **without** deserializing them — so a manifest with a
    /// declared schema can parse the bytes with it (§"Two readers"). Records the key as a
    /// dependency when the resolver is a `ContextResolver`.
    fn read_resource(&self, key: Key) -> BoxFuture<'static, Result<(Vec<u8>, Metadata), Error>>;
}

/// A chunk's value as the records crate sees it.
pub enum ChunkValue {
    View(Arc<dyn RecordView>),
    Source(Arc<dyn RecordSource>),
    /// Anything else, as bytes in its data format — parsed as a table by the source.
    Bytes { data: Vec<u8>, metadata: Metadata },
}
```

**`liquers-records` cannot name `liquers-lib`'s `Value`**, which sits above it, so it sees values
through an adapter trait that the value type implements — the pattern `ValueExtension` already uses
in the other direction:

```rust
// liquers-records/src/value.rs
/// How the records crate reads and builds a Liquers value without knowing its type.
/// Implemented by `liquers-lib`'s `Value`; an integration with its own value type implements it too.
pub trait RecordValue: ValueInterface {
    fn as_record_view(&self) -> Option<Arc<dyn RecordView>>;
    fn as_record_source(&self) -> Option<Arc<dyn RecordSource>>;
    fn from_record_view(view: Arc<dyn RecordView>) -> Self;
    fn from_record_source(source: Arc<dyn RecordSource>) -> Self;
}
```

`liquers-lib` implements it for `CombinedValue<SimpleValue, ExtValue>` — its own type, so the orphan
rule allows it — mapping the two methods pairs onto the two `ExtValue` variants.

Two resolver implementations, and the difference between them is **dependency recording**:

| Implementation | Wraps | Records dependencies | Used by |
|---|---|---|---|
| `ContextResolver<E>` | a clone of `Context<E>` — `Context::evaluate` (`context.rs:746`) | **yes** — each chunk becomes a dependency of the asset being computed | a command traversing a source inside its own body |
| `EnvResolver<E>` | `EnvRef<E>` — `EnvRef::evaluate` (`context.rs:359`), already a `'static` future | no | the `liquers-axum` handler, which serves a value whose asset is already complete; a language binding |

A stream that outlives the command that opened it must not record dependencies into an asset that
has already finished, so a stream handed across that boundary is opened with an `EnvResolver`.

Both are implemented for `E: Environment` **where `E::Value: RecordValue`**: a value that is a view
or a source is taken as such, and any other is serialized in its data format into
`ChunkValue::Bytes`. So records work in any environment whose value type implements the adapter —
`liquers-lib`'s does, and an integration with its own value type adds four methods.

### The stream is `futures::Stream`, extended by one method

`futures = "0.3.34"` is **already a direct dependency of `liquers-core`** (`Cargo.toml:77`, used in
`assets.rs`), so the standard trait costs nothing to adopt and a hand-rolled `next_batch` would be a
worse version of it. The whole combinator vocabulary comes with it: applying a filter to each view is
`map`, a limit is `take`, an asynchronous transformation is `then`, and concurrency is later
`buffer_unordered` rather than a rewrite. `RecordStream` adds only `schema()`, and `record_stream`
re-wraps a combinator chain so the schema survives it.

**No stream alias in `liquers-core`.** A boxed stream would ordinarily need a per-target alias in
`maybe_send.rs`, because `StreamExt::boxed()` is always `Send`-boxed and the `MaybeSend` marker cannot
be added as a trait-object bound (E0225). Making `MaybeSend` a **supertrait** of `RecordStream`
removes the need: `Pin<Box<dyn RecordStream>>` has the right `Send`-ness on each target by
transitivity, and `Box::pin(s)` coerces to it with no helper. `RecordSource` returns the existing
`liquers_core::maybe_send::BoxFuture`.

**A source's partition is still data.** A `ChunkedRecordSource` trait with a `partition()` method
would be redundant against the manifest: `chunks()` is the partition, expressed **as ids** — which
can be stored, cached, diffed and inspected. A source whose partition is **discoverable only
incrementally** — a SQL table paginated by offset, a remote API revealing the next page token after
each page — is served by the reserved `template` field. That is why `chunks()` returns a `ChunkList`
distinguishing `Known` from `Unbounded` **now**: enumeration is not always possible, and a consumer
written against a complete `Vec` would have to be revisited. See
[`chunking-and-resumability.md`](./chunking-and-resumability.md).

### Three scales, unchanged

| Scale | Unit of | Bounded by | Represented as |
|---|---|---|---|
| **Record** (row) | retrieval and identity | — | one row of a view |
| **Batch** | **memory** — what is resident at once | a row count or a byte budget | one item of a stream: a `RecordView`, usually a `RecordBatch` |
| **Chunk** | **refresh** — what expires and is re-produced together | the source's own partitioning | a `ChunkId`; streams as one or more batches |

A multi-gigabyte table is one source, partitioned into chunks; each chunk streams as batches, and
**one batch at a time is resident**. A chunk is therefore not materialized on traversal, which is the
point: a single large parquet file is one chunk, and holding it in memory is what this design exists
to avoid. A `RecordView` held as a *value* is the deliberate exception, used when a table is small
enough to be one.

### Views: `RecordView` and its implementations

```rust
/// Any finite table with random access: a batch, a projection, a row selection, a derived
/// column, a generated table. **Synchronous** — anything that must `.await` is a source.
pub trait RecordView: Debug + MaybeSend + MaybeSync + 'static {
    fn schema(&self) -> &Arc<RecordSchema>;
    fn len(&self) -> usize;
    /// **The one required read**: rows `rows` of column `col`, in Arrow layout with
    /// `Arc`-shared buffers. Out of range is an error, never a panic.
    fn column_range(&self, col: usize, rows: Range<usize>) -> Result<Column, Error>;

    // Provided. Each has the right cost without being overridden, because each asks
    // `column_range` for exactly the rows it needs.
    fn column(&self, col: usize) -> Result<Column, Error> {
        self.column_range(col, 0..self.len())
    }
    fn value(&self, row: usize, col: usize) -> Result<FieldValue, Error> {
        self.column_range(col, row..row + 1)?.get(0)
    }
    /// A `RecordBatch` with the same rows: `column()` for every field.
    fn materialize(&self) -> Result<Arc<RecordBatch>, Error> { /* … */ }
    /// The typed fast path — not an `Any` downcast. `Some` only for a `RecordBatch`.
    fn as_batch(&self) -> Option<&RecordBatch> { None }
    /// The chunk these rows came from, when they came from one.
    fn chunk_id(&self) -> Option<&ChunkId> { None }
    /// The origin dictionary the `Source`-role column indexes.
    fn origins(&self) -> &[ChunkOrigin] { &[] }
    /// The **implicit** identity of `row` — the chunk it came from and its position there.
    /// Always available: a table that came from no chunk is chunk 0. Views that select rows map
    /// through their base. See §"Every row has an implicit id".
    fn row_id(&self, row: usize) -> Result<RowId, Error> {
        Ok(RowId { chunk: 0, row: row as u64 })
    }
    /// `row`'s number across the whole source, when the rows before its chunk have been counted.
    fn row_number(&self, row: usize) -> Result<Option<u64>, Error> { Ok(Some(row as u64)) }
}

/// (chunk index, row within the chunk). The chunk index is the chunk's position in the source's
/// `ChunkList` — global across `chunks:` and `template:` (`manifest-format.md` §4a).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct RowId { pub chunk: u64, pub row: u64 }
```

#### Why the required method is a column *range*

The required read decides what every other access costs. Four candidates were compared:

| Required method | Whole column | One cell | Rows 1000..1050 of a computed view |
|---|---|---|---|
| `value(row, col)` | slow — one `FieldValue` per cell | cheap | cheap |
| `column(col)` | fast | **O(n)** — builds the whole column | **O(n)** — the same |
| **`column_range(col, rows)`** | **fast** | **cheap** | **cheap** |
| both `value` and `column` | fast | cheap | cheap — but two methods must agree, and nothing checks it |

A cell-first contract makes columns expensive, which defeats the columnar layout. A column-first
contract looks right until views stack: a row slice over a *computed* view asks its base for the
whole column and cuts it, so a UI scrolling a generated million-row table recomputes the column for
every window. Requiring both doubles every implementor's work, and letting each default to the other
recurses forever when neither is overridden — Rust cannot require "at least one". **One ranged
method** gives every access its proportional cost.

#### The implementations

All in `liquers-records/src/views.rs`. Each view holds its base as an `Arc<dyn RecordView>`.

| View | Built by | `column_range(c, r)` | `materialize()` |
|---|---|---|---|
| `RecordBatch` | builder, deserialization, `concat` | zero-copy slice | a shallow clone — `Arc`s, no data |
| `ColumnsView` | `select_columns` | delegates, with `c` mapped | `Arc`s only, over a batch |
| `RowRangeView` | `slice`, `head`, `row` | delegates, with `r` shifted | zero-copy slices, over a batch |
| `RowIndexView` | `filter` (mask), `take` (indices) | gathers the selected rows inside `r`, **for column `c` only** | a copy of the selected rows |
| `DerivedColumnView` | `with_column` | base columns delegate; the derived one is `f` over the source columns' same range | computes the derived column once |
| `AppendedColumnsView` | `with_columns` | base columns delegate; appended ones slice | `Arc`s only, over a batch |
| `RowFnView` | `RowFnView::new(schema, len, f)` | calls `f(row, c)` for each row in `r` | computes every cell |

**A mask is turned into indices when the view is built.** A filtered view must map its output row *k*
to a base row, and a bitmap answers that only by counting set bits from the start. Building the
index list once — O(n), four bytes per kept row — makes every later range read proportional to the
range. `Bitmap::iter_ones` is added for it.

**A view produces its columns on request; it does not keep them.** Reading the same column of a
filtered view twice gathers twice. The consumer's own variable is the cache: a `Column` is an owned,
cheaply cloned value. A consumer that reads a view repeatedly calls `materialize()` first. There is
no cache inside a view and no selection-aware kernel — both were considered and rejected as more
machinery than a light mechanism should carry.

**Views compose by stacking, and nothing merges them.** A filter over a filter is a `RowIndexView`
over a `RowIndexView`: correct, at one indirection per layer. Merging selections, and pushing a
selection down into a source (a SQL `WHERE`, skipping the chunk files a manifest lists) are **out of
scope** — the aim is a light, flexible mechanism rather than a query engine. A specialized source,
such as a future SQL record source, may offer pushdown as part of its own implementation, outside the
generic mechanism.

#### Building views

The constructors are **inherent methods on `dyn RecordView`**, not default trait methods:

```rust
impl dyn RecordView {
    /// Keeps the `Id` and `Source` fields even when not named — see below.
    pub fn select_columns(self: &Arc<Self>, names: &[&str]) -> Result<Arc<dyn RecordView>, Error>;
    pub fn slice(self: &Arc<Self>, offset: usize, len: usize) -> Result<Arc<dyn RecordView>, Error>;
    pub fn row(self: &Arc<Self>, row: usize) -> Result<Arc<dyn RecordView>, Error>;
    pub fn cell(self: &Arc<Self>, row: usize, column: &str) -> Result<Arc<dyn RecordView>, Error>;
    pub fn filter(self: &Arc<Self>, mask: &Bitmap) -> Result<Arc<dyn RecordView>, Error>;
    pub fn take(self: &Arc<Self>, indices: &[u32]) -> Result<Arc<dyn RecordView>, Error>;
    /// A derived column: `f` receives the same row range of each source column.
    pub fn with_column<F>(self: &Arc<Self>, field: FieldSchema, sources: &[usize], f: F)
        -> Result<Arc<dyn RecordView>, Error>
    where F: Fn(&[Column]) -> Result<Column, Error> + MaybeSend + MaybeSync + 'static;
    /// Precomputed columns appended — search evidence, which is not a function of other
    /// columns. Each must have `len()` rows.
    pub fn with_columns(self: &Arc<Self>, fields: Vec<FieldSchema>, columns: Vec<Column>)
        -> Result<Arc<dyn RecordView>, Error>;
}
```

A default body could not build these: wrapping `Arc<Self>` into an `Arc<dyn RecordView>` requires
`Self: Sized`, and a default body is type-checked without it. `ForeignValue`'s documentation records
the same wall. An inherent block on the trait object has `Self = dyn RecordView`, so the `Arc`
clones directly — and, not being part of the vtable, its methods may be generic, which `with_column`
needs.

**Column selection always keeps the key columns.** `select_columns` retains the `Id` and `Source`
fields whether or not they are named. That keeps a declared `Id` present in **every**
view rather than only of batches, and keeps every row identifiable — the "accompanying data" that
lets a single cell still say which record it belongs to.

**Position is a view concept.** Inside a view a row's position is stable, so `row` and `slice` take
positions. Across a source it is not — chunk sizes may be unknown — so a row of a source is addressed
by its id (`rec_id`), never by position.

#### Every row has an implicit id

A row is identified **without any column** by its `RowId`: the index of the chunk it came from, and
its position in that chunk. It is the fallback identity — the analogue of SQLite's `rowid` — and it
exists for every row of every table, whatever its schema declares.

| | How it is known | Cost |
|---|---|---|
| **`RowId { chunk, row }`** | The chunk index is the chunk's position in `chunks()`; the row is its position in that chunk | Nothing per row: a batch records it once per run of rows |
| **Row number** (`row_number`) | The chunk's first row number plus the row. A stream counts rows as it traverses chunks in order, so every batch it yields knows its first row number | Known during a traversal and in anything materialized from one; **not** when a single chunk is read on its own, until chunk sizes are recorded (open question 9) |

A `RecordBatch` records where its rows came from as a short list of runs:

```rust
/// Rows `len` long, starting at `first_row` of chunk `chunk`, and at row number `first_number`
/// of the source when that is known. A batch from one chunk has one run; a table materialized
/// from several chunks has one run per chunk.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct RowRun { pub chunk: u64, pub first_row: u64, pub first_number: Option<u64>, pub len: usize }
// on RecordBatch:   pub rows: Vec<RowRun>   — the run lengths sum to `len`
```

Views answer `row_id` and `row_number` through their base: a row range shifts, a filter maps through
its indices, a projection passes straight through. So **the `RowId` of a filtered row is the row's
original position**, not its position in the filtered view.

**The explicit `Id` becomes optional.** A schema declares **at most one** `Id` field. When it does,
that field is the record's identity — what `rec_id` matches and what an engine indexes as the key.
When it does not, the `RowId` is. The schema-less reader therefore **never guesses an `Id`**: a file
read without a schema has no `Id` column and its rows have their implicit ids.

**What the implicit id is not: stable across versions of its chunk.** Re-produce a chunk with a row
inserted and every later `RowId` in it shifts. Two consequences:

- **Reconciliation still works**, because it is chunk-wise: when a chunk's version changes, a
  consumer replaces *all* of that chunk's records — delete by the chunk term, insert the new ones.
  Positions inside a replaced chunk never need to match the old ones.
- **Identity of a record across versions** — a bookmark to "order 42" that survives a refresh, or an
  engine updating one record in place — needs the explicit `Id`. The search design's sinks say which
  they require.

**Addressing by implicit id is cheap.** `ns-rec/rowid-<chunk>-<row>` over a source opens **only**
chunk `chunk` — `chunks()` names it — so it reads one chunk rather than walking the stream, which is
what `rec_id` must do. Over a view it selects that row. `row-<n>` stays the position within a view.

#### Kernels live on `Column`, so views get the fast path too

```rust
impl Column {
    pub fn len(&self) -> usize;
    pub fn data_type(&self) -> FieldType;
    pub fn get(&self, i: usize) -> Result<FieldValue, Error>;
    pub fn slice(&self, offset: usize, len: usize) -> Result<Column, Error>;   // zero-copy
    pub fn take(&self, indices: &[u32]) -> Result<Column, Error>;              // the gather
    pub fn filter(&self, mask: &Bitmap) -> Result<Column, Error>;
    pub fn compare(&self, op: CompareOp, value: &FieldValue) -> Result<Bitmap, Error>;
    pub fn null_mask(&self) -> Bitmap;
    pub fn concat(columns: &[Column]) -> Result<Column, Error>;
}
```

Every view hands out real Arrow-layout `Column`s, so the typed kernels serve views and batches alike,
and **one column of any view exports to Arrow**. "High-performance" here means contiguous typed loops
over `&[T]` that the compiler vectorizes — no SIMD crate, no `unsafe`, no expression optimizer, no
parallel scheduler. What stays batch-only is **holding** the columns, and exporting a whole table as
one Arrow struct array, which goes through `materialize()`.

#### Writing a view

The contract is columnar. Two things keep writing one easy: a **mutable table** for data built
row by row or edited, and `RowFnView` for data computed by a closure.

```rust
/// A table being built or edited, readable as a view while it is written. A trait rather than
/// one struct's methods, so code that fills a table — the readers, a command — is written once
/// against it.
pub trait RecordViewMut: RecordView {
    /// Room for `additional` more rows in every column — the capacity call.
    fn reserve(&mut self, additional: usize);
    /// One row, in schema order. Type-checked; `FieldValue::Null` sets the validity bit.
    fn append_row(&mut self, values: &[FieldValue]) -> Result<(), Error>;
    fn set_value(&mut self, row: usize, col: usize, value: &FieldValue) -> Result<(), Error>;
    /// One column, for bulk and typed writes. Columns may then differ in length for a while:
    /// `len()` is the **shortest** column's length, and `freeze()` requires them equal.
    fn column_mut(&mut self, col: usize) -> Result<&mut ColumnMut, Error>;
}

/// The mutable table: owned, unshared, growable. `freeze()` makes it a `RecordBatch` without
/// copying — the `BytesMut` / `Bytes` idiom of the `bytes` crate.
pub struct RecordBatchMut { schema: Arc<RecordSchema>, columns: Vec<ColumnMut>, len: usize }
impl RecordBatchMut {
    pub fn new(schema: Arc<RecordSchema>) -> Self;
    /// The expected number of rows, allocated once.
    pub fn with_capacity(schema: Arc<RecordSchema>, rows: usize) -> Self;
    /// Fails unless every column holds `len` rows.
    pub fn freeze(self) -> Result<RecordBatch, Error>;
}
impl RecordViewMut for RecordBatchMut { /* … */ }
impl RecordView for RecordBatchMut { /* column_range copies the range: the columns are still changing */ }

impl RecordBatch {
    /// A mutable table with these rows. Buffers this batch holds alone are taken over
    /// (`Arc::try_unwrap`); shared ones are copied.
    pub fn into_mut(self) -> RecordBatchMut;
}

/// The mutable column: a growable typed buffer plus validity. Its storage is an aligned buffer
/// from the start, so `freeze()` is a move, not a copy.
pub struct ColumnMut { /* … */ }
impl ColumnMut {
    pub fn new(data_type: FieldType) -> Self;
    pub fn with_capacity(data_type: FieldType, rows: usize) -> Self;
    pub fn reserve(&mut self, additional: usize);
    pub fn push(&mut self, value: &FieldValue) -> Result<(), Error>;
    pub fn set(&mut self, row: usize, value: &FieldValue) -> Result<(), Error>;
    pub fn len(&self) -> usize;
    pub fn freeze(self) -> Column;
}
```

**Mutability stops at the value boundary.** A `RecordBatchMut` is owned by whoever builds it; what
becomes a value, or crosses a stream, is the frozen `RecordBatch` behind an `Arc`, which nothing
mutates. Editing a stored table is `into_mut()`, edit, `freeze()` — a new value, as everywhere else
in Liquers. Typed per-column pushes (`push_i64`, `push_str`) are the obvious next methods and are
left for when a bulk path needs them.

```rust
/// A table computed cell by cell — how a generator written as a closure meets the columnar
/// contract. `f(row, col)`, so a range read computes only the requested column.
pub struct RowFnView<F> { schema: Arc<RecordSchema>, len: usize, f: F }
impl<F> RowFnView<F>
where F: Fn(usize, usize) -> Result<FieldValue, Error> + MaybeSend + MaybeSync + 'static
{
    pub fn new(schema: Arc<RecordSchema>, len: usize, f: F) -> Result<Self, Error>;
}
```

**The cost, stated:** `RowFnView` builds one `FieldValue` per cell. That is right for generated and
small tables; a large generator should implement `column_range` directly and write its buffer in one
pass.

#### A view as a value: reading a single cell

A view with **exactly one row and exactly one payload column** reads as a scalar. *Payload* columns
are those whose `KeyRole` is neither `Id` nor `Source`; when a view has none — `select_columns-id` —
the `Id` column is the value.

**It reads exactly as its cell would as a base `Value`.** The cell is converted to core's `Value` and
the conversion is delegated, so a scalar read from a table and a scalar written in a query cannot
disagree:

| `FieldValue` | as base `Value` |
|---|---|
| `Null` | `None` — so the `_option` conversions give `None` |
| `Bool`, `Int`, `Float`, `Text`, `Bytes` | `Bool`, `I64`, `F64`, `Text`, `Bytes` |
| `UInt` | `I64` when it fits; a conversion error otherwise |
| `Date`, `Timestamp` | `Text`, ISO-8601 — core has no temporal variant |
| `Vector` | `Array` of `F64` |

That inherits the base value's own choices, including its lossy `i64 → f64`; whether automatic
conversions should refuse lossy edges is `VALUE-CONVERSION-CAPABILITY`'s question, not this design's.
Any other shape refuses with a conversion error naming the view's row count and payload columns.
`try_into_json_value` gives the scalar for a single cell and an array of row objects otherwise.

So `-R/data/prices.csv/-/ns-rec/rec_id-42/select_columns-price` is a number wherever a number is
expected — including a command's `f64` argument, linked through a recipe's `links:`. **That last case
needs `EXTENDED-VALUES-CANNOT-BIND-TO-SCALAR-ARGUMENTS` fixed first**: a resolved link binds through
`TryFrom<Value>`, and today every scalar `TryFrom` refuses an extended value. It is a Phase 4
prerequisite step (§"Known-Issue Preflight").

#### A small view keeps its whole base alive

A one-row view of a 100 MB batch holds 100 MB for as long as the view is cached. The rule: **commands
whose result is bounded by a count the caller chose materialize** — `rec_id`, `row`, `head` — because
copying a few rows costs nothing and releases the base. `select_columns`, `slice` and `filter` return
views. The distinction is in the *commands*; the Rust constructors above are always lazy.

#### Views are synchronous; asynchronous work is a source

`RecordView` guarantees a known `len()` and an immediate `column_range()`, and three consumers depend
on that and are synchronous: scalar reading, **argument binding** (`TryFrom<Value>`, `commands.rs`),
and Arrow export and table rendering. A source may have an unknown length and answers only when
awaited. So:

- **An asynchronous transformation is a source** wrapping a source or a view. Its stream maps each
  view with `then`, and it is written with `record_stream`. Evaluating a query per row is this case.
- **View → source is free** (`InMemorySource`). **Source → view needs an await** (`materialize`),
  and argument binding is synchronous, so it **cannot happen automatically**. A command that wants a
  view and receives a source awaits the collection itself. `rec_id` over a source works this way: it
  walks the source and returns a materialized one-row batch.
- An asynchronous random-access view — `async fn column_range` — would be a fourth abstraction and
  would make every view consumer asynchronous, scalar reading and argument binding included. What it
  would buy is paging a UI through a source by position, and a source position is not stable when
  chunk counts are unknown. Not designed; a windowed read of a source is a possible later
  `RecordSource` method.

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
// on `RecordSource` — §"The types"
/// Cheap, synchronous, no I/O. The reconciliation planning primitive.
fn chunks(&self) -> ChunkList<'_>;
/// Full provenance for one chunk. Reads metadata through the resolver, so it is async.
fn describe_chunk<'a>(&'a self, id: &'a ChunkId, resolver: &'a dyn ChunkResolver)
    -> BoxFuture<'a, Result<ChunkDescriptor, Error>>;
```

This mirrors `get_asset_info`: listing is cheap, describing costs a read. Reconciliation uses both —
`chunks()` to learn what exists, `describe_chunk` to get the `(id, version)` pair for each one it
must compare.

So **provenance is "the query and the dependency versions this chunk was produced from"**, and
**validity is the staleness check the dependency manager already performs**. A record's provenance is
its chunk's — that is the flyweight, and it costs one `Source`-role column index per row rather than
a `Metadata` per record.

**Reconciliation, not push.** A consumer holding a copy of a chunk compares `(ChunkId, Version)`
pairs against a fresh `chunks()` and re-opens what differs. Correctness comes from the set
difference; any notification mechanism is a latency optimization on top, never the thing correctness
depends on. This is what the search design's
[`interoperability-layer.md`](../store-and-asset-search/interoperability-layer.md) §3 builds on, and
why `chunks()` must not produce records.

### Value extension — `ExtValue`, not core's `Value`

Two of the three abstractions are values: the view and the source. The traits are
`liquers-records`'; **the value variants holding them live on `ExtValue` in `liquers-lib`**, and
neither could live on `liquers_core::value::Value`.

`Value` derives `Serialize, Deserialize, Debug, Clone, PartialEq` and is `#[serde(untagged)]`
(`value.rs:20-21`). Every variant must satisfy all five, and a trait object satisfies none of the
hard ones:

| Requirement | `Arc<dyn RecordView>` or `Arc<dyn RecordSource>` |
|---|---|
| `Clone` | fine — clones the `Arc` |
| `PartialEq` | not derivable for a trait object |
| `Serialize` | not derivable; most sources have no byte form at all |
| **`Deserialize`** | **impossible in principle** — bytes cannot say which implementation to rebuild, and a generator cannot be reconstructed from bytes at any amount of effort |

`untagged` makes a half-measure worse, since deserialization tries each variant in turn.

`ExtValue` is the right home and `CLAUDE.md` already prescribes it for new value types. It derives
**only `Debug, Clone`** (`mod.rs:23`) and already holds exactly this shape — `Arc<dyn UIElement>`,
`Arc<dyn ForeignValue>`, and `Arc<Mutex<dyn WidgetValue>>` behind the `egui` feature.

```rust
// liquers-lib/src/value/mod.rs
pub enum ExtValue {
    // … existing …
    /// A finite table — a `RecordBatch` or any view. Shareable, cacheable, serializable
    /// (materialized on the way out).
    #[cfg(feature = "records")]
    RecordView { value: Arc<dyn liquers_records::RecordView> },
    /// Something that can be asked, repeatedly, for a stream. Shareable, never consumed by use;
    /// serializable only as a manifest.
    #[cfg(feature = "records")]
    RecordSource { value: Arc<dyn liquers_records::RecordSource> },
}
```

**One variant for every table, not one per representation.** A batch and a view are both
`RecordView`: a second variant for the materialized form would put a storage detail into the type
system — argument types, `type_identifier` in metadata, `TypeInfo` entries and UI widgets would all
need two names for "a table" — and a stored view would read back as a different type than was
written. The type-level guarantee a second variant would buy, "this is materialized", is worth nothing
when `materialize()` is free on a batch. `ExtValue::Image` is the precedent: one variant over
`DynamicImage`, which has many representations internally.

**Only two variants, and neither is hazardous.** The three-way model keeps the troublesome thing out
of the value system entirely: a stream is never an `ExtValue`, because a stream is a traversal in
flight and a value is something you can hold, clone and cache. `Clone` clones the `Arc`, `PartialEq`
is not required, and serialization is a method.

**Where the partly-consumed hazard went.** Phase 1 answer 2 warned that a stream "may be already
partly consumed", that sharing it should be avoided, and that stream commands would therefore be
`volatile`. With sources as values and streams confined to the inside of a call, **the hazard has
nowhere to appear**: what a command receives and returns is a source, which is re-openable by
construction, and the stream it opens lives and dies inside that call.

`volatile` is therefore *not* the normal case for a record command. It is needed only when a result
genuinely should not be reused, which is a separate question from streaming.

**Serialization is a fallible, per-format method, which is exactly the semantics needed.**
`DefaultValueSerializer::as_bytes(&self, data_format: &str) -> Result<Vec<u8>, Error>`
(`value.rs:919-926`) may refuse, and `ExtValue::UIElement` already refuses this way with
`ErrorType::SerializationError` (`mod.rs:309-316`). Each arm is a one-line delegation, as the
`Foreign` arm's is:

| Value | `as_bytes` |
|---|---|
| `RecordView` | every format of §"Table formats" — `csv`, `tsv`, `ndjson`, `json`, `md`, `html`, and with their features `feather` and `parquet` — through `materialize()` when the view is not already a batch |
| `RecordSource` — `ManifestSource` | `yaml`, `json` — the manifest |
| `RecordSource` — any other: in memory, filtering, mapping | **refused** — `SerializationError`. Its rows are reached through `materialize` |

**A refusal is not a failure.** The asset write path already handles a value with no byte form: it
stores the metadata only ("Non-serializable data – store metadata only", `assets.rs`, step 8 of the
produce path), and the value is re-derived from its recipe when next needed. A wrapped source is
cheap to re-derive — it is its base plus a function — so nothing is lost. (The reload reaches the
recipe through the fast-track's *corrupted-data* branch, which tries to deserialize the empty bytes
first and logs a corruption — correct in outcome, misleading in the log, and filed as
`METADATA-ONLY-ENTRY-RELOADS-AS-CORRUPTED`.) Where an error *is*
constructed it uses a **typed constructor**, `Error::from_error(ErrorType::SerializationError, …)` as
the neighbouring `UIElement` arm does, never `Error::new`, which `CLAUDE.md` forbids — worth stating
because `value.rs:956` and `:968` reach for `Error::new` for serialization errors today.

**Deserialization rebuilds the reference implementation.** A `RecordView` identifier deserializes to
a `RecordBatch`; a `RecordSource` identifier to a `ManifestSource`, from `yaml` or `json` — the only
byte form a source has. So a stored view reads back materialized, under the same identifier — the
one-variant decision is what makes that a non-event.

**The feature gate.** Both variants are `#[cfg(feature = "records")]`, so every exhaustive `match` on
`ExtValue` gains a gated arm — see §"Feature-gating discipline". `liquers-web` depends on
`liquers-lib` with `default-features = false, features = ["webui"]` (`liquers-web/Cargo.toml:15`),
and adds `records` to that list; polars stays out of the browser, which is the requirement that drove
the DataFrame role in the first place.

**What the prototype adds that this design did not have.** `_store_batches` rewrites its manifest
after *every* batch, so a run that dies partway leaves its finished chunks usable. A generator cannot
be checkpointed; a growing manifest can. The equivalent here is a `store_record_stream` command
that appends a query to the manifest and rewrites it per chunk — cheap, and the difference between a
failed six-hour job being worthless and being resumable.

**What it does not carry over.** The prototype's manifest holds store *keys*, so it can `contains`,
list and clean its own directory before rewriting. Queries are more general — they cover a computed
chunk, which keys cannot — and the cost is that **cleanup is no longer automatic** for an unkeyed
manifest. Keyed chunks (§"Keyed chunks") are where a manifest owns its folder again.

**Registration** is the unchanged four-step procedure: extend `ExtValue`; choose the identifiers
(`RecordView` and `RecordSource`, bare CamelCase — Liquers owns both concepts); implement the
conversions in `ExtValueInterface` and `DefaultValueSerializer`, plus the scalar hooks of
§"A view as a value"; and add both `TypeInfo` entries to `ExtValue::type_descriptions()`
(`mod.rs:148`) — `CLAUDE.md`'s "four steps, not three; a type with no `TypeInfo` cannot be stored".

**Everything records-specific lives in `liquers-records`**, with `liquers-lib` holding the glue
behind its `records` feature — see §"Integration Points". `liquers-core` gains only the two general
features of §"Keyed chunks".

**Serializing the multi-gigabyte case** still meets `VALUE-SERIALIZATION-HAS-NO-INCREMENTAL-WRITER`:
`as_bytes` returns a `Vec<u8>`, so NDJSON or CSV over a large stream cannot stream through it. A
manifest sidesteps the problem — the manifest itself is small — which is one more reason to prefer
that form, and `liquers-axum` streams a source directly (§"Streaming a record source over HTTP").


### Table formats: what a `RecordView` writes and reads

Every `RecordView` serializes through the ordinary synchronous path, and a `RecordView` identifier
deserializes to a `RecordBatch`. Which formats are offered is decided by two costs — **code and
dependencies**, which matter most for the wasm build, and **what survives a round trip** — and the
heavier formats sit behind their own features.

| Format | Data format, aliases | Write | Read | Round trip | Implementation | New dependency | Feature |
|---|---|---|---|---|---|---|---|
| CSV | `csv`, `csv:comma` | yes | yes | values; types inferred; roles lost; null distinct from `""` | hand-written, ~250 lines with the reader | none | `records` |
| TSV | `tsv`, `csv:tab` | yes | yes | as CSV | shares the CSV code | none | `records` |
| NDJSON | `ndjson`, `jsonl` | yes | yes | values and JSON types; roles lost; differing keys read as a union | ~150 lines over `serde_json` | none | `records` |
| JSON | `json` — **one fixed shape**, an array of row objects; the other shapes are conversions (§"JSON shapes are conversions") | yes | yes | as NDJSON | ~50 lines over the NDJSON code | none | `records` |
| Markdown | `md`, `markdown` — a GFM pipe table | yes | yes | presentation: headers are labels, types inferred | ~60 lines to write, ~80 to read | none | `records` |
| HTML | `html` — a `<table>` fragment | yes | **no** | presentation only | ~60 lines | none | `records` |
| Arrow IPC file (Feather v2) | `feather`, `ipc`, `arrow_ipc`, `arrow` — the last so a `.arrow` chunk key reads as IPC | yes | yes | **lossless** — types, nulls, and the schema with roles | ~1 000–1 500 lines, mostly metadata: the bodies are our buffers | `flatbuffers` | `records-ipc` |
| Parquet | `parquet` | yes, minimal | **only with `polars`** | lossless as written; roles lost when read through polars | writer ~800–1 200 lines | `flate2`, already in the graph | `records-parquet` |

Names and aliases are those of the DataFrame serializer (`liquers-lib/src/polars/serde.rs:25-34`), so
a filename means the same thing for a `polars_dataframe` and a `RecordView`. The line counts are
sizing estimates, not measurements; Phase 4 measures the `liquers-web` quickstart's `.wasm` before
and after each feature.

**What is already in the wasm build**, from `cargo tree -p liquers-web --target wasm32-unknown-unknown`:
`serde_json`, `serde_yaml`, `chrono`, `base64`, `itoa`, `ryu`, `memchr`, and `flate2` with
`miniz_oxide` (through `png`). Not in it: any CSV crate, `flatbuffers`, snappy or zstd. So every
format in `records` costs **no new dependency** in the browser, and the Parquet writer's only
compression codec is one the browser build already carries.

#### Tier 1 — in `records`, no new dependency

**CSV and TSV are hand-written, and the reason is nulls rather than size.** No CSV crate is in the
lockfile, but the deciding fact is that a CSV file can distinguish a null from an empty string only
by quoting, and a reader must therefore know whether a field was quoted — which the `csv` crate does
not report. The convention is PostgreSQL's `COPY … CSV`: **an unquoted empty field is null, a quoted
`""` is the empty string**. The writer quotes exactly when needed (the separator, a quote, CR or LF,
or an empty string) and writes `\n`; the reader accepts `\n` and `\r\n`, embedded newlines inside
quotes, and doubled quotes. Phase 3's `column_null_distinct_from_empty_string` is the test.

| Column type | Written as |
|---|---|
| `Int`, `UInt` | decimal (`itoa`) |
| `Float` | shortest round-trip representation (`ryu`) |
| `Bool` | `true` / `false` |
| `Date` | `YYYY-MM-DD` |
| `Timestamp` | RFC 3339 with microseconds and `Z` |
| `Text` | as is, quoted when needed |
| `Binary` | base64 |
| `Vector` | a JSON array, in one cell |

**Formula injection is not sanitized.** A cell beginning with `=`, `+`, `-` or `@` can execute in a
spreadsheet; neutralizing it alters the data, which a data format must not do. Documented, not
handled.

#### Two readers: schema-aware and schema-less

Every text format is read by one of two readers, and **which one runs depends only on whether a
schema is available**, not on the format:

```rust
// liquers-records/src/formats/mod.rs
pub enum ReadSchema<'a> {
    /// A schema is known: parse every cell as its declared type. Nothing is guessed.
    Declared(&'a RecordSchema),
    /// No schema: infer one from the data (the rules below).
    Infer,
}

pub fn read_table(bytes: &[u8], format: TableFormat, schema: ReadSchema<'_>, options: &ReadOptions)
    -> Result<RecordBatch, Error>;
pub fn write_table(view: &dyn RecordView, format: TableFormat, options: &WriteOptions)
    -> Result<Vec<u8>, Error>;

/// `csv`/`tsv` (separator), `ndjson`, `json` (records), `md`, `html`; `feather` and `parquet`
/// under their features.
pub enum TableFormat { Csv { separator: u8 }, NdJson, Json, Markdown, Html, /* … */ }
pub struct ReadOptions { pub header: bool /* default true */ }
```

Where a schema comes from is a separate question from how it is used, which is what lets one reader
serve every source of schema — the manifest now, a linked argument, metadata later:

| Situation | Reader | Schema from |
|---|---|---|
| A chunk of a manifest that declares `uniform_schema` | **schema-aware** | the manifest |
| `ns-rec/parse` or `ns-rec/from_json` with a `schema` argument, linked through `links:` | **schema-aware** | the linked value — a `records_schema` result, or a YAML/JSON schema document |
| The `table` JSON shape (`from_json-table`) | **schema-aware** | the document itself |
| `feather`, `parquet` written by Liquers | **schema-aware** | the file's own metadata (`liquers.schema`) |
| `deserialize_from_bytes` of a `RecordView` in a text format | schema-less | — : the deserializer receives no metadata (see "Should the schema live in metadata?") |
| A chunk of a manifest with no `uniform_schema`; `parse` without a `schema` | schema-less | — |

**The schema-aware reader parses; it never guesses.**

| Concern | Rule |
|---|---|
| Columns | Matched **by name** through the header (`header: true`), by position otherwise. A schema field missing from the file is a null column when nullable, an error otherwise. A column the schema does not declare is an **error** naming it — the schema is a promise about the data, and a silent drop hides a changed extract |
| Cells | Parsed as the declared type: `Text` keeps the cell verbatim (so `01234` is safe without any rule), `Int` must parse as `i64`, `Date` as `YYYY-MM-DD`, and so on. A cell that does not parse is an error naming row, column and value |
| Nulls | CSV's convention (unquoted empty is null, quoted `""` is empty); JSON's `null`. A null in a non-nullable field is an error |
| JSON types | Must agree with the declared type, with the lossless coercions only: an integral number into `Float`, a string into `Date`/`Timestamp` by parsing, a base64 string into `Binary`, an array of numbers into `Vector` of the declared `dim` |
| `Id`, roles, labels, descriptions | From the schema — nothing is lost |
| Cost | One pass, cells parsed straight into their `ColumnMut`s; no buffered strings. Faster and smaller than inference, which must see a whole column before choosing its type |

**The schema-less reader infers**, with the rules of the table below. It exists because a file with
no known schema must still be readable, not because inference is good: types are guessed and roles
are lost. **A manifest without `uniform_schema` pays a further, less obvious cost** — each chunk is
inferred on its own, so two chunks of the same data can disagree (a column that happens to hold only
integers in one chunk and a decimal in the next), and a stream that is uniform in fact becomes
non-uniform by inference. Declaring the schema is what makes a folder of CSV files a uniform stream.

**How a manifest's chunks reach the reader: a stored chunk is parsed, a computed chunk is checked.**

- A chunk whose query is a **plain resource** (`-R/data/sales/daily_0010.csv`, no actions) is read
  as bytes from the store — `ChunkResolver::read_resource` — and parsed with the manifest's schema, in
  the format of the key's extension. This bypasses the asset manager's deserialization on purpose: a
  schema-less deserialization followed by a conversion to the declared types would be lossy (an
  inferred `1.50` becomes `1.5` before it could be kept as text) and would parse every cell twice.
- A chunk produced by a **command** is evaluated as usual and must yield a `RecordView`, which is
  **checked** against the declared schema — names, types and nullability — and refused on mismatch.
  Nothing is re-parsed.

`read_resource` records the key as a dependency when the resolver is a `ContextResolver`, as
evaluating it would have; how it does so without evaluating is a Phase 4 detail.

**A stored chunk is read whole, as one batch.** `read_resource` returns the file's bytes and
`read_table` parses them in one pass, so for stored files **a chunk is a batch**, and its size is
whatever the manifest's author made it. That is accepted for this version: keeping chunk files a
reasonable size is part of writing a manifest. Reading one large file as several batches needs an
incremental reader over streaming store reads (`CORE-STORE-OPENBIN-MISSING`), and changes nothing a
consumer sees — a chunk already streams as batches in the traits.

**Writing a schema by hand** is what a manifest's `uniform_schema` asks for, so the schema's YAML
form defaults what a person should not have to write: `nullable` defaults to `true`, `key` to `None`,
`role` to no indexing, `label` and `description` as before. And the `Id` field's implied role —
`Exact`-indexed and stored, which reconciliation needs — is **supplied** by `RecordSchema::new` when
left at the default, and rejected only when a role contradicts it. So the smallest valid schema is a
list of names and types; an `Id` is declared only when records need an identity beyond their
implicit `RowId`.

#### Schema-less inference rules

| Step | Rule |
|---|---|
| Type, per column | the first of `Bool` (`true`/`false`, any case), `Int`, `Float`, `Date` (`YYYY-MM-DD`), `Timestamp` (RFC 3339), `Text` that fits **every** non-null cell. A column of nulls only is nullable `Text` |
| `Int` means canonical | a cell counts as `Int` only if it fits `i64` **and** formatting the parsed number gives the cell back. So `01234`, `+5` and `1e3` stay text — a ZIP code or an account number keeps its leading zero — and a number too large for `i64` is not silently turned into a `Float` |
| Nullable | any null seen |
| JSON and NDJSON | JSON's own types decide: an integral number without exponent that fits `i64` is `Int`, other numbers `Float`. Only *strings* go through the date and timestamp tests, so `"42"` stays text. An array of numbers of one length in every row is a `Vector`; any other array or object is `Text` holding its JSON |
| The `Id` | **never guessed.** A table read without a schema has no `Id` column; its rows have their implicit `RowId`s. Writers keep schema order
| Everything else | default roles; labels derived from names |

So schema-less reading round-trips **values and plain types, not roles**. The schema-aware reader,
the `table` JSON shape and the self-describing binary formats are what keep roles.

#### Markdown and HTML

**Markdown and HTML are presentation, and use labels.** Their headers are each field's `label`, not
its `name` — the use `FieldSchema::label` was added for. Markdown right-aligns numeric columns in the
alignment row, and escapes `|`, backslash and line breaks (as `<br>`), and `<` and `&`, because GFM
renders inline HTML. Reading Markdown back turns a header into a name by lowercasing it and replacing
spaces with `_`, the inverse of a default label, so a label left at its default round-trips.

**HTML escaping is a security requirement, not a nicety.** `liquers-axum` serves `.html` as
`text/html`, so an unescaped `<script>` in a cell runs in the browser of whoever opens the URL. Every
cell, label and description is escaped (`&`, `<`, `>`, `"`, `'`). The fragment is a `<table
class="liquers-records">` with `<thead>` and `<tbody>`; a description becomes the header's `title`
attribute, numeric cells carry `class="num"`, and a null is an empty cell with `class="null"`.

**HTML cannot be read back, and the type registry cannot say so.** `TypeInfo::supported_data_formats`
means "written to **and** read from" (`type_system.rs:100`). Declaring `html` makes it servable by
filename; a stored `.html` table then fails to deserialize and is recomputed from its recipe, through
the path `METADATA-ONLY-ENTRY-RELOADS-AS-CORRUPTED` records. Filed as
`TYPE-INFO-CANNOT-DECLARE-WRITE-ONLY-FORMATS`; reading HTML would need an HTML parser, which is not
cheap.

#### JSON shapes are conversions, not formats

JSON has no single table shape. pandas alone writes six (`orient=`), a dictionary of lists is what
both pandas and polars build a frame from, and a data format name cannot choose among them — a
qualified name such as `json:table` cannot even be written in a query
(`DATA-FORMAT-CONSTANTS-AND-TOOLING`, point 4). So the `json` **format** has one fixed shape, the
array of row objects that NDJSON's lines also hold, and every shape — that one included — is reached
by **explicit conversion commands** between a `RecordView` and a plain JSON value:

| `orient` | Shape | pandas | polars |
|---|---|---|---|
| `records` *(default)* | `[{"id": 1, "total": 9.5}, …]` | `to_json(orient="records")`, `to_dict("records")` | `write_json()`, `to_dicts()` |
| `list` | `{"id": [1, 2], "total": [9.5, 3.0]}` — a dictionary of lists | `to_dict("list")`; `pd.DataFrame(d)` reads it | `to_dict(as_series=False)`; `pl.DataFrame(d)` |
| `split` | `{"columns": […], "index": […], "data": [[…], …]}` | `orient="split"` | — |
| `values` | `[[1, 9.5], …]` — rows without names | `orient="values"` | — |
| `columns` | `{"total": {"1": 9.5, …}}` — pandas' default for `to_json` | `orient="columns"` | — |
| `index` | `{"1": {"total": 9.5}, …}` | `orient="index"` | — |
| `table` | `{"schema": {"fields": […], "primaryKey": ["id"]}, "data": [ …records… ]}` | `orient="table"` — a Table Schema | — |

- **The `Id` is pandas' index.** The shapes that have an index — `split`, `columns`, `index`, and
  `table`'s `primaryKey` — put the declared `Id` column there, or the row number when the schema
  declares none; reading them turns an index back into an `Id` column. In `records` and `list` the `Id` is an ordinary column. JSON object keys are strings, so an
  `Id` read from `columns` or `index` keys is text unless a schema says otherwise.
- **`table` is the lossless text shape.** Its Table Schema (the Frictionless Data standard, which
  pandas writes and reads) has `title` and `description` per field — our `label` and `description`
  exactly — and `primaryKey` for the `Id`. Types map as `integer`, `number`, `boolean`, `string`,
  `date`, `datetime`, `string` with `format: binary`, and `array` for a vector. Roles travel as an
  extra field property that a reader not knowing it ignores. Reading `table` is schema-aware.
  **Interop is tested against a fixture written by pandas**, not assumed from the specification.
- **`values`** carries no names: reading it needs a schema (positional) or names the columns `c0`,
  `c1`, ….

```
…/ns-rec/to_json-list/data.json            a dictionary of lists, served or stored as JSON
…/ns-rec/to_json-table/data.json           lossless: schema, roles, labels
-R/data/export.json/-/ns-rec/from_json      shape detected
-R/data/export.json/-/ns-rec/from_json-split
```

`to_json` returns a plain JSON value — core's `Value::Array` or `Value::Object` — so the result
serializes, caches and travels like any JSON value, and the shape is written in the query. `from_json`
takes a JSON value, or text or bytes holding one. Its default `orient` is `auto`, which recognizes an
array of objects (`records`), an array of arrays (`values`), an object with `schema` and `data`
(`table`), one with `columns` and `data` (`split`), and an object whose values are arrays of one
length (`list`). An **object of objects is ambiguous** — `columns` and `index` have the same shape
transposed — and `auto` refuses it, asking for the orient.

**A prerequisite found while designing this:** `liquers-lib`'s `SimpleValue` *writes* `json` but its
`deserialize_from_bytes` accepts only `txt`, `html` and `toml` (`simple.rs:637-647`), so a JSON
document stored under a key cannot be read back as a value. `-R/data/export.json/-/ns-rec/from_json`
depends on that being fixed — filed as `SIMPLE-VALUE-CANNOT-READ-JSON`. Until then `from_json` works
on computed JSON values and on text.

#### Tier 2 — Arrow IPC file (Feather v2), behind `records-ipc`

**Cheap relative to its value, because the bodies are already written.** An IPC file is the magic
`ARROW1`, a schema message, one record-batch message per batch, a footer indexing them, the footer's
length and the magic again ([IPC]). A record-batch message's body is the column buffers, each padded
to 8 bytes — exactly our `Buffer`s, in the order the C Data Interface already fixes. What has to be
*written* is the metadata: `Schema`, `Field`, `RecordBatch` (field nodes and buffer offsets), `Footer`
and `Block`, as flatbuffers.

- **Lossless.** The schema's `custom_metadata` carries `liquers.schema`, the `RecordSchema` as JSON,
  so roles, labels, descriptions and the `Id` survive; a batch message's `custom_metadata` carries
  its `chunk_id`. Other readers ignore both and still see correct types.
- **Scope.** Exactly the `Column` subset of §"Where our layout meets Arrow's". Reading refuses
  dictionary batches, compressed bodies (the `compression` field — LZ4 and zstd are not carried),
  64-bit-offset types and any nesting other than `FixedSizeList(Float32)`, each with an error naming
  what was found.
- **Dependency.** The `flatbuffers` runtime (Apache-2.0, no dependencies) with code generated from
  Arrow's `Schema.fbs`, `Message.fbs` and `File.fbs`, checked in; unused tables are dead-code
  eliminated. A hand-written flatbuffer encoder for the eight tables involved is possible at ~500
  lines, but offsets and vtables are where silent corruption hides, and the runtime is small.
- **Interop is tested, not assumed**: with `records-ipc` and `polars` both on, a file written here
  reads in polars and one written by polars reads here.
- **In the browser** it is optional. Inside one page the lent-buffer mechanism is better; IPC is for
  bytes crossing a process boundary — an `.arrow` served to an `apache-arrow` JavaScript client. The
  IPC *stream* format (no footer) is the natural body for a future streaming serializer
  (`VALUE-SERIALIZATION-IS-SYNCHRONOUS-AND-WHOLE-VALUE`) and a small variation of this code.

#### Tier 3 — Parquet: writing is cheap, reading is not

**The writer**, behind `records-parquet`: `PAR1`, one row group per batch, one column chunk per
column, one version-1 data page per chunk, `PLAIN` encoding, definition levels in the RLE/bit-packing
hybrid for nullable columns (a flat schema has a maximum level of 1), and a `FileMetaData` footer
with logical types (`STRING`, `DATE`, `TIMESTAMP(MICROS)`) and `key_value_metadata` carrying
`liquers.schema`. The footer and page headers are Thrift compact protocol — a small encoder for the
handful of structures involved, not a Thrift dependency. Per-column min/max statistics are cheap for
primitive columns and worth writing, because they let other engines skip row groups. Compression is
`UNCOMPRESSED` or `GZIP` through `flate2`. **`Vector` columns are refused**: Parquet's `LIST` needs
repetition levels, which a flat writer does not produce.

**A reader is not cheap, because real files are not written the way this writer writes them.**
pyarrow writes dictionary-encoded pages compressed with snappy by default; polars writes zstd; both
may use data page v2 and delta encodings. A reader that handles only files like its own would refuse
most Parquet a user actually has — worse than refusing honestly. So:

- **With `polars` on** (native), in `liquers-lib`'s polars bridge, Parquet is read through polars' reader and converted with a
  `DataFrame → RecordBatch` bridge, the reverse of the one already planned. Roles are lost on this path
  unless polars exposes the key-value metadata; the `Id` is inferred as for CSV.
- **Without it**, reading is refused with an error naming the `polars` feature.
- A polars-free native reader would be the arrow-rs `parquet` crate behind yet another feature — large,
  with the `arrow-*` crates behind it. Not proposed.

#### Considered and left out

| Format | Why not |
|---|---|
| XLSX | A zip container plus SpreadsheetML to write (`rust_xlsxwriter`) and `calamine` to read — moderate dependencies — and polars already offers XLSX for DataFrames on native |
| YAML, a list of maps | Trivial with `serde_yaml`, but a worse NDJSON |
| Avro, ORC | No use case, heavy |

#### Should the schema live in metadata?

Today a schema reaches a reader from a manifest, a linked argument, or the data itself (`table` JSON,
Feather, Parquet). Metadata would add the one missing case: **a single stored `x.csv` written by
Liquers reading back with its types and roles.**

| For | Against |
|---|---|
| A lossless CSV and NDJSON round trip for everything Liquers writes, with no manifest | **Layering.** `MetadataRecord` is `liquers-core`'s and `RecordSchema` is `liquers-lib`'s, so the field would have to be opaque — which is `CORE-METADATA-NO-APPLICATION-ATTRIBUTES`, not a records feature |
| `records_schema` and a UI's column list answered from metadata without reading the data, as `get_asset_info` answers "what is this" | **It would not reach the reader.** `deserialize_from_bytes(b, type_identifier, data_format)` receives no metadata (`value.rs:931`); a schema in metadata changes nothing on the load path until that call does |
| The search design could plan an index from metadata alone | **Two sources of truth.** A file edited outside Liquers no longer matches its metadata; the schema-aware reader then refuses what a schema-less read would accept. Correct, surprising, and in need of a rule — is a metadata schema authoritative or advisory? |

**Not in this design, and nothing here forecloses it.** When application attributes exist and the
load path can hand metadata to a deserializer, a metadata schema becomes one more row of the
"schema from" table above, read by the **same** schema-aware reader. That is the reason the reader
takes its schema as an argument rather than finding it itself. Open question 17.

#### The registry follows the features

`ExtValue::type_descriptions()` builds the `RecordView` format list with the same `#[cfg]`s that
compile the formats, so a build **never advertises a format it cannot write** — the rule
`availability02_advertised_types_match_the_enabled_features` enforces for stores, applied to formats.
A test asserts, per feature combination, that every advertised format writes. The media types these
formats need are partly missing from `liquers-core/src/media_type.rs` (`ndjson` has no entry;
`arrow`, `feather` and `parquet` map to `application/octet-stream`), filed as
`MEDIA-TYPES-MISSING-FOR-TABULAR-FORMATS`.

## Data Structures

In the `liquers-records` crate.

### Columns, not rows: the batch is Arrow-laid-out

A predicate — or any filter — over a **column** produces a boolean mask, and filters combine by
ANDing masks. That is how polars and DuckDB filter, and it is faster than a row walk. Decisively,
the cheap routes to Arrow (the C Data Interface, and typed arrays over wasm memory) are **only**
possible if the data is already laid out Arrow's way.

```rust
/// A batch of rows in Arrow's memory layout — the **materialized** `RecordView`. The unit of
/// memory, the form data rests in, and the form that exports to Arrow as a whole.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RecordBatch {
    /// Field names, types and roles — once per batch.
    pub schema: Arc<RecordSchema>,
    /// One column per schema field, in order. `len` rows each.
    pub columns: Vec<Column>,
    pub len: usize,
    /// Identity of the chunk these rows came from — once per batch, not per row. `None` for a
    /// table materialized from several chunks; `rows` then says which rows came from which.
    pub chunk_id: Option<ChunkId>,
    /// Where the rows came from, as runs — the implicit `RowId` of every row, and its row
    /// number when known (§"Every row has an implicit id"). One run for a single-chunk batch.
    pub rows: Vec<RowRun>,
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

**`RecordBatch` implements `RecordView`** with the fast answers: `column_range` is a zero-copy slice,
`as_batch` returns `Some(self)`, and `materialize` is a shallow clone — `Arc`s, no data. It keeps
`PartialEq`, which the trait object cannot have; tests comparing views compare their materialized
batches.

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
| **Arrow IPC / Feather** | yes — it is serialization | `liquers-records`, behind its `ipc` feature | Not sharing, but the lossless file format. Specified in §"Table formats" |
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
#[wasm_bindgen(js_name = RecordBatch)]
pub struct LiquersRecordBatch {
    /// Keeps every buffer alive for the handle's lifetime — the answer to Hazard B.
    /// A view crossing into JavaScript is materialized first: free for a batch, and the
    /// only way to give JS stable pointers into a view's computed columns.
    inner: Arc<RecordBatch>,
}

#[wasm_bindgen(js_class = RecordBatch)]
impl LiquersRecordBatch {
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
2. **A typed-array view must not outlive the handle.** `free()` drops the `Arc`, and a JS view held
   past that point is Hazard B with no detection. The `debug-handles` feature already used for
   `RUNTIME05` gives the test: assert the live batch-handle count returns to zero after `free()`.

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
to bundle — which is the wasm build, where polars is not an option at all. Views and column kernels
give that almost for free:

| Operation | Implementation |
|---|---|
| select columns | `select_columns` — a `ColumnsView`; no data copied |
| filter | `Column::compare` to a mask, masks combined with `Bitmap::and`/`or`, then `filter` — a `RowIndexView` |
| slice, head, one row | a `RowRangeView`; zero-copy over a batch |
| derived column | `with_column` — a `DerivedColumnView` |
| concat | `RecordBatch::concat`: schema equality check, then `Column::concat` |
| column stats | a pass per column |

So `liquers-web` gets a usable tabular value with **no new dependency**, and a polars-enabled native
build converts a materialized batch to a real `DataFrame` over the shared buffers when it wants one.
Group-by and join are a query engine, and out of scope.

### FieldValue — the scalar type

```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum FieldValue {
    Null, Bool(bool), Int(i64), UInt(u64), Float(f64),
    Text(Arc<str>), Bytes(Arc<[u8]>),
    /// Days since the epoch, as `Column::Date` stores it.
    Date(i32),
    /// Microseconds since the epoch, as `Column::Timestamp` stores it.
    Timestamp(i64),
    Vector(Arc<[f32]>),
}
```

**24 bytes** — `Date(i32)` is smaller than the largest payload and does not change it — against `serde_json::Value`'s 32 — smaller *and* able to carry bytes without
base64, a timestamp as a type, and a vector compactly. **Not a type parameter**: that would infect
`RecordBatch<V>`, `RecordView`, the stream and the value variants — circular, since `Value` is the
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
    /// Unique identifier, conventionally snake_case. What a predicate and a consumer match on.
    pub name: String,
    /// Human-readable, for table headers in documents, reports and dashboards.
    /// Defaults from the name the same way `ArgumentInfo` does — `name.replace("_", " ")`.
    #[serde(default)]
    pub label: String,
    /// What this field means. A data dictionary entry, a column tooltip.
    #[serde(default)]
    pub description: String,
    pub data_type: FieldType,
    /// Default `true` — what a hand-written schema means by leaving it out.
    #[serde(default = "true_default")]
    pub nullable: bool,
    /// Structural role in the batch — nothing to do with indexing.
    #[serde(default)]
    pub key: KeyRole,
    /// What an index should do with this field. Portable *intent*; engines translate it.
    #[serde(default)]
    pub role: FieldRole,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum KeyRole {
    /// The row's identity. **At most one per schema**; without one, the implicit `RowId` is.
    Id,
    /// Index into `RecordBatch`'s origin dictionary. At most one.
    Source,
    /// Ordinary data.
    #[default]
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
//   at most one field with KeyRole::Id — without one, the implicit RowId identifies rows
//   that field has indexed == Some(IndexKind::Exact)   — delete-by-term needs it; supplied
//   that field has stored == true                       — a hit must say which record it is; supplied
//   at most one field with KeyRole::Source
```

That is the same discipline Tantivy uses — the schema is validated when built and hands out field
handles — and it means a schema that *cannot* be reconciled is rejected at construction rather than
at the first refresh. A schema **without** an `Id` is valid: engines that update records one by one
need the explicit field, while chunk-wise reconciliation works on the implicit `RowId`
(§"Every row has an implicit id").

#### Alignment with `ArgumentInfo`

`FieldSchema` and `ArgumentInfo` (`command_metadata.rs:517`) describe different things — a column of
data, and a parameter of a command — but both are *a named, typed slot that a person eventually
sees*. Where they overlap they should agree, and where they differ the difference should be
deliberate.

| Concept | `ArgumentInfo` | `FieldSchema` | Verdict |
|---|---|---|---|
| Identifier | `name` | `name` | **Aligned.** Both snake_case, both the matching key |
| Human-readable name | `label`, defaulting to `name.replace("_", " ")` | `label`, **same default** | **Aligned** — deliberately, including the derivation, so the two feel like one system |
| Prose | *(absent)* | `description` | **Divergent, and `ArgumentInfo` is the one missing it** — see below |
| Type | `argument_type: ArgumentType` | `data_type: FieldType` | **Deliberately different.** `ArgumentType` describes what a query parameter may carry; `FieldType` describes a column's storage and maps onto Arrow. Different domains, and `data_type` is the right word for a column. The naming asymmetry is the cost of using each domain's vocabulary |
| Presentation hint | `gui_info: ArgumentGUIInfo` — the preferred entry widget | *(absent)* | **A real gap, deferred.** A field wants the read-side analogue: alignment, a number format, a date format. "Displayed as table headers in reports" will want it soon. Not invented here, because a display hint designed against no renderer is guesswork |
| Free hint bag | `hints: serde_json::Map` | *(absent, and should stay absent)* | **Deliberately divergent.** A free map is exactly the escape hatch through which engine-specific configuration would re-enter the schema, which §"What is portable, and what is not" spends its length keeping out. An argument has one consumer, the UI; a field has many, and the boundary matters more |
| Known values | `presets: Vec<ParameterPreset>` | *(absent)* | **Deferred.** The field analogue is an enumeration of expected values, useful for faceting — but it overlaps with what an `Exact` index already offers, and should not be added before the overlap is resolved |
| Default | `default: CommandParameterValue` | *(absent)* | **Not applicable.** A missing cell is `nullable` plus a validity bit, not a default |
| `multiple`, `injected` | present | *(absent)* | **Not applicable.** Both are about how a query supplies a parameter |
| `nullable`, `key`, `role` | *(absent)* | present | **Not applicable.** Storage and indexing have no argument analogue |

**Two things follow.**

`label` is aligned down to its default (`name.replace("_", " ")`, as six construction sites in
`command_metadata.rs` do it) and its builder shape (`with_label`). A field and an argument should not
feel like they came from different systems.

And the comparison found a gap **in the existing code, not in this design**: `ArgumentInfo` has no
per-argument documentation. `CommandMetadata` has `doc`, and the only prose an *argument* can carry
is its `label` or an untyped `hints` entry — so a command author cannot explain what a parameter
means in the place a UI or an agent would look. Filed as
`ARGUMENT-INFO-HAS-NO-DESCRIPTION`. `FieldSchema` takes `description` regardless; the two should
match once that is fixed.

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
`IndexKind::Exact` through `with_column`, and the index is then on a real field. Same
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
`RecordSource::schema()` and `RecordStream::schema()` are `Some` when the producer promises one
schema and `None` otherwise. A
consumer checks rather than assumes. The prototype takes the same position in the weakest possible
way — on a column-count mismatch it **warns and continues** — and this design keeps the tolerance
while making the promise inspectable.

Non-uniformity is legal with consequences rather than an error:

| Operation | Non-uniform source |
|---|---|
| iterate, filter per view | fine |
| `materialize` into one table | **fails**, naming the first differing field |
| serialize as data (CSV, NDJSON) | **not available** — serialization goes through `materialize`. Export chunk by chunk instead. Streaming serialization (`VALUE-SERIALIZATION-IS-SYNCHRONOUS-AND-WHOLE-VALUE`) would restore NDJSON for this case, each row carrying its own keys |
| serialize as a manifest | fine — the manifest does not care |

The motivating case is "every CSV file in a folder becomes one chunk", which is a manifest with one
query per file and no guarantee the files agree. That is useful rather than illegal: it works for
iteration, filtering and per-chunk use, and fails only where one schema is genuinely required.

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
`ChunkId`, stored **once per batch** rather than per row, plus the `Id`-role field when the schema
declares one and the implicit `RowId` always. *Addressability* —
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

This works for every record stream **whose schema declares an `Id`**. For every stream without
exception, `<source query>/ns-rec/rowid-<chunk>-<row>` addresses a row by its implicit id — and reads
one chunk rather than walking to it.
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

A bounded set of rows is either a **`RecordView`** (one table, materialized or not) or a
**`RecordSource`** that happens to be finite — an `InMemorySource` when its views are already in
memory. No third type sits between them: a `RecordSet { batches: Vec<RecordBatch> }` would duplicate
what a source already expresses, and would need its own answer to every question the source has
already answered about sharing, serializing and re-opening.

`truncated()` therefore lives on `RecordSource`, as a producer's report that it stopped early.

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

| Trait | Implemented by | Note |
|---|---|---|
| `RecordView` | `RecordBatch`, `ColumnsView`, `RowRangeView`, `RowIndexView`, `DerivedColumnView`, `AppendedColumnsView`, `RowFnView<F>` | §"Views". `RecordBatch` overrides `column_range`, `materialize` and `as_batch`; the others override `value` only where a direct read is cheaper than a one-row range |
| `RecordSource` | `ManifestSource`, `InMemorySource`, and wrappers — a filtering source and an asynchronously mapping one | Only `ManifestSource` has a byte form, its manifest. `InMemorySource` overrides `materialize` (free for one batch) |
| `RecordStream` | the stream `ManifestSource::stream` returns, and the `record_stream` adapter | Nothing else needs one: a combinator chain is re-wrapped |
| `RecordStreamExt` | `BoxRecordStream` | `materialize(max_rows)` |
| `ChunkResolver` | `ContextResolver<E>`, `EnvResolver<E>` | For `E: Environment<Value = liquers_lib::value::Value>` |
| `Stream` | `record_stream`'s adapter | By delegation; `S: Unpin` keeps it free of pin projection |
| `ExtValueInterface` conversions | `ExtValue::RecordView`, `ExtValue::RecordSource` | `from_*`/`as_*` arms, per `TYPE_SYSTEM_GUIDE.md` |
| `ValueExtension` scalar hooks | `ExtValue::RecordView` | §"A view as a value". Needs the hooks `EXTENDED-VALUES-CANNOT-BIND-TO-SCALAR-ARGUMENTS` adds |
| `DefaultValueSerializer` | `ExtValue::RecordView`, `ExtValue::RecordSource` | One-line delegations — §"Value extension" |

**No change to `AsyncStore` or `AssetManager`.** Record production is a *command* concern.

## Generic Parameters & Bounds

The value boundary is **trait objects**, not type parameters: `Arc<dyn RecordView>` and
`Arc<dyn RecordSource>` in `ExtValue`, `Arc<dyn RecordView>` as a stream's item, and
`Arc<dyn ChunkResolver>` into a source. `FieldValue` stays a dynamic enum for the same reason it
always was — a `V` parameter would infect every one of these and is circular, since `Value` is the
obvious `V`.

| Bound | Where | Why |
|---|---|---|
| `Debug + MaybeSend + MaybeSync + 'static` | supertraits of `RecordView`, `RecordSource`; `MaybeSend + 'static` of `RecordStream` | `Send + Sync` on native, vacuous on wasm; supertraits carry them to the trait object, as for `ForeignValue` |
| `T: bytemuck::Pod` | `Buffer<T>` | What makes the aligned cast safe |
| `F: Fn(usize, usize) -> Result<FieldValue, Error> + MaybeSend + MaybeSync + 'static` | `RowFnView<F>` | The generator closure lives inside a shared value |
| `F: Fn(&[Column]) -> Result<Column, Error> + …` | `with_column` | The same, for a derived column |
| `S: Stream<…> + Unpin + MaybeSend + 'static` | `record_stream` | `Box::pin` meets `Unpin` |

Generic methods appear only in inherent blocks (`impl dyn RecordView`), never in a trait, so every
trait stays object-safe.

## Function Signatures

The traits, the reference sources and `ChunkResolver` are specified in §"The types" and §"Views",
and are not repeated here.

### `liquers-records/src/lib.rs`

```rust
impl RecordSchema {
    /// Fails when more than one field has `KeyRole::Id`, when that field's role contradicts
    /// `Exact`-indexed and stored (supplied when left default), or when more than one field
    /// has `KeyRole::Source`.
    /// Checked once per schema rather than per row.
    pub fn new(fields: Vec<FieldSchema>) -> Result<Self, Error>;
    pub fn source_field(&self) -> Option<usize>;
    /// Columns whose `indexed` is `FullText` — what an unqualified text query matches.
    pub fn text_fields(&self) -> &[usize];
    pub fn index_of(&self, name: &str) -> Option<usize>;
    /// Fields whose `KeyRole` is neither `Id` nor `Source` — what scalar reading counts.
    pub fn payload_fields(&self) -> Vec<usize>;
    /// The declared `Id` field, when there is one.
    pub fn id_field(&self) -> Option<usize>;
}

impl RecordBatch {
    /// Validates column count, lengths and types against the schema.
    /// `rows` defaults to one run of chunk 0 from row 0 — a standalone table.
    pub fn new(schema: Arc<RecordSchema>, columns: Vec<Column>, chunk_id: Option<ChunkId>,
        rows: Option<Vec<RowRun>>, sources: Vec<ChunkOrigin>) -> Result<RecordBatch, Error>;
    /// Fails when schemas differ, naming the first differing field.
    pub fn concat(batches: &[RecordBatch]) -> Result<RecordBatch, Error>;
}

/// `CompareOp` is what `Column::compare` takes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompareOp { Eq, Ne, Lt, Le, Gt, Ge }

impl ChunkOrigin {
    /// Build the directly evaluable query for one row, when `locator` allows.
    pub fn locator_query(&self, id: &FieldValue) -> Option<Query>;
}

impl ManifestSource {
    /// Validates what does not depend on the key — no name collisions among explicit chunks —
    /// as `TryFrom<ManifestSpec>` does, then applies `key` when given.
    pub fn new(spec: ManifestSpec, key: Option<Key>) -> Result<ManifestSource, Error>;
    /// Supplies the key a deserialized manifest lacks: derives the template naming and the chunk
    /// ids, and validates what needs the key — per-chunk `arguments`/`links` only on keyed
    /// chunks, no collision with the template's pattern. `to_record_source`, and so every record
    /// command, calls it with the state's metadata key.
    pub fn with_key(self, key: Key) -> Result<ManifestSource, Error>;
    pub fn spec(&self) -> &ManifestSpec;
}

impl InMemorySource {
    pub fn new(views: Vec<Arc<dyn RecordView>>) -> InMemorySource;
}


impl Bitmap {
    /// `len` bits, all clear.
    pub fn new(len: usize) -> Self;
    /// The way a caller builds a mask without knowing the LSB-first packing.
    pub fn from_bools(bits: &[bool]) -> Self;
    /// Panics on an out-of-range `i`, as slice indexing does.
    pub fn set(&mut self, i: usize, value: bool);
    /// Bits, not bytes.
    pub fn len(&self) -> usize;
    pub fn get(&self, i: usize) -> bool;
    pub fn and(&self, other: &Bitmap) -> Result<Bitmap, Error>;
    pub fn or(&self, other: &Bitmap) -> Result<Bitmap, Error>;
    pub fn not(&self) -> Bitmap;
    pub fn count_ones(&self) -> usize;
    /// Positions of set bits, in order — how a mask becomes a `RowIndexView`'s indices.
    pub fn iter_ones(&self) -> impl Iterator<Item = usize> + '_;
}
```

### `liquers-records/src/buffer.rs`

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

### Construction helpers, options and the provider chain

Named in prose above and pinned here, because Phase 3's tests are written against them.

```rust
// liquers-records/src/manifest.rs
/// The generated tail of a manifest. Chunk `i` (a global index, at least the number of explicit
/// chunks) is `<query>-<offset>-<batch_size>` with `offset = first_offset + step × i`, followed by
/// the chunk's key filename when the manifest is keyed (`ChunkNaming::key(i)`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChunkTemplate {
    pub query: String,
    #[serde(default)]
    pub first_offset: u64,
    pub step: u64,
    pub batch_size: u64,
}
impl ChunkTemplate {
    pub fn query_at(&self, index: u64, filename: Option<&str>) -> Result<Query, Error>;
    pub fn offset_at(&self, index: u64) -> u64;
}

// liquers-records/src/formats/shapes.rs
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JsonOrient { Records, List, Split, Values, Columns, Index, Table, Auto }
impl std::str::FromStr for JsonOrient { type Err = Error; /* "records", "list", …, "auto" */ }
/// `Auto` is refused for writing.
pub fn to_json(view: &dyn RecordView, orient: JsonOrient) -> Result<serde_json::Value, Error>;
pub fn from_json(value: &serde_json::Value, orient: JsonOrient, schema: ReadSchema<'_>)
    -> Result<RecordBatch, Error>;

// liquers-records/src/formats/mod.rs — `header` defaults to true, so `Default` is written by
// hand: a derived one would make it false.
#[derive(Debug, Clone)] pub struct ReadOptions  { pub header: bool }
#[derive(Debug, Clone)] pub struct WriteOptions { pub header: bool }
impl Default for ReadOptions  { fn default() -> Self { ReadOptions  { header: true } } }
impl Default for WriteOptions { fn default() -> Self { WriteOptions { header: true } } }
impl TableFormat {
    /// `csv`, `csv:comma`, `tsv`, `csv:tab`, `ndjson`, `jsonl`, `json`, `md`, `markdown`, `html`,
    /// `feather`/`ipc`/`arrow_ipc`/`arrow`, `parquet`. Anything else is refused, naming the format.
    pub fn from_data_format(data_format: &str) -> Result<TableFormat, Error>;
}

// liquers-records/src/schema.rs
impl FieldSchema {
    /// Nullable, `KeyRole::None`, the default role, and `label` = `name` with `_` → space.
    pub fn new(name: impl Into<String>, data_type: FieldType) -> Self;
    pub fn with_label(self, label: impl Into<String>) -> Self;
    pub fn with_description(self, description: impl Into<String>) -> Self;
    pub fn with_key(self, key: KeyRole) -> Self;
    pub fn with_role(self, role: FieldRole) -> Self;
    pub fn not_null(self) -> Self;
}
impl FieldRole {
    pub fn text() -> Self; pub fn keyword() -> Self; pub fn stored_only() -> Self;
    pub fn numeric() -> Self; pub fn vector(metric: VectorMetric) -> Self; pub fn ignored() -> Self;
    pub fn and_stored(self) -> Self; pub fn and_fast(self) -> Self;
}

// liquers-lib/src/records/convert.rs
#[derive(Debug, Clone, Default)]
pub struct ToRecordOptions {
    pub format: Option<String>,             // None: from the state's metadata
    pub header: Option<bool>,               // None: true
    pub schema: Option<Arc<RecordSchema>>,
    pub max_rows: Option<usize>,            // None: 1 000 000, when a source is materialized
}

// liquers-core/src/recipes.rs
pub struct RecipeProviderChain<E: Environment> { providers: Vec<Arc<dyn AsyncRecipeProvider<E>>> }
impl<E: Environment> RecipeProviderChain<E> {
    pub fn new(providers: Vec<Arc<dyn AsyncRecipeProvider<E>>>) -> Self;
    pub fn push(&mut self, provider: Arc<dyn AsyncRecipeProvider<E>>);
}
// `impl AsyncRecipeProvider<E> for RecipeProviderChain<E>`: the first provider answering
// `Some` wins; `assets_with_recipes` concatenates, de-duplicated by key.

// liquers-web/src/records.rs — the Rust side of the handle
impl From<Arc<RecordBatch>> for LiquersRecordBatch { /* the live handle count rises by one */ }
impl Drop for LiquersRecordBatch { /* and falls by one */ }
/// Live `LiquersRecordBatch` handles — `RUNTIME05`'s idiom, applied to batches.
#[cfg(feature = "debug-handles")]
pub fn live_batch_handle_count() -> usize;
```

A column descriptor (`LiquersRecordBatch::column`) is a plain object with `kind`, `ptr` (a byte
offset into linear memory), `len` (in elements) and `validity` (a descriptor of the same shape, or
`null`).

## Integration Points

### Crate placement: a `liquers-records` crate, with glue in `liquers-lib`

```
liquers-core  ←  liquers-records  ←  liquers-lib (feature `records`)  ←  liquers-axum / liquers-web / liquers-py
```

**Records are their own crate, depending on `liquers-core` only.** The reason is modularity for
what gets built *on* records: a relational access layer, the search design's engine sinks, a future
GIS layer. Each of those is naturally a small crate, and the question is what it must depend on:

| A crate built on records depends on | Crates pulled in |
|---|---|
| `liquers-lib`, even with every default feature off — `image`, `resvg`, `usvg`, `tiny-skia` and `typetag` are mandatory there | **172** |
| `liquers-records` — `liquers-core` plus `bytemuck`; Arrow IPC and Parquet optional | **about 60** (core alone is 56) |

Counted with `cargo tree -e normal` on the current lockfile. The boundary also enforces what a
module could only ask for: `liquers-records` cannot reach egui, polars or the UI, and `liquers-lib`
reaches records only through their public API. And records work gets a small test loop,
`cargo test -p liquers-records`, which builds core and nothing above it — the build this environment
can afford to run often.

**What goes where** follows one rule: `liquers-records` holds everything that does not need
`liquers-lib`'s `Value` or its commands; `liquers-lib` holds the glue that does.

| `liquers-records` | `liquers-lib`, behind `records` |
|---|---|
| schema; `Column`, `Buffer`, `Bitmap`, `FieldValue`, `RecordBatch`; `RowId` | `ExtValue::RecordView` and `ExtValue::RecordSource`, their `TypeInfo`s, `DefaultValueSerializer` arms and scalar hooks |
| the traits `RecordView`, `RecordViewMut`, `RecordSource`, `RecordStream`, `ChunkResolver`; `RecordValue` | `impl RecordValue for Value` |
| views and their constructors (`impl dyn RecordView` is legal here, where the trait is defined); column kernels; `RecordBatchMut`, `ColumnMut`; `materialize` | the `ns-rec` commands and their registration |
| readers and writers, both schema modes; Markdown, HTML; Arrow IPC and Parquet behind features | `to_record`, `to_record_source` — they take `Value`, `Metadata` and `Context` |
| `ManifestSpec`, `ManifestSource`, `InMemorySource`, `ChunkNaming`, `ManifestRecipeProvider`; `ContextResolver`, `EnvResolver` | adding the manifest provider to the environment's chain; the polars bridge (it needs both crates) |
| — | `pub use liquers_records::*;` **inside** the glue module `liquers_lib::records`, so `liquers_lib::records::RecordBatch` works for every crate above `liquers-lib`. Not also `pub use liquers_records as records;` at the crate root — that name is the glue module's, and the two would clash (E0255) |

**Commands stay in `liquers-lib`**, as the `rec` namespace should: they are registered with
`register_command!` against `liquers-lib`'s `Value`. The cost is that a command and the function it
calls are in different crates — a command is a thin wrapper, so this is a small price for keeping the
records crate free of the command layer.

| Crate | File | Change |
|---|---|---|
| `liquers-records` | `src/buffer.rs` | `AlignedBuffer`, `Buffer<T>`, `Bitmap` — the Arrow-layout primitives; the only place `bytemuck` is used |
| `liquers-records` | `src/lib.rs` | The traits `RecordView`, `RecordSource`, `RecordStream`, `ChunkResolver`; `BoxRecordStream`, `record_stream`, `RecordStreamExt`; `FieldValue`, `Column` and its kernels, `CompareOp`, `RecordBatch`, `RowId`, `RowRun`, `ChunkOrigin`, `LocatorRule`, `ChunkId`, `ChunkList`, `ChunkDescriptor` |
| `liquers-records` | `src/schema.rs` | `RecordSchema`, `FieldSchema`, `FieldType`, `FieldRole`, `KeyRole` and their construction helpers |
| `liquers-records` | `src/manifest.rs` | `ManifestSpec` and its version handling, `ChunkTemplate`, `ChunkNaming` |
| `liquers-records` | `src/views.rs` | The view implementations, the `impl dyn RecordView` constructors, `RowFnView` |
| **`liquers-core`** | `src/recipes.rs` | `RecipeProviderChain`; `RecipeProviderChoice` unchanged; `Recipe` gains `stored` and `cached` as `Option<bool>`, absent meaning `true`, with `stored()`/`cached()` accessors |
| **`liquers-core`** | `src/metadata.rs` | `MetadataRecord` and `AssetInfo` gain `stored` and `cached` the same way — `Option<bool>`, because both derive `Default` |
| **`liquers-core`** | `src/assets.rs` | the store writes skip when `stored: false`; key-asset registration skips when `cached: false` |
| **`liquers-core`** | `src/context.rs`, `src/environment_builder.rs` | `with_appended_recipe_provider` on `GenericEnvironment` and `EnvironmentBuilder` |
| `liquers-py` | wrappers of `Recipe`, `AssetInfo`, `MetadataRecord` | the two fields, or `..Default::default()` in struct literals |
| `liquers-records` | `src/provider.rs` | `ManifestRecipeProvider` |
| `liquers-records` | `src/value.rs` | `RecordValue`, `ChunkValue` |
| `liquers-records` | `Cargo.toml` (new crate) | depends on `liquers-core`, `serde` (with `derive` and `rc`), `serde_json`, `serde_yaml`, `futures`, `chrono`, `bytemuck`, `async-trait`, `scc`; features `ipc` (`dep:flatbuffers`) and `parquet` (`dep:flate2`) — Phase 4 Step 2.1 has the full block |
| `liquers-lib` | `src/records/mod.rs` (new) | `pub use liquers_records::*`, `impl RecordValue for Value`, adding `ManifestRecipeProvider` to the environment's chain |
| workspace | `Cargo.toml` | `liquers-records` as a member and in `default-members` |
| `liquers-records` | `src/mutable.rs` | `RecordViewMut`, `RecordBatchMut`, `ColumnMut` |
| `liquers-lib` | `src/records/convert.rs` (new) | `to_record`, `to_record_source` — what every record command accepts |
| `liquers-records` | `src/formats/` | `mod.rs` (`read_table`, `write_table`, `ReadSchema`, `TableFormat`, the options), `csv.rs` (CSV and TSV), `ndjson.rs` (NDJSON and the `json` format), `shapes.rs` (the JSON orients), `markdown.rs`, `html.rs`, `infer.rs` (schema-less inference); `ipc.rs` behind the crate's `ipc` feature; `parquet.rs` and `thrift.rs` behind its `parquet` feature |
| `liquers-records` | `src/sources.rs` | `ManifestSource`, `InMemorySource`, the wrapping sources, `ContextResolver`, `EnvResolver` |
| `liquers-lib` | `src/records/commands.rs` (new) | The `ns-rec` command set |
| `liquers-lib` | `src/records/polars.rs` (new, `records` + `polars`) | `RecordBatch → polars::DataFrame` over the shared buffers, and `DataFrame → RecordBatch` — the path Parquet is read through |
| `liquers-lib` | `src/value/mod.rs` | `ExtValue::RecordView` and `ExtValue::RecordSource`, **cfg-gated**, with every exhaustive match gaining a gated arm; both `TypeInfo` entries; the `DefaultValueSerializer` arms; the scalar hooks |
| `liquers-lib` | `src/value/extended.rs` | **Prerequisite, not records-specific:** `ValueExtension` scalar hooks, delegated from `CombinedValue`'s `ValueInterface` and `TryFrom` impls — `EXTENDED-VALUES-CANNOT-BIND-TO-SCALAR-ARGUMENTS` |
| `liquers-lib` | `Cargo.toml` | the `records`, `records-ipc` and `records-parquet` features, forwarding to `liquers-records` |
| `liquers-web` | `src/records.rs` (new) | The `RecordBatch` handle, per-column descriptors, `columnCopy`; the JS companion that revalidates typed-array views |
| `liquers-web` | `Cargo.toml` | add `"records"` to the `liquers-lib` feature list |
| `liquers-axum` | `src/axum_integration.rs`, `src/query/handlers.rs` | Streaming for `RecordSource` + `csv`/`ndjson`: `Body::from_stream`, eager first batch, uniform-schema check. **Not as a record-specific branch** — `liquers-axum` cannot name `ExtValue` — but through the core-level hook of `VALUE-SERIALIZATION-IS-SYNCHRONOUS-AND-WHOLE-VALUE`, which this design then depends on for HTTP streaming |
| `liquers-py` | later milestone | Arrow C Data Interface export — the only place `unsafe` FFI belongs. Needs `liquers-py` to depend on `liquers-lib` (planned) or on `liquers-records` directly; today it depends on `liquers-core` only |
| `specs` | `command_registry.yaml` | Regenerated |

### Why `liquers-core` needs no stream alias

A boxed stream would ordinarily need a per-target `BoxStream` alias and a boxing helper in
`liquers-core/src/maybe_send.rs`, because `StreamExt::boxed()` is always `Send`-boxed and the
`MaybeSend` marker cannot be written as a trait-object bound (E0225). With `RecordStream` a trait
whose supertraits include `MaybeSend`, the trait object carries the right `Send`-ness on each target
by transitivity, so `Pin<Box<dyn RecordStream>>` needs no alias and `Box::pin` needs no helper.
`RecordSource` returns core's existing `BoxFuture`. The core changes this design does make are the
general ones of §"Keyed chunks".

### The features

```toml
# liquers-records/Cargo.toml
[dependencies]
liquers-core = { path = "../liquers-core" }
serde = { version = "1.0.229", features = ["derive", "rc"] }   # derives over Arc<…>
serde_json = "1.0.151"
serde_yaml = "0.9.34"                                # the manifest provider reads *.manifest.yaml
futures = "0.3.34"
chrono = "0.4.45"
bytemuck = "1.25"                                    # the aligned-buffer cast; tiny, no_std
async-trait = "0.1.92"                               # ManifestRecipeProvider
scc = "3.8.8"                                        # the provider's manifest cache
flatbuffers = { version = "…", optional = true }     # version pinned in Phase 4
flate2 = { version = "1", optional = true }          # already in the graph through png

[features]
default = []
# Arrow IPC file (Feather v2): the lossless table format.
ipc = ["dep:flatbuffers"]
# The Parquet writer. Reading Parquet goes through polars, in liquers-lib.
parquet = ["dep:flate2"]
```

```toml
# liquers-lib/Cargo.toml
[features]
default = ["egui", "image-support", "polars", "records", "records-ipc", "records-parquet"]
# Columnar record streams: a tabular value type with an Arrow-compatible layout.
records = ["dep:liquers-records"]
records-ipc = ["records", "liquers-records/ipc"]
records-parquet = ["records", "liquers-records/parquet"]

[dependencies]
liquers-records = { path = "../liquers-records", optional = true }
```

**`liquers-records` has no default features**, so a crate built on it takes only what it asks for;
`liquers-lib` forwards its three features. **In `liquers-lib`'s `default`, as `polars` is**, so the
routine `cargo test -p liquers-lib --lib --tests` exercises the glue; being in `default` is not being
mandatory, and the matrix below proves the feature is cleanly optional.

**`bytemuck` and `serde/rc` are plain dependencies of the records crate** — no longer optional flags
on `liquers-lib`. `bytemuck` is in `Cargo.lock` at 1.25.2 but is a direct dependency of no workspace
crate today (it arrives through `egui`), so a build without `records` still adds nothing. `serde`'s
`rc` feature is declared where it is needed, instead of being relied on through feature unification
— which, in a `--no-default-features` build of `liquers-lib`, does not enable it.

### Feature-gating discipline

A cfg-gated enum variant is the classic way to break a build that was not tested, and `ExtValue`
already carries the scar tissue — its `as_bytes` match has a comment recording that a previous
catch-all "silently absorbed new variants". Every exhaustive match on `ExtValue` therefore needs a
`#[cfg(feature = "records")]` arm, in `type_name`, `type_identifier`, `as_bytes`,
`type_descriptions` and the `ExtValueInterface` conversions.

`scripts/check-build-matrix.sh` gains the rows that prove it, mirroring the existing per-feature rows:

```
-p liquers-records --tests                                      # the crate alone, no features
-p liquers-records --features ipc,parquet --tests
--target wasm32-unknown-unknown -p liquers-records              # the records crate in the browser
--no-default-features --features records --tests
--no-default-features --features records,polars --tests     # the polars bridge
--no-default-features --features webui,records --tests
--no-default-features --features records-ipc --tests
--no-default-features --features records-parquet --tests        # writer only; reading refused
--no-default-features --features records-parquet,polars --tests # reading through polars
--target wasm32-unknown-unknown --no-default-features --features webui,records
--target wasm32-unknown-unknown --no-default-features --features webui,records,records-ipc
```

and the existing `--no-default-features --tests` row already proves the build with `records` **off**.
Most records tests live in `liquers-records` and need no gate. Test files in `liquers-lib` that need
the glue carry `#![cfg(feature = "records")]` at file level, as the existing optional-dependency test
files do.

### What this settles, and what it costs

This also places **the search design's predicate**: `SearchPredicate` evaluates against a `RecordView`
through the column kernels, so it depends on `liquers-records` and nothing more. The search design's
engine sinks (Tantivy, Qdrant) and the relational access layer (`NO-RELATIONAL-DATABASE-ACCESS-LAYER`)
can each be a small crate over `liquers-records`, which is the
modularity this placement is for.

The costs, stated: one more crate to version and keep in the dependency-flow line of `CLAUDE.md`;
the `RecordValue` adapter, because the records crate cannot name `liquers-lib`'s `Value`; and commands
living in a different crate from the functions they wrap.

## Streaming a record source over HTTP (`liquers-axum`)

> **Superseded as a mechanism by `VALUE-SERIALIZATION-IS-SYNCHRONOUS-AND-WHOLE-VALUE` (2026-09-24).**
> `liquers-axum` depends on `liquers-core` and `liquers-store` only, and its handlers are generic over
> `E: Environment`, so it cannot name `ExtValue::RecordSource` and the branch below cannot live in
> `axum_integration.rs` as written. The behaviour specified here — eager first batch, the uniformity
> check before a CSV header, NDJSON's in-band error, backpressure — stands; it is delivered by a
> core-level asynchronous serialization hook that a record source implements, with nothing
> record-specific in `liquers-axum`. That issue inventories the handlers the pattern changes.
>
> **Until then, a source is served over HTTP by materializing it** — `/q/…/ns-rec/materialize/daily.csv`
> goes through the ordinary `BinaryResponse` path, with no change to `liquers-axum`, bounded by
> `max_rows`. A plain `/q/…` of a manifest-backed source returns its manifest.

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

A **view** needs none of this. It is finite and in memory, so `as_bytes` serves it through the normal
flow; only a source reaches the streaming branch.

```rust
// liquers-axum — sketch
let source: Arc<dyn RecordSource> = /* from the evaluated value */;
let resolver: Arc<dyn ChunkResolver> = Arc::new(EnvResolver::new(envref)); // records no dependencies
let mut stream = source.stream(resolver).await?;       // fails BEFORE headers, see below
let schema = stream.schema();                          // CSV header, and the uniformity check
let first = stream.next().await.transpose()?;          // pull one batch eagerly

// `stream` is 'static — it owns the source's Arc and the resolver — so the body may outlive
// this handler.
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
header otherwise. So the handler checks `RecordStream::schema()` *before* sending headers and
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
| **Concepts** | The three traits and *why* they are three: `RecordSource` (asked repeatedly, shareable, async), `RecordStream` (one traversal, never a value), `RecordView` (a finite table, sync) — and `RecordBatch`, the materialized view. The conversion table with costs. The `Iterable`/`Iterator` analogy stated once, plainly. The placement rule: asynchronous work is a source |
| **Views** | The built-in views and what `column_range` costs on each; why views do not cache; column selection keeping the key columns; scalar reading of a single cell and its conversion table; which commands materialize and why |
| **Scales** | Record / batch / chunk — unit of retrieval, of memory, of refresh — and why conflating them breaks either memory or refresh |
| **Schema** | `RecordSchema`, `FieldSchema`, `FieldType`, `FieldRole`. The two orthogonal axes and which integration target each serves. The exactly-one-`Id` rule and that it is checked in `RecordSchema::new` |
| **Field naming** | The `meta.` / `attr.` / `key.` qualification, why `status` forced it, and that ambiguity is an error naming every candidate |
| **Identity and retrieval** | `(chunk id, record id)`. `ChunkOrigin`: `chunk` as the guaranteed path, `locator` as the direct one, `info` absent for a non-asset source. **Why a CSV row has no `AssetInfo` and the file does** |
| **Provenance and validity** | The `Metadata` per chunk; provenance as "the query and dependency versions this came from"; validity as the existing staleness check; the flyweight to record level |
| **Memory layout** | The columnar form, `Column` variants, `Bitmap`'s three uses, `AlignedBuffer` and 64-byte alignment. **Cites [COLUMNAR] per claim** |
| **Arrow interoperability** | The two-level model — data as `&[T]`, structure rebuilt as `repr(C)` — the exact type/format mapping table, the three export routes and their real costs, and the three places the layout is not 1:1. **Cites [COLUMNAR] and [CDATA]**; states that format strings are verified against the spec, not this document |
| **Browser sharing** | Hazards A and B, the identity check, the refresh rule, read-only views, the handle lifetime, and `columnCopy` as the fallback |
| **Methods** | Every public method with its contract and failure mode — the three traits, the `impl dyn RecordView` constructors, `Column`'s kernels, `RecordSchema::{new, id_field, source_field, text_fields, index_of, payload_fields}`, `RecordBatch::{new, concat}`, `Bitmap::{new, from_bools, set, len, get, and, or, not, count_ones, iter_ones}`, `ChunkOrigin::locator_query`, `record_stream`, the three `materialize`s |
| **Table formats** | The format table with what each round-trips; the null convention of CSV; the inference rules and the `Id` rule; labels in presentation formats; HTML escaping; which features bring which formats |
| **Serialization** | What each value writes in each format; that a source writes only its manifest, and every other source nothing, re-derived from its recipe; that rows reach bytes through `materialize`, with its limit and its refusal of non-uniform sources |
| **Limits** | The Arrow subset supported and what is excluded; uniformity not promised and the two operations that need it; the feature gate |

Links out to `VALUE_TYPE_SYSTEM.md`, `STORE_SEMANTICS.md` for the key semantics it inherits, and the
design folder for *why*.

### Guide: `specs/guides/RECORD_STREAM_GUIDE.md`

`kind: guide` · audience contributor · area `lib/value` · workflow **"produce records from a new
source"**.

| § | Content |
|---|---|
| **Choose your shape first** | A decision table: a small table → return a view (usually a batch); a directory of files → a manifest source; a huge single file → a source whose stream yields batches; anything asynchronous per row → a wrapping source. Getting this wrong is the expensive mistake, so it comes first |
| **Walkthrough: a command producing records** | **The guide's spine.** End to end, from an empty file to a passing test: define the schema (roles, and an `Id` field when rows have one), build the batch with `RecordBatchMut`, fill `ChunkOrigin` so results are retrievable, return `ExtValue::RecordView`, then register with `register_command!` — `context` last, `async fn` taking owned `State` — and regenerate `command_registry.yaml` |
| **Second walkthrough: a manifest source** | The CSV-directory case: one query per file, what `uniform_schema` to declare, and why a manifest is preferred over a generator (rewindable, cacheable, checkpointable) |
| **Choosing a batch size** | Rows vs bytes, and the memory arithmetic |
| **Using views as a DataFrame** | `select_columns`/`filter`/`slice`/`with_column`/`concat` with masks; when to `materialize`; what is deliberately absent and where it lives instead |
| **Writing a view** | Implementing `column_range`; `ColumnMut`; `RowFnView` for a generator; the equivalence test against the provided defaults |
| **Handing a batch to pandas or polars** | The polars path via `polars-arrow`; the pyo3 path; what "zero-copy" does and does not cover |
| **Reading a chunk from JavaScript** | The handle, the view-refresh rule, and when to reach for `columnCopy` |
| **Pitfalls** | The `len + 1` offsets invariant; `Vector` needing a child node; forgetting the `TypeInfo` entry (the type then cannot be stored); forgetting a `#[cfg(feature = "records")]` match arm (a build with the feature off fails); holding a JS view across a wasm call |
| **Testing** | Unit tests beside the code; the round-trip test per format; the `debug-handles` release assertion; the build-matrix rows |

Every snippet is taken from a real test in the implementation, so the guide cannot drift from
behaviour without a test failing.

### Existing documents to update

| Path | Change |
|---|---|
| `specs/reference/VALUE_TYPE_SYSTEM.md` | The `RecordView` and `RecordSource` identifiers, their `TypeInfo`s, that both are feature-gated, and that each holds a trait object |
| `specs/guides/TYPE_SYSTEM_GUIDE.md` | Both variants in the worked list; the gated-variant case as a worked example, since it is the first optional value type after `polars` |
| `specs/guides/COMMAND_REGISTRATION_GUIDE.md` | A pointer to the record-producing walkthrough rather than a duplicate of it |
| `specs/README.md` | The capability-map entry, `designing` → `built` |
| `specs/guides/LANGUAGE-INTEGRATION_GUIDE.md` | **Already updated** — VALUE's third bridging category (lent buffers), RECIPE's corrected listing/containment rule, and the RECORDS subsection, revised 2026-09-24 for the trait form: a batch crosses as Arrow, a view as a wrapper or materialized |
| `CLAUDE.md` | `liquers-records` in the project-structure list and the dependency-flow line (`liquers-core ← liquers-records ← liquers-lib`); "Where Code Goes" gains records (data model, formats, manifests in `liquers-records`; `ns-rec` commands in `liquers-lib`); `cargo test -p liquers-records` beside the default test command; the `records` features in the feature-matrix section with the new rows. Written when the crate exists, since `CLAUDE.md` describes HEAD |

| `specs/reference/ASSETS.md`, `ASSET_LIFECYCLE.md` | `stored` and `cached`: what each skips, that a stored copy is still preferred, that both false is not volatile |
| `specs/reference/ENVIRONMENT_CONFIG.md` | The recipe provider chain and how a provider is appended |
| `specs/reference/PROJECT_OVERVIEW.md` | Recipe providers as a chain; keyed record chunks as one generative provider |

**Discarded candidates:** `STORE_SEMANTICS.md`, `STORE_IMPLEMENTATION_GUIDE.md` and
`CONFORMANCE_TERMS.md` — this design touches no store trait. The `get_asset_info` repair in
`ASSETS.md` belongs to the search design.

`affects_docs`: `reference/RECORD_STREAMS.md`, `guides/RECORD_STREAM_GUIDE.md`,
`reference/VALUE_TYPE_SYSTEM.md`, `guides/TYPE_SYSTEM_GUIDE.md`,
`guides/COMMAND_REGISTRATION_GUIDE.md`, `guides/LANGUAGE-INTEGRATION_GUIDE.md`,
`reference/ASSETS.md`, `reference/ASSET_LIFECYCLE.md`, `reference/ENVIRONMENT_CONFIG.md`,
`reference/PROJECT_OVERVIEW.md`.

## Relevant Commands

Deliberately thin: this design owns the *mechanism*, and each consumer brings its own producers. The
names mirror the `pl` namespace's (`select_columns`, `head`, `slice`), so the two tabular vocabularies
read alike.

| Command | Signature | Returns | Purpose |
|---|---|---|---|
| `rec_id` | `async fn rec_id(state, id: String, context) -> result` | a **materialized** one-row batch | **The single-record selector.** The one record whose `Id` field matches. Needs a declared `Id` — without one it refuses, naming `rowid`, which addresses every row. On a source it walks chunks until it finds the row |
| `row` | `fn row(state, n: i64) -> result` | a materialized one-row batch | A row by position — views only; a source has no stable positions |
| `select_columns` | `fn select_columns(state, columns: Vec<String> multiple) -> result` | a view | Projection. Keeps the `Id` (when declared) and `Source` columns whether named or not |
| `head` | `fn head(state, n: i64 = 5) -> result` | a materialized batch | The first rows, for inspection |
| `slice` | `fn slice(state, offset: i64, length: i64) -> result` | a view | A row range |
| `rowid` | `async fn rowid(state, chunk: i64, row: i64, context) -> result` | a materialized one-row batch | A row by its implicit id. Over a source it opens **only** that chunk |
| `to_record_source` | `async fn to_record_source(state, format: String = "", context) -> result` | a `RecordSource` | Any input a source can be made from — §"What a record command accepts" |
| `materialize` | `async fn materialize(state, max_rows: i64 = 1000000, context) -> result` | a `RecordBatch` | **The one step from a source to a table**, and so to bytes: `…/ns-rec/materialize/daily.csv`. Refused past `max_rows` and for a non-uniform source. On a view, `materialize()` — freezes it and releases a pinned base |
| `records_schema` | `fn records_schema(state) -> result` | a value | The schema — how an agent discovers field names, and a value a `schema` argument can be linked to |
| `to_json` | `fn to_json(state, orient: String = "records") -> result` | a JSON value | A view as one of the seven JSON shapes of §"JSON shapes are conversions" |
| `from_json` | `fn from_json(state, orient: String = "auto", schema) -> result` | a `RecordBatch` | A JSON value, text or bytes as a table; `auto` detects the shape and refuses the ambiguous one |
| `to_record` | `async fn to_record(state, format: String = "", header: bool = true, schema, context) -> result` | a `RecordView` | Any input a table can be made from — §"What a record command accepts". Schema-aware when `schema` is given |

Namespace `rec`, written `ns-rec` in a query. `rec_id` and `materialize` are `async` and take
`context`, **last**, because over a source they open a stream with a `ContextResolver` — which also
makes the chunks dependencies of the result. The others take a view. A command receiving a source
where it needs a view refuses rather than collecting silently, and its error names
`ns-rec/materialize` — `rec_id` and `materialize` are the ones that walk a source, because that is
their purpose. Every record command converts its input with the same two helpers as `to_record` and
`to_record_source`, below.

#### What a record command accepts

Two async helpers turn any reasonable input into a table or a source, and every `ns-rec` command
uses them on its state; `to_record` and `to_record_source` are the same helpers as commands, for
converting explicitly.

```rust
pub async fn to_record(value: &Value, metadata: &Metadata, options: &ToRecordOptions,
    context: &Context<impl Environment<Value = Value>>) -> Result<Arc<dyn RecordView>, Error>;
pub async fn to_record_source(value: &Value, metadata: &Metadata, options: &ToRecordOptions,
    context: &Context<impl Environment<Value = Value>>) -> Result<Arc<dyn RecordSource>, Error>;
```

| Input | `to_record` → a `RecordView` | `to_record_source` → a `RecordSource` |
|---|---|---|
| a `RecordView` | itself | an `InMemorySource` of it |
| a `RecordSource` | **refused**, naming `materialize` — turning a source into a table costs, and the cost is written in the query | itself |
| **bytes or text** | parsed as a table in `csv`, `tsv`, `json`, `ndjson`/`jsonl` — the format from the `format` argument, else from the state's metadata; schema-aware when a `schema` is linked | a **manifest** when the document carries `manifest: record-stream` (its folder, from the metadata key, is the `cwd`); otherwise parsed as a table and wrapped |
| a JSON value (array or object) | `from_json` with `orient = auto` | a manifest when it carries the discriminator; otherwise as for `to_record` |
| a **key** | the keyed asset is fetched from the asset manager; a `RecordView` or `RecordSource` value is taken as such, anything else is converted by the rows above | the same |
| anything else | a conversion error naming what was received | the same |

- **A key needs the asset manager, so only the async commands accept one** — which is why both
  helpers take `context`. The view commands that are synchronous (`select_columns`, `slice`, `head`,
  `row`, `to_json`) accept values, including bytes and text, but not keys.
- **The manifest is recognized by its `manifest: record-stream` discriminator**, not by its file
  name, so a manifest stored under any key — or produced by a command — is read as one.
- **The format is never sniffed.** A text value with no format in its metadata and none given is an
  error, rather than a guess between CSV and NDJSON that is right most of the time.

**`schema` is optional and linked.** `from_json` and `to_record` take it through a recipe's `links:` — a
`records_schema` result, or a YAML/JSON schema document — and pass it to the schema-aware reader.
How the macro spells an optional value-typed argument is a Phase 4 detail to check against
`register_command!`, not assumed here.

**There are no `records_to_csv` / `records_to_ndjson` commands.** Serialization is chosen the Liquers
way, by the filename that ends the query — `…/ns-rec/materialize/daily.csv`, `…/daily.ndjson` — so a
format-per-command set would be a second vocabulary for the same thing. Producers (`ns-search/records`, a CSV
projection, a parquet projection) are owned by the designs that need them.

**Which commands materialize** follows §"A small view keeps its whole base alive": those whose result
is bounded by a count the caller chose — `rec_id`, `row`, `head` — copy their few rows and release the
base; `select_columns` and `slice` return views.

**`rec_id` is deliberately in the record namespace rather than a projection's.** It works on any
record source or view, so a projection does not have to supply its own selector — and a projection
that can do better supplies a `locator` instead of a competing command.

**A cell is a chain, not a command.** `…/ns-rec/rec_id-42/select_columns-price` is one row and one
payload column, so it reads as a scalar (§"A view as a value").

## Error Handling

All errors are `liquers_core::error::Error` via typed constructors. No `Error::new`, no new error
type, no `unwrap`/`expect`.

| Situation | Outcome |
|---|---|
| A schema with more than one `Id` field | `Error::general_error` from `RecordSchema::new` |
| `rec_id` on a source or view whose schema declares no `Id` | `Error::general_error` naming `rowid` as the alternative |
| An `Id` field that is not `Exact`-indexed and stored | `Error::general_error` — it could not be reconciled by delete-by-term |
| Column count, length or type disagrees with the schema in `RecordBatch::new` | `Error::general_error` |
| `concat` or `materialize` of batches with different schemas | `Error::general_error` naming the first differing field |
| A mask whose length differs from the view, or an index past its end | `Error::general_error` |
| `column_range` or `value` out of range | `Error::general_error` — never a panic |
| `select_columns` naming a field the schema does not declare | `Error::general_error` naming the field and listing the available ones |
| `ColumnMut::push` of a value of the wrong type | `Error::general_error` |
| A scalar read of a view that is not one row by one payload column | `Error::conversion_error`, naming the row count and the payload columns |
| `materialize` passing `max_rows` | `Error::general_error` stating the limit and how to raise it — the stream is dropped, not truncated silently |
| A view command receiving a source | `Error::conversion_error`, naming `ns-rec/materialize` |
| `to_record_source` given a document that is not a manifest | `Error::general_error` from `TryFrom<ManifestSpec>` |
| Colliding explicit chunk names | `Error::general_error` naming them, at load |
| Per-chunk `arguments`/`links` on an unkeyed chunk, or an explicit name matching the template's pattern | `Error::general_error` naming the chunk, when the key is supplied (`with_key`) or at the latest when a stream is opened |
| A format the build does not include (`feather` without `records-ipc`, …) | `ErrorType::SerializationError` naming the feature — and the registry does not advertise it |
| Reading `parquet` without `polars`, or `html` at all | `ErrorType::SerializationError` naming why |
| An IPC file with dictionary batches, compression, 64-bit offsets or unsupported nesting | `ErrorType::SerializationError` naming what was found |
| A Parquet write of a `Vector` column | `ErrorType::SerializationError` — `LIST` needs repetition levels |
| Malformed CSV — an unterminated quote, rows of differing length | `ErrorType::SerializationError` with the line number |
| Schema-aware read: a cell that does not parse as its declared type | `ErrorType::SerializationError` naming row, column and value |
| Schema-aware read: a column the schema does not declare, or a missing non-nullable one | `ErrorType::SerializationError` naming the column |
| A computed chunk whose view does not match the manifest's `uniform_schema` | `Error::general_error` naming the first differing field |
| `from_json-auto` on an object of objects | `Error::general_error` — `columns` or `index`, stated |
| `as_bytes` on a source with no manifest | `ErrorType::SerializationError` via `Error::from_error` — the write path stores metadata only |
| State is not an `ExtValue::RecordView` or `ExtValue::RecordSource` | `Error::conversion_error` |
| A field name no schema declares | Not an error — a `Warning` log entry on the evaluation's `Metadata` |
| Unreadable entry while producing records | Skipped, counted in an `Info` log entry |

## Sync vs Async Decisions

| Operation | Choice | Rationale |
|---|---|---|
| Every `RecordView` method and constructor, `Column` kernels | sync | In memory. Asynchronous work belongs in a source — §"Views are synchronous" |
| Scalar reading of a view | sync | Argument binding (`TryFrom<Value>`) is synchronous |
| `RecordSource::stream`, `describe_chunk`, `materialize`; `RecordStreamExt::materialize` | async | Evaluation and store access through `ChunkResolver` |
| `RecordSource::chunks`, `schema`, `manifest` | sync | What the source already holds |
| Serializing any value | sync | Which is why a source's rows go through `materialize` — §"A source serializes only as its manifest" |
| `Bitmap` operations | sync | Pure |
| Schema construction and validation | sync | Pure |
| Record-producing commands | async | Store access |
| Serialization to CSV / NDJSON | sync today | Becomes streaming once an incremental writer exists |

## Serialization Strategy

Every concrete data type derives `Serialize, Deserialize` — `RecordBatch`, `Column`, the schema
types, `ManifestSource`, `ChunkId`, `ChunkDescriptor`; `Query` uses the existing `query_format`
helper. `AlignedBuffer` and `Buffer<T>` serialize as their bytes — alignment is a memory property,
not a wire one, and is re-established on deserialization.

The traits are **not** `Serialize`: a trait object has no derive, and bytes cannot say which
implementation to rebuild. Serialization is a method at the value boundary instead — §"Value
extension" — with three outcomes:

| Value | Written as | Read back as |
|---|---|---|
| a `RecordView` of any kind | its materialized batch, in any format of §"Table formats" | a `RecordBatch` — same identifier; schema inferred for the text formats, carried for `feather` and `parquet` |
| a `ManifestSource` | the manifest, `yaml` / `json` | a `ManifestSource` |
| any other source | **nothing** — metadata only | re-derived from its recipe; its rows reach bytes through `materialize` |

A stream is never serialized and never crosses a query boundary. A `ManifestSource` serializes as a
**manifest**, whose format —
`<filename_prefix>.manifest.yaml`, mirroring `recipes.yaml`'s `arguments` and `links` — is specified
in [`manifest-format.md`](./manifest-format.md). Three of its rules bear on this design:

- **Identity has two regimes.** For an *unkeyed* stream the query is the chunk's identity, so a value
  varying per chunk must live in the query. For a *keyed* chunk — one named by its query's filename
  or by the template's convention (§"Keyed chunks") — the
  chunk's key distinguishes it, so per-chunk `arguments` and `links` are usable, exactly as in
  `recipes.yaml`. **Per-chunk `arguments` and `links` are wanted** — a manifest that merges tables
  from different sources gives each explicit chunk its own statement or connection — and they apply
  to **explicit** chunks only; a template's are shared by every chunk it generates. Used on an
  unkeyed chunk they alias silently, so they **require keyed chunks** — which is open question 18.
- **Versioning is lenient, because manifests are written by hand.** An absent or unknown `version`
  reads as the latest; an unknown field is reported as a `Warning` log entry rather than refused.
  Backward compatibility is the aim, and the format is stabilized later.
- **The explicit form is a `RecipeList`.** A chunk entry is a `Recipe` field for field, so a manifest
  is a stream header plus a recipe list — inheriting planning, arguments, links, `volatile` and
  `expires` rather than restating them.
- **No string interpolation.** A command hydrates its own statement, keeping the format free of
  templating syntax and of an injection story. `expires` bounds each chunk, and the dependency
  cascade (`dependencies.rs`) expires whatever derives from it.

## Concurrency Considerations

No shared mutable state. Buffers are `Arc`-shared and immutable once built, and a view holds only
its base `Arc` and its selection, so every view is immutable too: `select_columns`, `slice` and a
column hand-off copy nothing and are safe to share across threads, which the `MaybeSync` supertrait
makes the compiler check on native. A view that gathers does so into a fresh `Column` owned by the
caller, so concurrent readers never contend. Batch production is sequential; concurrency is
`buffer_unordered` over the chunk stream when there is something to measure. No lock is held across
an `.await`.

## Compilation Validation

- **Object safety.** `RecordView`, `RecordSource`, `RecordStream` and `ChunkResolver` have no generic
  methods and no `Self`-returning methods. `self: Arc<Self>` (`RecordSource::stream`) is an
  object-safe receiver. Generic helpers live in `impl dyn RecordView`, outside the vtable.
- **Default methods do not construct views.** Coercing `Arc<Self>` to `Arc<dyn RecordView>` needs
  `Self: Sized`, which a default body lacks — hence the inherent block. `RecordBatch`'s overrides are
  in an impl where `Self` is sized, so they may do what the defaults cannot.
- **`dyn RecordStream` is a `Stream`.** `Pin<Box<S>>` implements `Stream` for `S: Stream + ?Sized`,
  and `Stream::poll_next` takes `self: Pin<&mut Self>`, which is object-safe. The `Item` type is fixed
  in the supertrait bound, so the trait object names it.
- **`Send`-ness by supertrait, not by alias.** `MaybeSend`/`MaybeSync` are supertraits, so the trait
  objects are `Send + Sync` on native and unconstrained on wasm. They are gated on `target_arch`,
  **never** on a Cargo feature — `maybe_send.rs` documents why: feature unification would silently
  strip `Send` from the native build workspace-wide.
- **The stream is `'static`.** It owns an `Arc` of its source and an `Arc<dyn ChunkResolver>`, so
  `Body::from_stream` — which requires `'static` — accepts it.
- Both variants are `Arc`-wrapped, so `size_of::<ExtValue>()` is unchanged and `Value` is untouched.
- `ExtValue` derives only `Debug, Clone`, so the trait objects need no `Serialize`, `Deserialize` or
  `PartialEq` — the reason these variants cannot live on `Value`.
- `liquers-records` depends on `liquers-core` only, so a dependency cycle is impossible by
  construction: nothing in it can name `liquers-lib`.
- `impl RecordValue for Value` is legal in `liquers-lib`: `CombinedValue` is `liquers-lib`'s own type.
- `impl dyn RecordView { … }` is legal in `liquers-records`, the crate defining the trait (E0116
  forbids it anywhere else).
- Every exhaustive `match` on `ExtValue` has a `#[cfg(feature = "records")]` arm, so
  `--no-default-features` still compiles — the failure mode a gated enum variant causes, and what the
  new build-matrix rows exist to catch. No code matches over *implementations* of the traits.
- `liquers-core` gains no dependency and no `unsafe`; its changes are the provider chain and two
  `Option<bool>` fields per type, absent meaning `true`, so stored recipes and metadata written
  before them — and every `..Default::default()` literal — read unchanged. A plain `bool` would
  have made the derived `Default` mean "not stored".
- `bytemuck`, `flatbuffers` and `flate2` are dependencies of `liquers-records` only, so a
  `liquers-lib` build without `records` resolves an unchanged dependency graph.
- `Buffer<T>: bytemuck::Pod` holds for `i32`, `i64`, `u64`, `f32`, `f64`.
- **`serde/rc` is enabled by the `records` feature.** Deriving `Serialize` over `Arc<T>` needs serde's
  `rc` feature. In the default build some other dependency turns it on, but in
  `--no-default-features --features records` nothing does (checked with `cargo tree -e features`),
  so without the explicit feature the record types would compile in one configuration and not
  another — exactly what the build matrix exists to catch.
- **Closures are neither `Debug` nor boxable with `MaybeSend`.** `dyn Fn(..) + MaybeSend` is E0225 —
  `MaybeSend` is not an auto trait — so `DerivedColumnView<F>` and `RowFnView<F>` are generic over the
  closure and are erased only when coerced to `Arc<dyn RecordView>`. Both implement `Debug` by hand
  (the schema and length, not the closure), because `RecordView: Debug` and a derive would fail.
- **`ManifestSource` lends ids it owns.** `chunks()` returns `&[ChunkId]`, so the ids are derived once
  in `TryFrom<ManifestSpec>` rather than computed per call from the stored queries.
- `liquers-lib`'s `records` feature gates the dependency, the variants, the matches and the glue
  module together — a partially-gated feature is the classic way to break a configuration nobody
  built.

## References to liquers-patterns.md

Async-by-default — `BoxFuture` returned from object-safe trait methods, as `async_trait` would
generate; `MaybeSend`/`MaybeSync` as supertraits for wasm, following `ForeignValue`; typed error constructors;
explicit match arms with no default; `Arc` for shared payloads in `Value`; `TypeInfo` registration
as the fourth step of adding a value type; `context` last in a command signature.

## Open Questions for Phase 3

0. ~~Should record selection be a *view*?~~ — **answered by §"Views".** A view is an
   implementation of `RecordView`, not a distinct value form, and selection is a view constructor.
   Composition by merging and pushdown into a source are **out of scope** for the generic mechanism,
   which stays light; a specialized source (SQL) may offer pushdown as part of its own
   implementation. `rec_id` over a large source therefore still reads chunks until it finds the row
   — correct, and accepted. `RECORD-SELECTION-IS-EAGER-NOT-A-VIEW` is updated accordingly.

1. **Can a `RecordSource` front a relational database?** `engine-survey.md` §3 finds the read path
   fits well — sqlx's `fetch()` is already a row stream, and keyset chunking works *because* the
   schema already requires exactly one ordered unique `Id`. Three things do not fit, and the first
   is structural:
   - A SQL source needs chunk queries **generated**, not listed, since the count is unknown and
     `COUNT(*)` is expensive. [`chunking-and-resumability.md`](./chunking-and-resumability.md) works
     this through: the manifest's `template` generates them — constructed in this version, with
     template chunks keyed by `ChunkNaming` — and `ChunkList::Unbounded` is what consumers see. With `RecordSource` a trait, a
     SQL source may also be **its own implementation** — the natural home for pushdown, if it is ever
     wanted — rather than a manifest.
   - **`Decimal` stops being deferrable.** Reading a `NUMERIC` column as `Float` is a corruption bug,
     not an approximation. Also absent: `uuid`, `jsonb`, arrays, intervals.
   - **Writing is entirely undesigned.** An access layer implies `INSERT`/`UPDATE`, transactions and
     conflict handling; this design is read-only throughout.

   Filed as `NO-RELATIONAL-DATABASE-ACCESS-LAYER` rather than absorbed here.
2. ~~What is the default batch size?~~ — **answered for this version**: a stored chunk is read whole,
   so a chunk is a batch (§"Two readers"). The question returns with incremental reading. Was: a row
   count or a byte budget? A byte budget is the
   honest answer for the multi-gigabyte case but needs a size estimate per column.
3. ~~Is `with_columns` the right extension point for derived fields?~~ — **split in two**:
   `with_column` for a column computed from others (the functional-index replacement), lazily, and
   `with_columns` for precomputed ones (the search design's evidence).
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
10. ~~The `RecordBatchBuilder` append surface.~~ — **decided**: `RecordViewMut` with
    `append_row(&[FieldValue])`, `reserve` and `with_capacity`, implemented by `RecordBatchMut`;
    `ColumnMut` is the mutable column (§"Writing a view"). Was: Three Phase 3 drafts produced three APIs. The
    recommendation is `append_row(&[FieldValue])` as the primary call, with `ColumnMut` as the
    per-column path — which §"Writing a view" already introduces, so the builder may reduce to a
    schema plus one `ColumnMut` per field. To settle before Phase 4.
11. ~~Must every view carry an `Id`?~~ — **decided**: no; every row has an implicit `RowId`, and the
    explicit `Id` is optional (§"Every row has an implicit id"). Was: The invariant is kept on every view by making
    `select_columns` retain key columns. A future aggregation (`sum` → one row, one column) has no
    natural id. Not needed now — aggregation is out of scope — but the first aggregate will have to
    synthesize one or relax the invariant for derived tables.
12. **Equivalence of overrides.** Where `RecordBatch` or a view overrides a provided method (`column`,
    `value`, `materialize`), its answer must equal the default's. Phase 3 should state this as a
    property test over each built-in view, so the two paths cannot drift apart unnoticed.
13. ~~`materialize`'s default limit.~~ — **decided**: 1 000 000 rows, raised in the query. Was: 1 000 000 rows is a guess. A byte budget is the honest measure
    but needs a per-column size estimate; a row count is what can be checked while draining.
14. **Materializing a non-uniform source.** Refused today. A *union* schema — every field of every
    chunk, missing ones null, as polars' diagonal concat does — would make it possible, at the cost
    of silently widening every chunk's schema. Worth adding as an explicit option if the
    folder-of-CSVs case needs a single table.
15. **Inference strictness.** The `Date` and `Timestamp` tests turn a column of ISO strings into
    dates, which is usually wanted and occasionally not. The schema-aware reader is the opt-out —
    declare the column `Text` — so this is only about files read with no schema at all.
16. **Parquet read without polars.** Refused. If browser Parquet reading is ever needed, the choice
    is the arrow-rs `parquet` crate (large) or a reader limited to dictionary + snappy/zstd, which
    covers pyarrow's and polars' defaults — both substantial.
17. **The schema in metadata** — §"Should the schema live in metadata?". Waits on
    `CORE-METADATA-NO-APPLICATION-ATTRIBUTES` and on the load path passing metadata to a
    deserializer; then it is one more source for the same schema-aware reader.
18. ~~Making `stored` and `cached` work~~ — **decided: in this project** (§"Keyed chunks"). Explicit
    chunks are keyed by their query's filename; template chunks by `<prefix>_{n:04}.<extension>`,
    `extension` configurable with default `csv`. A manifest recipe provider in a core provider chain
    serves them; the asset manager honours the flags.

**Settled, and recorded so they are not reopened without new information:**

| Question | Answer |
|---|---|
| Which crate owns records | **`liquers-records`**, depending on core only; `liquers-lib` holds the glue (value variants, commands, conversions, polars bridge) behind `records`. Not core: the arguments for core each have a more general fix — streaming serialization, `liquers-py` depending on `liquers-lib`, provider-aware validation, extensible metadata |
| Which enum owns the value variants | `ExtValue`, because `Value`'s `Deserialize` bound is unsatisfiable for a trait object |
| Data structures or interfaces | **Interfaces.** `RecordView`, `RecordSource`, `RecordStream` are traits; `RecordBatch`, `ManifestSource`, `InMemorySource` are reference implementations |
| One value variant for tables or two | **One**, `RecordView`, over batches and views alike. `ExtValue::Image` over `DynamicImage` is the precedent |
| Names | `RecordView` (trait and value), `RecordBatch` (materialized struct) — not `RecordChunk`, which would collide with the chunk as unit of refresh |
| The required read of a view | `column_range(col, rows)` — the one method that gives cell, window and column reads each their proportional cost |
| Where kernels live | On `Column`, so views and batches share them |
| Whether views cache | No. The consumer's `Column` is the cache; `materialize()` is the explicit one |
| Merging stacked views; pushdown | Out of scope for the generic mechanism |
| Keyed chunks | In this project: explicit chunks keyed by their filename, template chunks by `<prefix>_{n:04}.<extension>`; a manifest recipe provider in a core provider chain; `stored`/`cached` honoured by the asset manager |
| Asynchronous transformations | Sources, never views — views stay synchronous because scalar reading and argument binding are |
| How a source becomes bytes | As its manifest only. Its rows go through `materialize`, a command, because serialization is synchronous and evaluation is not |
| How a source reaches evaluation | `ChunkResolver`, an object-safe trait; `ContextResolver` records dependencies, `EnvResolver` does not |
| Whether a `ChunkedRecordSource` trait is needed | No — `chunks()` is the partition as data. A SQL source is simply another `RecordSource` implementation |
| Whether a `RecordSet` type survives | No — a view or a source, not a third name |
| How far the DataFrame surface goes | `select_columns`/`filter`/`slice`/`with_column`/`concat`; group-by and join are a query engine |
| Whether a stream can be rewound | Not applicable — re-open the source instead |
| Whether roles can differ per engine | No per-engine maps. A field declares the access paths it **affords**; each engine projects |
| Whether functional indexes (soundex) are roles | No — they are derived columns |

## Changelog

The design reached this shape through eight rounds of review. Recorded because the *reversals* carry
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

| 2026-09-24 | **Interfaces, not data structures.** `RecordSource`, `RecordStream` and a new `RecordView` became traits; `SourceBacking` became two implementations, `ManifestSource` and `InMemorySource`. The value variants hold trait objects | Views, on-the-fly filters and generated tables were enum variants inside this module rather than something a command author can write |
| 2026-09-24 | **`RecordView` and `RecordBatch`**: one trait for every finite table, one struct for the materialized one. **One** value variant, `RecordView`, over both; `RecordChunk` retired as a name | A second variant would put a storage detail into the type system, and a stored view would read back as a different type. "Chunk" is the unit of refresh |
| 2026-09-24 | The required read of a view is **`column_range`** | A cell-first contract makes columns slow; a column-first one makes a window over a computed view O(n); requiring both leaves two methods that must agree |
| 2026-09-24 | Kernels moved from `RecordBatch` to **`Column`**; no caches inside views; no merging and no pushdown | Views then share the fast path. The aim is a light, flexible mechanism, not a query engine |
| 2026-09-24 | **`ChunkResolver`** replaces `Context<impl Environment>` in `stream()` and `describe_chunk()`, with a context-backed and an environment-backed implementation. `stream` takes `Arc<Self>` and returns a `'static` stream | A generic method cannot be called on `dyn RecordSource`; an HTTP body outlives its handler; a stream outliving its command must not record dependencies |
| 2026-09-24 | **Scalar reading**: a one-row, one-payload-column view reads as its cell would as a base `Value`. `select_columns` keeps key columns. Commands bounded by a caller's count materialize | The user's requirement that pointing at a cell yield a value; a tiny view must not pin a large base |
| 2026-09-24 | **Asynchronous work is a source.** Views stay synchronous; source → view is an explicit await | Scalar reading and argument binding are synchronous |
| 2026-09-24 | `liquers-core` **untouched** — the `BoxStream` alias removed | `MaybeSend` as a supertrait gives the trait object the right `Send`-ness on each target |
| 2026-09-25 | **`ManifestSpec` models the manifest-level fields** of `manifest-format.md` §4 — the `manifest` discriminator (`ManifestKind`), `version`, `title`, `description`, shared `arguments` and `links`, `volatile`, `expires` — and gets a hand-written `Default`. A manifest written by Liquers now carries its discriminator, so it is recognized when read back as plain YAML | Phase 4 final review: a template chunk could not receive the SQL statement or connection its query needs, and the provider had no `expires`/`volatile` to copy. Follows the user's decision that a template's arguments and links are shared by its chunks |
| 2026-09-25 | **The surface Phase 3's tests needed, pinned.** `Bitmap` construction (`new`, `from_bools`, `set`, `len`); `ChunkTemplate`, `JsonOrient` with `to_json`/`from_json`, `ReadOptions`/`WriteOptions` with a hand-written `Default`, `TableFormat::from_data_format`, the `FieldSchema` builders and `FieldRole` constructors, `ToRecordOptions`, `RecipeProviderChain::{new, push}`; the `liquers-web` handle's `From`, `Drop`, live-handle count and descriptor fields. Module layout gains `schema.rs` and `manifest.rs`; the formats file is `ndjson.rs`; the format files are gated on the crate's own `ipc`/`parquet` features | Phase 3 approval; its §"What Phase 3 found that Phase 2 must absorb" |
| 2026-09-25 | **Review pass** — Rust review and two independent reviewers (Phase 1 conformity, codebase alignment). Fixed: `stored`/`cached` as `Option<bool>` (the three types derive `Default`, so a `bool` would default to "not stored"); the `records` re-export clashing with the glue module; a stored manifest losing its key — key-dependent validation moved to `with_key`; `RecordBatch.rows` and `RowRun`'s derives; `FieldSchema`'s hand-authoring defaults; `RecordViewMut::len` with uneven columns; the provider's `async_trait` attributes; stale `rec_id` and `select_columns` rows | Review before re-approval |
| 2026-09-25 | **Records become their own crate, `liquers-records`**, depending on core only; `liquers-lib` keeps the glue — `ExtValue` variants, `ns-rec` commands, `to_record`/`to_record_source`, the polars bridge — behind `records`, forwarding `records-ipc` and `records-parquet` to the crate's `ipc` and `parquet`. A `RecordValue` adapter trait lets the crate read and build `liquers-lib`'s `Value` without naming it; `ChunkResolver::evaluate` returns a `ChunkValue`. Moving records into core was assessed and rejected | Modularity for crates built on records — about 60 dependencies instead of `liquers-lib`'s 172 — a boundary that enforces layering, and a small test loop. Each argument for core has a more general fix: streaming serialization, `liquers-py` depending on `liquers-lib`, provider-aware validation, extensible metadata |
| 2026-09-25 | **Keyed chunks, in this project.** Explicit chunks are recipes keyed by their query's filename; template chunks are named `<prefix>_{n:04}.<extension>` from the manifest's own name, `extension` default `csv`; the template is constructed in this version. `ManifestRecipeProvider` in a new core `RecipeProviderChain`; `stored`/`cached` on `Recipe`, `MetadataRecord` and `AssetInfo`, honoured by the asset manager. `ChunkKeys` replaced by the derived `ChunkNaming`; `number_format` dropped. `liquers-core` is no longer untouched | The user chose option 1: `stored` and `cached` must work, and per-chunk arguments need keys |
| 2026-09-25 | **Decisions for Phase 3.** `RecordViewMut` / `RecordBatchMut` / `ColumnMut` with capacity replace the builder. Every row has an implicit `RowId { chunk, row }` and, during a traversal, a row number; the explicit `Id` becomes optional and the schema-less reader stops guessing one; `rowid` addresses a row by reading one chunk. `to_record` / `to_record_source` accept views, sources, bytes, text, JSON values and keys, replacing `parse` and `source`. A stored chunk is read whole. Manifest: per-chunk `arguments`/`links` on explicit chunks, `stored`/`cached` per manifest, unknown versions read as the latest | The user's answers to the open decisions |
| 2026-09-25 | **Two readers, and JSON shapes as commands.** A schema-aware reader (declared types, strict, one pass) beside the schema-less one; manifests with `uniform_schema` parse stored chunks from their bytes and check computed ones. `to_json`/`from_json` over seven orients, pandas-compatible, `table` lossless; `parse`. The `json` format keeps one shape. Schema-in-metadata discussed and deferred | A manifest's declared schema must be usable when reading CSV and NDJSON chunks; JSON has too many table shapes for a format name to choose; pandas interop |
| 2026-09-24 | **Table formats specified.** `records`: CSV, TSV, NDJSON, JSON, Markdown, HTML, hand-written with no new dependency; `records-ipc`: Arrow IPC / Feather, lossless; `records-parquet`: a minimal Parquet writer, reading only through polars. Text formats infer a schema, with an `Id` rule and writers putting the `Id` first | A `RecordView` must round-trip through CSV and NDJSON at least; wasm size keeps heavier formats behind features. `deserialize_from_bytes` sees no metadata, so the text formats cannot carry a schema |
| 2026-09-24 | **A source serializes only as its manifest; its rows need `materialize`** — an async method on `RecordSource`, an extension on `BoxRecordStream`, and the `ns-rec/materialize` command, bounded by `max_rows`. `ns-rec/source` added; `records_to_csv` / `records_to_ndjson` removed; `InMemorySource` lost its byte form; `collect_view` renamed | Producing a source's rows needs awaits and serialization is synchronous. Moving the await into a command needs no change outside `liquers-lib`, and settles when a source is written as data: only when asked |
| 2026-09-24 | HTTP streaming of a source moved from an axum branch to `VALUE-SERIALIZATION-IS-SYNCHRONOUS-AND-WHOLE-VALUE` | `liquers-axum` does not depend on `liquers-lib` and is generic over `E: Environment`, so it cannot name `ExtValue::RecordSource` |
| 2026-09-24 | `records` enables `serde/rc`; `ManifestSource` serializes through `ManifestSpec`; closure-holding views are generic with a hand-written `Debug` | A Rust review of the trait form: `Arc` fields fail to derive `Serialize` in a minimal build; `chunks()` could not borrow ids from a `Vec<Query>`; `dyn Fn + MaybeSend` is E0225 |
| 2026-09-25 | **Phase 4 review.** `RecipeProviderChoice` is unchanged; the chain is appended in code (`with_appended_recipe_provider`, `LibKind`'s default) — §B and the Integration Points rows. §C's sites corrected against `assets.rs`: the metadata saver (`save_metadata_to_store`) is a `stored` write site; `cached: false` acts where the `get(key)` path registers (`get_nonvolatile_resource_asset`, `ImmediateAssetManager::get_resource_asset`), not at `try_insert_key_asset`; the flags are taken when the manager creates the keyed asset, not in `resolve_volatility_before_evaluation`, which runs before the provider's recipe replaces the ad-hoc one. The records crate's Cargo block gains `derive`, `serde_yaml`, `async-trait` and `scc`. Two unbalanced code fences repaired (`ChunkValue`, `RowFnView`) | Checking the plan's claims against the code: a `Choice` is configuration data and cannot name another crate's provider; a `stored: false` chunk would otherwise leave a metadata-only entry; the manifest provider parses YAML and implements an `#[async_trait]` trait |

**Corrections worth keeping visible**, because each was stated wrongly first:

- `bytemuck` was described as free because it is in `Cargo.lock`. It is a direct dependency of no
  workspace crate, arriving transitively via `egui`.
- "Zero-copy" was applied to the whole Arrow export. It is true of the data and false of the
  description, which costs O(columns) allocations.
- The browser route was called fragile without checking whether invalidation is detectable.
- `Value::Recipe` was cited as having 17 occurrences; it has 27, 8 of them in `value.rs`.
- Phase 2 placed the Arrow C Data Interface export in `liquers-py`, which depends on `liquers-core`
  only and so could not reach `RecordBatch`. Consistent once `liquers-py` depends on `liquers-lib`,
  which is the plan.
- The record types derived `Serialize` over `Arc` fields without enabling serde's `rc` feature. It
  compiled in the default build only because another dependency enables it.
- `FieldValue` had no `Date` variant although `Column` and `FieldType` did, so a single-cell read of a
  date column had nothing to return. Added.
- The value-extension section said neither variant was feature-gated while its own code gated both —
  left over from before the `records` feature. Both are gated.


**Open external checks:** Arrow format strings against [CDATA], and Tantivy/Lucene/Qdrant API names
against the crate versions in use. Both are stated from the specifications rather than verified in
this repository, and both fail loudly at the boundary if wrong.
