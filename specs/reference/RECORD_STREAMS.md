---
title: Record Streams
kind: reference
audience: internal
area: [records, lib/value, lib/commands, web]
reviewed: 2026-09-27
---

# Record Streams

How Liquers holds, moves, stores and addresses tabular record data: the `liquers-records` crate
(data model, views, sources, manifests, table formats, the manifest recipe provider) and its glue in
`liquers-lib` (`src/records/`, the `ExtValue` variants in `src/value/mod.rs`) and `liquers-web`
(`src/records.rs`, `src/records_column.js`).

This document states what is true at HEAD. *Why* it is this shape is in
[`design/record-streams/`](../design/record-streams/DESIGN.md), especially
[Phase 2](../design/record-streams/phase2-architecture.md) and the implementation log in
[`phase5-evidence.md`](../design/record-streams/phase5-evidence.md). How to *build* on it — a
command that produces records, a manifest source, a view — is in
[`RECORD_STREAM_GUIDE.md`](../guides/RECORD_STREAM_GUIDE.md). The value-type identifiers are part of
[`VALUE_TYPE_SYSTEM.md`](VALUE_TYPE_SYSTEM.md); the asset-manager side of `stored` / `cached` is in
[`ASSETS.md`](ASSETS.md); chunk keys inherit the key semantics of
[`STORE_SEMANTICS.md`](STORE_SEMANTICS.md).

## Crates and features

| Crate | Holds | Features |
|---|---|---|
| `liquers-records` | Everything below that does not need `liquers-lib`'s `Value`. Depends on `liquers-core` only. **No `unsafe`.** | `ipc` (Arrow IPC read/write, pulls `flatbuffers`), `parquet` (Parquet write, pulls `flate2`). Neither is default |
| `liquers-lib` | `ExtValue::RecordView` / `RecordSource`, `to_record` / `to_record_source`, the `ns-rec` commands, the polars bridge, the provider-chain default. `liquers_lib::records` re-exports all of `liquers-records` | `records` (the glue), `records-ipc` → `liquers-records/ipc`, `records-parquet` → `liquers-records/parquet`. All three are default |
| `liquers-web` | The `RecordBatch` JavaScript handle | `records`, forwarded, default |

With `records` off, neither value variant exists and no `ns-rec` command is registered.

## Concepts

Three traits, one struct:

| Name | Kind | Is | Sync? | Analogy |
|---|---|---|---|---|
| `RecordSource` | trait (`value.rs`) | Something that can be asked, **repeatedly**, for a traversal. Shareable (`Arc<dyn RecordSource>`), never consumed by use | async (`stream`, `describe_chunk`, `materialize`) | `Iterable` |
| `RecordStream` | trait (`batch.rs`) | **One** traversal: a `futures::Stream` of `Result<Arc<dyn RecordView>, Error>` plus `schema()`. Never a value | async | `Iterator` |
| `RecordView` | trait (`batch.rs`) | A **finite table** with random access | **sync** | a slice |
| `RecordBatch` | struct (`batch.rs`) | The **materialized** view: columns in Arrow layout. The one form data rests in | sync | `Vec` |

**Placement rule:** a view is synchronous; anything that must `.await` (evaluate a query, read a
store) is a source. A view never becomes asynchronous; a source reaches a table only through an
explicit await (`materialize`).

### Conversions

| From → To | How | Cost |
|---|---|---|
| view → source | `InMemorySource::new(vec![view])` | free (an `Arc` clone) |
| source → stream | `RecordSource::stream(self: Arc<Self>, resolver)` | async; nothing read until polled |
| stream → view | poll the stream | one chunk evaluated or read per item |
| stream → batch | `RecordStreamExt::materialize(stream, max_rows)` | reads every chunk; `RecordBatch::concat` |
| source → batch | `RecordSource::materialize(self, resolver, max_rows)` | as above; `InMemorySource` holding one view overrides it (no stream) |
| view → batch | `RecordView::materialize()` | a shallow clone for a `RecordBatch`; reads every column for any other view |
| batch → view | `Arc<RecordBatch>` as `Arc<dyn RecordView>` | free |
| batch ↔ mutable | `RecordBatch::into_mut()` / `RecordBatchMut::freeze()` | a move of every value buffer the batch holds alone; a copy of shared ones and of validity |

### Scales

| Scale | Unit of | Type | Identity |
|---|---|---|---|
| record (row) | retrieval | a row of a view | `RowId { chunk, row }`, optionally a declared `Id` field |
| batch | memory | `RecordBatch` | — |
| chunk | refresh and storage | one query or one key; one asset | `ChunkId` |

Every source in the tree yields **exactly one view per chunk**, so today a chunk is also a batch.
A chunk is what the asset manager caches, stores and invalidates; keeping it separate from the row
is what lets one changed file refresh without rereading the others, and keeping it separate from
the whole stream is what bounds memory to one resident chunk during a walk.

## Schema

`schema.rs`. Two orthogonal axes per field — `data_type: FieldType` (what Arrow, polars and SQL
consume) and `role: FieldRole` (what a search index consumes) — plus a structural `key: KeyRole`.

| Type | Fields / variants |
|---|---|
| `RecordSchema` | `fields: Vec<FieldSchema>`, `type_identifier: Option<String>` (Liquers' type of the thing the rows describe), plus a private cached index of full-text fields |
| `FieldSchema` | `name`, `label` (default: `name` with `_` → space, as `ArgumentInfo`), `description`, `data_type`, `nullable` (default `true`), `key`, `role` |
| `FieldType` | `Bool`, `Int` (i64), `UInt` (u64), `Float` (f64), `Text`, `Binary`, `Date` (days since epoch, i32), `Timestamp` (µs since epoch, i64), `Vector` (fixed-width f32) |
| `KeyRole` | `Id`, `Source` (index into the batch's origin dictionary), `None` (default) |
| `FieldRole` | `indexed: Vec<IndexKind>`, `stored: bool`, `fast: bool`. Constructors `text()` (FullText, `Simple`, positions), `keyword()` (Exact), `numeric()` (Range), `vector(metric)` (Similarity), `stored_only()`, `ignored()` (= `default()`); combinators `and_stored()`, `and_fast()` |
| `IndexKind` | `Exact`, `Substring`, `FullText { analyzer, positions }`, `Range`, `Similarity { metric }` |
| `Analyzer` / `VectorMetric` | `Raw`, `Simple`, `Stemming { language }`, `Named(String)` / `Cosine`, `Dot`, `Euclidean` |

Roles are **capabilities the data affords**, not instructions; nothing in the tree consumes them yet
beyond the Id rule and `text_fields()`.

**`RecordSchema::new` is the only constructor, and deserialization goes through it** (a private
`RecordSchemaSpec`), so a hand-written YAML schema is checked too. It refuses:

- a duplicate field name;
- **more than one `KeyRole::Id` field** (the one-`Id` rule: *at most* one — a schema without an `Id`
  is valid, its rows identified by `RowId`);
- more than one `KeyRole::Source` field;
- an `Id` field whose role is neither Exact-indexed-and-stored nor `FieldRole::default()`. A default
  role is **replaced** by `{ indexed: [Exact], stored: true, fast: <kept> }`.

`FieldSchema` deserializes leniently: `nullable` → `true`, `key` → `None`, `role` → default,
`label` → derived from `name`.

| Method | Returns |
|---|---|
| `id_field()` / `source_field()` | position of the `Id` / `Source` field |
| `index_of(name)` | exact-name position |
| `payload_fields()` | positions whose `KeyRole` is `None` — what scalar reading counts |
| `text_fields()` | positions whose `indexed` holds a `FullText` |
| `resolve_field(name)` | see Field naming |

### Field naming

`RecordSchema::resolve_field(name)` implements the `meta.` / `attr.` / `key.` qualification: an exact
name wins; otherwise `name` is tried under each qualifier, and more than one match is an error naming
every candidate. **Nothing calls it**: no command, view constructor or reader resolves qualified
names — `select_columns` and the readers use exact names (`index_of`). No producer emits qualified
field names.

## Views

`views.rs`. The one required read is `column_range(col, rows) -> Result<Column, Error>`; out of range
is an error, never a panic. **Views do not cache**: reading the same column of a filtered view twice
gathers twice. The caller's variable, or `materialize()`, is the cache.

| View | Built by | `column_range` cost | Row identity |
|---|---|---|---|
| `RecordBatch` | `RecordBatch::new`, readers, `freeze` | `Column::slice`: **copies** the range ([`COLUMN-SLICE-COPIES-INSTEAD-OF-SHARING-BUFFER-STORAGE`](../issues/COLUMN-SLICE-COPIES-INSTEAD-OF-SHARING-BUFFER-STORAGE.md)) | its `rows` runs |
| `ColumnsView` | `select_columns` | the base's, remapped | the base's |
| `RowRangeView` | `slice`, `row` | the base's, shifted | the base's, shifted |
| `RowIndexView` | `filter`, `take` | reads the base over `min..=max` of the selected rows, then gathers | the base row's — a filtered row keeps its original `RowId` |
| `DerivedColumnView<F>` | `with_column` | base columns pass through; the derived column calls `f` on the source columns' same range, **on every read** | the base's |
| `AppendedColumnsView` | `with_columns` | slices the stored column | the base's |
| `RowFnView<F>` | `RowFnView::new(schema, len, f)` | `f(row, col)` once per requested cell | default: chunk 0, row = position, number = position |
| `PlacedView` | `place_chunk` (non-batch input) | the base's | stamped: see Identity |
| `RecordBatchMut` | `RecordBatchMut::new` | clones and freezes the whole column, then slices | default |

A small view **keeps its whole base alive** (`Arc<dyn RecordView>`); `materialize()` is how to drop
the base. `RecordView::materialize`'s provided body reads every column and approximates provenance
as one run starting at row 0's `row_id` / `row_number` — exact for a contiguous view; `RowIndexView`
overrides it with one run per row, `PlacedView` with its own run.

### Constructors on `impl dyn RecordView`

Inherent methods on the trait object (`self: &Arc<Self>`), so they are called on an
`Arc<dyn RecordView>` — an `Arc<RecordBatch>` needs `as Arc<dyn RecordView>` first.

| Method | Result | Refuses |
|---|---|---|
| `select_columns(&[&str])` | `ColumnsView`: named columns in the order given (duplicates dropped), then the `Id` and `Source` fields **appended** if not named | an unknown name, listing those available |
| `slice(offset, len)` | `RowRangeView` | `offset + len > len()` |
| `row(n)` | `slice(n, 1)` | as `slice` |
| `cell(row, column)` | `row(row)` then `select_columns(&[column])` — so it may carry the `Id`/`Source` columns too | as both |
| `filter(&Bitmap)` | `RowIndexView`; the mask becomes indices once, at construction | a mask of another length |
| `take(&[u32])` | `RowIndexView` | an index out of bounds |
| `with_column(field, sources, f)` | `DerivedColumnView`; `f: Fn(&[Column]) -> Result<Column, Error>` gets the same row range of each source column; must return that many rows (checked on read) | a source index out of range; a duplicate field name |
| `with_columns(fields, columns)` | `AppendedColumnsView` | count, length or type mismatch; a duplicate name |
| `single_cell()` | the scalar a view reads as — see below | any other shape |

Each derived schema keeps the base's `type_identifier`.

### Scalar reading

`single_cell()` answers a `FieldValue` when the view has **one row** and either **one payload
column**, or no payload column and an `Id` field (the `Id` is the value), or no payload column and a
sole column. Otherwise it refuses, naming the row and payload-column counts.

`liquers-lib` uses it so that a one-cell `RecordView` value converts like the base value its cell
stands for (the `ExtValue` scalar hooks in `value/mod.rs`); the conversion rules are then the base
`SimpleValue`'s own:

| Cell | Read as base value |
|---|---|
| `Null` | `None` — `try_into_i64_option` / `try_into_f64_option` give `None` |
| `Bool` / `Int` / `Float` / `Text` / `Bytes` | `Bool` / `I64` / `F64` / `Text` / `Bytes` |
| `UInt` | `I64` when it fits, else a conversion error |
| `Date` | `Text`, `YYYY-MM-DD` |
| `Timestamp` | `Text`, RFC 3339 with microseconds and `Z` |
| `Vector` | `Array` of `F64` |

So an `Int` cell reads as `i64` and `f64` but not `i32`, exactly as a base `I64` does
([`VALUE-CONVERSION-CAPABILITY`](../issues/VALUE-CONVERSION-CAPABILITY.md)). This is what lets a
recipe link bind a one-cell view into a scalar argument. `try_into_json_value` gives the cell's
base JSON for a one-row view with exactly one payload column (or a single column), and otherwise the
`records`-orient array of row objects. A `RecordSource` refuses every scalar conversion.

## Identity and retrieval

### Rows

| Concept | Type | Meaning |
|---|---|---|
| implicit id | `RowId { chunk: u64, row: u64 }` | the chunk's index in the source's chunk order, and the row's position **in that chunk**. Every row has one; a table from no chunk is chunk 0 |
| row runs | `RowRun { chunk, first_row, first_number: Option<u64>, len }` | how `RecordBatch::rows` records identity once per run, not per row; run lengths sum to `len` |
| row number | `row_number(row) -> Option<u64>` | the row's position across the whole traversal, when the rows before its chunk were counted |
| explicit id | the `KeyRole::Id` field | optional; what `ns-rec/rec_id` matches |

`RecordBatch::new` with `rows: None` gives one run `{ chunk: 0, first_row: 0, first_number: Some(0) }`
— a standalone table numbers its rows by position.

**A streamed chunk is placed at its chunk index.** Both sources' streams pass each chunk's view
through `place_chunk(view, chunk_index, Some(chunk_id), Some(rows_counted_so_far))`: a `RecordBatch`
is re-stamped (shallow clone, one run, the chunk id), any other view is wrapped in a `PlacedView`.
Whatever ids the view had on its own are superseded. **A chunk read on its own** (`ns-rec/rowid`
over a source) is placed with `first_number: None`: its rows have `RowId`s but no row number.
`RecordBatch::concat` joins the runs, so a materialized multi-chunk table keeps every row's
`RowId` and number.

### Chunks

| Type | Meaning |
|---|---|
| `ChunkId::Query(Query)` | **unkeyed** chunk: identified by the query that produces it (serialized as its encoded string) |
| `ChunkId::Key(Key)` | **keyed** chunk: identified by the key it is stored under |
| `ChunkList::Known(&[ChunkId])` | every chunk is known — a diff can detect deletions |
| `ChunkList::Unbounded { computed }` | the count is unknown (a template); `computed` is the known prefix; a walk ends at the first short chunk |
| `ChunkDescriptor { id, query, metadata, origin, schema }` | full provenance for one chunk, from `describe_chunk`. `Debug + Clone` only — `Metadata` has no serde or equality ([`METADATA-LACKS-SERIALIZE-DESERIALIZE-AND-PARTIALEQ`](../issues/METADATA-LACKS-SERIALIZE-DESERIALIZE-AND-PARTIALEQ.md)) |

### `ChunkOrigin`

`RecordBatch::sources: Vec<ChunkOrigin>` is a dictionary the `KeyRole::Source` column indexes, so
origin is stored once per source rather than per row.

| Field | Meaning |
|---|---|
| `asset: Query` | the asset the rows were projected from |
| `chunk: Query` | the **guaranteed** retrieval path: evaluating it yields the rows again |
| `info: Option<AssetInfo>` | the asset's own description, when it has one — `None` for a non-asset source: a CSV row has no `AssetInfo`, the file does |
| `locator: Option<LocatorRule { namespace, command, leading_parameters }>` | an optional **direct** path. `ChunkOrigin::locator_query(&id)` builds `<asset>/-/ns-<namespace>/<command>-<leading…>-<id>` on the query AST (never by string concatenation), `None` without a locator or for a `Null` id |

`RecordBatch::concat` rebases an `Int` / `UInt` `Source` column onto the joined dictionary and
refuses any other `Source` column type. **No producer in the tree fills `sources` yet**, and
`describe_chunk` builds its origin with `info: None, locator: None`.

## Provenance and validity

A chunk's provenance is its asset's `Metadata` — query, version, dependency records, status,
`updated` — returned by `RecordSource::describe_chunk` through the resolver's `metadata(query)`:
the store's metadata when the store holds the key, otherwise the chunk is **evaluated** to obtain
it (including a keyed chunk not yet produced). Validity is the asset manager's existing staleness
check on that asset; records add no second mechanism. At row level identity is a flyweight:
`chunk_id` once per batch, origins once per source, `RowRun`s once per run.

When a command walks a source through `ContextResolver`, each chunk it evaluates is recorded as a
dependency of the command's asset — including a keyed chunk read straight from the store, whose
dependency is recorded by hand with the stored version — so the materialized result is invalidated
when a chunk changes. `EnvResolver` (for a stream that outlives its request) records nothing.

## Sources, manifests and keyed chunks

### `RecordSource`

| Method | Contract |
|---|---|
| `stream(self: Arc<Self>, resolver: Arc<dyn ChunkResolver>)` | a fresh, `'static` traversal; callable any number of times |
| `chunks()` | sync, no I/O |
| `describe_chunk(&id, &dyn ChunkResolver)` | async; reads metadata |
| `schema()` | the schema every view will have, **when declared** — default `None` |
| `truncated()` | the producer stopped early — default `false` |
| `manifest()` | the manifest, the only byte form a source has — default `None` |
| `materialize(self, resolver, max_rows)` | provided: open a stream and drain it with `RecordStreamExt::materialize` |

`ChunkResolver` is what a source needs from the environment: `evaluate(query) -> ChunkValue`,
`metadata(query)`, `read_resource(key) -> (bytes, metadata)`. `ChunkValue` is `View(..)`,
`Source(..)` or `Bytes { data, metadata }` (any other value, serialized in its own format).
`ContextResolver<E>` and `EnvResolver<E>` implement it for `E::Value: RecordValue`; `RecordValue` is
the adapter by which `liquers-records` reads and builds `liquers-lib`'s `Value` without naming it.

| Implementation | Chunks | Byte form |
|---|---|---|
| `ManifestSource` | the manifest's explicit chunks, then its template | its `ManifestSpec` |
| `InMemorySource` | one per view, ids are placeholders (`in_memory_view-<n>`) never evaluated; `schema()` is `Some` only when every view's schema is equal | none |

A chunk that evaluates to a nested `RecordSource` is refused.

### Manifest format

A manifest is a YAML (or JSON) document; `ManifestSpec` in `manifest.rs`. Full field-by-field
specification: [`manifest-format.md`](../design/record-streams/manifest-format.md).

```yaml
manifest: record-stream        # discriminator; always written
version: 1                     # optional; absent, unknown or not a whole number reads as latest
title: Daily sales
chunks:                        # explicit chunks, in order: recipes.yaml's own Recipe type
  - query: "-R/raw/sales/jan.csv/-/ns-rec/to_record/jan.csv"   # keyed by its filename, jan.csv
template:                      # optional generated tail
  query: "-R/db/orders/-/ns-mydb/page"
  first_offset: 0              # default 0
  step: 1000
  batch_size: 1000
extension: csv                 # of generated chunk keys, and their stored format; default csv
stored: true                   # default true
cached: true                   # default true
uniform_schema: { fields: [ … ] }   # optional; a RecordSchema
arguments: {}                  # shared by every keyed chunk
links: {}
volatile: false
expires: …                     # as in recipes.yaml
```

| Field | Rule |
|---|---|
| `chunks` | `Vec<Recipe>`. A chunk whose query ends in a filename is **keyed** by `<manifest folder>/<filename>`; otherwise it is unkeyed and identified by its query |
| `template` | chunk *i* (a **global** index, starting after the explicit chunks) is `<query>-<first_offset + step × i>-<batch_size>`, followed by `/<key filename>` when keyed. The walk ends at the first template chunk with fewer than `batch_size` rows — or at the first error, which is yielded. The template command must therefore return a short or empty chunk past the end, not an error |
| naming | a template chunk's key is `<folder>/<prefix>_{n:04}.<extension>`, `prefix` = the manifest's filename without `.manifest.yaml`. `ChunkNaming::index_of` accepts only the canonical spelling (`daily_00042.csv` is not chunk 42) |
| `uniform_schema` | stored chunk bytes are parsed **with** it (schema-aware reader); a computed chunk's view is **checked** against it: same field count, names and types in order, and no nulls in a field declared not null. Without it, bytes are read schema-less and `schema()` is `None` |
| `stored`, `cached`, `volatile`, `expires` | copied onto every chunk recipe; see below |

`ChunkTemplate::offset_at` saturates rather than overflowing. `ChunkTemplate::query_at` parses the
rendered text, so a malformed template query fails when a template chunk is first rendered (a
stream reaching the template, or a provider lookup) — not when the manifest is loaded.

### Load-time checks

| Refused | When |
|---|---|
| two explicit chunks keyed by the same filename | `ManifestSource::new`, hence also deserialization |
| a template with `batch_size: 0` or `step: 0` (the walk would never end) | `ManifestSource::new`, hence also deserialization |
| an explicit chunk name that matches the template's naming pattern | `with_key` |
| per-chunk `arguments` / `links` on an explicit chunk whose query has no filename; shared `arguments` / `links` when any explicit chunk has no filename | `with_key` |
| a key with no filename | `with_key` |
| **any** per-chunk or shared `arguments` / `links` on a manifest with **no key** — every chunk of a keyless manifest is unkeyed, so they could not be applied | `to_record_source` (liquers-lib) and `ManifestSource::stream` |
| a chunk name that the folder's `recipes.yaml` also defines | `ManifestRecipeProvider`, when it is about to answer |

### Keyed and keyless manifests

A manifest has a key — and its chunks can be keyed — only when it was read from a store key whose
filename ends in **`.manifest.yaml`**. `to_record_source` applies the state's metadata key under
that condition only; a manifest stored as `data/sales.yaml`, returned by a command, or read from an
un-suffixed file is **keyless**, and every one of its chunks (template included) is an unkeyed query.
A `RecordSource` value deserialized from bytes is always keyless first; `to_record_source` re-keys
it. Serialization writes only `ManifestSpec`; the key and derived ids are never written.

How a stream reads one chunk (`ManifestSource::read_chunk`):

| Chunk | Read |
|---|---|
| keyed, and the store holds the key | `resolver.read_resource(key)` — the stored bytes, **not** through the asset manager, parsed in the format the metadata names (falling back to the key's extension) with `uniform_schema` when declared. An existing stored copy is preferred even under `stored: false` |
| keyed, store answers `KeyNotFound` | the key is evaluated as a resource query; the provider supplies the chunk recipe with the manifest's flags |
| unkeyed | its query is evaluated |

### `ManifestRecipeProvider`

`provider.rs`. Serves recipes for chunk keys, so `-R/data/sales/daily_0042.csv` is evaluable from
anywhere a `recipes.yaml` entry would be, not only through the source.

| Lookup | Behaviour |
|---|---|
| a name shaped `<prefix>_<digits>.<ext>` | checks exactly one file, `<folder>/<prefix>.manifest.yaml`; serves the template chunk when `index_of` matches and the index is past the explicit chunks |
| otherwise (or no template match) | scans every `*.manifest.yaml` in the folder for an explicit chunk with that filename |
| `contains` | `recipe_opt(..).is_some()` — never enumerates |
| `assets_with_recipes` | explicit chunks only; generated names are addressable but not listed |

| Chunk recipe | Fields |
|---|---|
| template | manifest `title`, `description`, `arguments`, `links`; `cwd` = the folder; `volatile`, `expires`, `stored`, `cached` from the manifest |
| explicit | the chunk's own recipe, `cwd` = the folder, the manifest's `arguments` / `links` merged **under** the chunk's (the chunk wins), and `volatile`, `expires`, `stored`, `cached` **replaced** by the manifest's |

Caches: parsed manifests by key, re-validated against the store's metadata version on each hit (a
store that reports no version re-reads every time); and each folder's list of manifest names, **for
the provider's lifetime**
([`MANIFEST-PROVIDER-FOLDER-LISTING-NEVER-REFRESHES`](../issues/MANIFEST-PROVIDER-FOLDER-LISTING-NEVER-REFRESHES.md)).
A store's `KeyNotFound` or `KeyNotSupported` for a manifest or folder means "no manifest here".

**Provider chain.** With `records`, `LibKind::default_recipe_provider()` is
`RecipeProviderChain::new([DefaultRecipeProvider, ManifestRecipeProvider])`: a `recipes.yaml` entry
wins. A build that replaces the base provider (`with_recipe_provider`,
`with_recipe_provider_choice`) loses the manifest provider unless it calls
`RecordsRecipeProvider::with_records_recipe_provider()`; no configured-construction path in the tree
calls it yet. Chain semantics are documented on `RecipeProviderChain` (`liquers-core/src/recipes.rs`)
and in [`ENVIRONMENT_CONFIG.md`](ENVIRONMENT_CONFIG.md).

### `stored` and `cached`

`Recipe`, `MetadataRecord` and `AssetInfo` carry `stored: Option<bool>` and `cached: Option<bool>`
(`None` = `true`); a manifest sets both on every chunk recipe. `stored: false` writes neither the
value nor a metadata-only entry, but an existing stored copy is still read and preferred.
`cached: false` evaluates for the request and does not register the asset for reuse. Both false is
**not** volatile. The asset-manager contract, and the dependency-graph behaviour of an uncached
keyed asset, are in [`ASSETS.md`](ASSETS.md).

## Memory layout

`buffer.rs`, `column.rs`. A `Column` is laid out as Arrow specifies, so an export can hand over
pointers.

| Type | Layout |
|---|---|
| `AlignedBuffer` | bytes in an `Arc<Vec<AlignedChunk>>`, `AlignedChunk` = `#[repr(C, align(64))] [u8; 64]` (bytemuck `Pod`, no `unsafe`): the start is **64-byte aligned** and storage is padded with zeros to a 64-byte multiple; the logical length is kept separately. Clone shares; immutable through the public API |
| `Buffer<T: Pod>` | a typed view over an `AlignedBuffer`; `from_slice`, `as_slice`, `as_bytes` |
| `Bitmap` | `len` bits, LSB-first per byte (Arrow's layout). Three uses: **validity** (set = valid), **filter masks**, and **`Bool` storage**. Bits past `len` are ignored by every operation, including equality and `count_ones` |

`Bitmap` API: `new(len)` (all clear), `from_bools`, `from_bytes(bytes, len)` (never fails; short
input reads as clear), `set(i, v)` (**panics** out of range — the crate's only deliberate panic),
`get(i)` (`false` past the storage), `len`, `is_empty`, `and` / `or` (error on length mismatch),
`not`, `count_ones`, `iter_ones`, `as_bytes`.

| `Column` variant | Storage | Arrow |
|---|---|---|
| `Bool { validity, values: Bitmap }` | bit-packed | `Bool` |
| `Int { validity, values: Buffer<i64> }` | | `Int64` |
| `UInt { validity, values: Buffer<u64> }` | | `UInt64` |
| `Float { validity, values: Buffer<f64> }` | | `Float64` |
| `Text { validity, offsets: Buffer<i32>, data: AlignedBuffer }` | `len + 1` offsets, contiguous UTF-8 | `Utf8` |
| `Binary { validity, offsets: Buffer<i32>, data: AlignedBuffer }` | as `Text` | `Binary` |
| `Date { validity, values: Buffer<i32> }` | days since 1970-01-01 | `Date32` |
| `Timestamp { validity, values: Buffer<i64> }` | µs since the epoch | `Timestamp(Microsecond)` |
| `Vector { validity, dim, data: Buffer<f32> }` | `dim × len` contiguous values | `FixedSizeList(Float32, dim)` |

`validity: Option<Bitmap>` is `None` when no row is null. A null slot's value bytes are zero (fixed
width) or empty (variable width). A `Vector` with `dim == 0` and rows carries its row count in its
validity bitmap. `i32` offsets cap a column's data at `i32::MAX` bytes (an error, never a wrap).

**`Column::validate`** checks that a column's parts agree: validity has exactly `len()` bits;
`Text` / `Binary` offsets are `len() + 1`, non-negative, non-decreasing and end within `data`, and a
`Text` column's bytes are valid UTF-8 split only at character boundaries; `Vector` data is exactly
`dim × len()`; a `Bool` bitmap holds storage for its length. `RecordBatch::new` runs it on every
column, and `RecordBatch` deserializes through `new`, so every reader and every deserialized batch
is validated. The fields stay `pub`, so code can break the invariants afterwards
([`RECORD-BATCH-FIELDS-CAN-BE-MUTATED-PAST-VALIDATION`](../issues/RECORD-BATCH-FIELDS-CAN-BE-MUTATED-PAST-VALIDATION.md)).

**Kernels** live on `Column`, so every view shares them: `len`, `is_empty`, `data_type`, `get(i)`
(error out of range), `slice`, `take(&[u32])`, `filter(&Bitmap)`, `compare(op, &FieldValue) ->
Bitmap` (a null row never matches; `Binary` and `Vector` refuse; the value must match the column's
type), `null_mask`, `concat` (all the same variant). `FieldValue` is the scalar: `Null`, `Bool`,
`Int`, `UInt`, `Float`, `Text(Arc<str>)`, `Bytes(Arc<[u8]>)`, `Date(i32)`, `Timestamp(i64)`,
`Vector(Arc<[f32]>)`. `CompareOp` is `Eq`, `Ne`, `Lt`, `Le`, `Gt`, `Ge`.

Serde: a `Buffer<T>` serializes as a readable array of numbers, an `AlignedBuffer` as bytes; both
rebuild alignment on the way back. This serde form is not one of the table formats.

### `RecordBatch` and the mutable forms

`RecordBatch { schema, columns, len, chunk_id, rows, sources }`.

| Function | Contract |
|---|---|
| `RecordBatch::new(schema, columns, chunk_id, rows: Option<_>, sources)` | refuses a column count ≠ field count, a column failing `validate`, unequal lengths, a column type ≠ its field's, runs not summing to `len` |
| `RecordBatch::concat(&[RecordBatch])` | refuses no input, a differing field count, or a differing name, type or key role at some position (named); **widens** a field to nullable when any input is; rebases the `Source` column; joins runs and origins; keeps `chunk_id` only for a single input |
| `RecordBatch::into_mut()` | a `RecordBatchMut`: value buffers held alone are moved (`Arc::try_unwrap`), shared ones copied; validity always copied |

`RecordViewMut: RecordView` — `reserve`, `append_row(&[FieldValue])` (type-checked and **atomic**: a
refused value rolls back the earlier columns of that row), `set_value`, `column_mut`.
`RecordBatchMut` implements it; its `len()` is the shortest column's, and `freeze()` requires them
equal. `freeze()` returns a **standalone** batch (chunk 0, no `chunk_id`, no origins): an edit does
not carry provenance over. `ColumnMut` (`new`, `with_capacity`, `reserve`, `push`, `set`, `len`,
`data_type`, `freeze`) is growable and aligned from the start, so `freeze` moves its value buffer;
`set` on a `Text` / `Binary` cell rebuilds the tail
([`COLUMNMUT-VALIDITY-AND-VARIABLE-LENGTH-SET-ALWAYS-COPY`](../issues/COLUMNMUT-VALIDITY-AND-VARIABLE-LENGTH-SET-ALWAYS-COPY.md)).
The first non-null vector pushed fixes a `Vector` column's `dim` and backfills earlier nulls.

## Arrow interoperability

Arrow standardizes buffers, not structs, so compatibility is at the buffer level: a `Buffer<T>` is a
contiguous little-endian `[T]` with Arrow's recommended alignment and padding, and the `Column`
variants map one-to-one onto the Arrow types in the table above. Three places need more than a
pointer:

| Where | What |
|---|---|
| `Text` / `Binary` | offsets are `len + 1` **`i32`** entries — Arrow's `Utf8` / `Binary`, not `LargeUtf8` / `Utf8View` |
| `Vector` | Arrow's `FixedSizeList` is two levels: the list node has validity only, the `f32` values are a child `Float32` array |
| `Timestamp` | no time zone is carried in the column |

| Route | Data copied | Status |
|---|---|---|
| Arrow IPC file (Feather v2) | yes — serialization | **built**, `liquers-records` `ipc` feature (see Table formats) |
| typed arrays over wasm memory | none | **built**, `liquers-web` (see Browser sharing) |
| polars `DataFrame` | yes | **built as Rust API only**: `liquers_lib::records::polars::{record_batch_to_dataframe, dataframe_to_record_batch}` (feature `polars`). No command exposes it. Roles are lost; no `Id` is guessed; `Vector` columns are refused |
| Arrow C Data Interface (`liquers-py`, polars-arrow) | none | **not built** |

## Browser sharing

`liquers-web/src/records.rs`. A `RecordView` value crossing into JavaScript is **materialized**
(`default_value.rs`) and handed over as a `LiquersRecordBatch`, exposed to JavaScript as
`RecordBatch`. A `RecordSource` has no JavaScript representation.

| Member | Returns |
|---|---|
| `numRows`, `numColumns` (getters) | counts |
| `schemaJson()` | the `RecordSchema` as JSON |
| `column(i)` | a descriptor `{ kind, ptr, len, validity }` pointing into linear memory; `dim` for `Vector`; for `Text` / `Binary` no `ptr`/`len` but `offsets` (`Int32`, `numRows + 1`) and `data` (`Uint8`) leaves. `Bool` is kind `BoolBitmap`, validity kind `Bitmap`; both with `len` in **bytes** and `rows` the bit count |
| `columnCopy(i)` | an owned copy, detached from linear memory, O(n): a typed array for numeric kinds (`Int`/`Timestamp` → `BigInt64Array`, `UInt` → `BigUint64Array`, `Float` → `Float64Array`, `Date` → `Int32Array`, `Bool` unpacked to a `Uint8Array`), an array of strings for `Text`, arrays of per-row `Uint8Array` / `Float32Array` for `Binary` / `Vector`; nulls are `null` only in the array forms |
| `columnView(i)` | a `RecordColumn` (`records_column.js`) over `column(i)` |
| `free()` | drops the handle's `Arc<RecordBatch>` |

Descriptors are built from the batch's own column storage, not from `RecordView::column` (which
copies), so a pointer stays inside a buffer the handle keeps alive. Views are read-only by
convention; nothing prevents a JavaScript write into linear memory.

| Hazard | Answer |
|---|---|
| **A** — `memory.grow` detaches the `ArrayBuffer` (pages do not move) | **refresh rule**: before each use, if `view.buffer !== memory.buffer`, rebuild the view at the same `ptr` and `len`. `RecordColumn.view` does exactly this — one reference comparison per access |
| **B** — the buffer is freed | prevented while the handle lives (it holds the `Arc`); after `free()`, a view held on is undetectably dangling — the caller's discipline. Use `columnCopy` (or `RecordColumn.toCopy()`) for anything held across an `await` or past `free()` |

`live_batch_handle_count()` (feature `debug-handles`, test-only) counts live handles.

## Table formats

`formats/`. `read_table(bytes, TableFormat, ReadSchema, &ReadOptions)` and
`write_table(&dyn RecordView, TableFormat, &WriteOptions)`. The format comes only from the data
format name (`TableFormat::from_data_format`), **never sniffed**. `ReadSchema::Declared(&schema)`
selects the schema-aware reader (strict, nothing guessed); `ReadSchema::Infer` the schema-less one.
`ReadOptions` / `WriteOptions { header: bool }` default to `true`.

| Format | Data format names | Write | Read | Round trip | Crate feature |
|---|---|---|---|---|---|
| CSV | `csv`, `csv:comma` | yes | yes | values; types inferred; roles, labels, `Id` lost; null distinct from `""` | — |
| TSV | `tsv`, `csv:tab` | yes | yes | as CSV | — |
| NDJSON | `ndjson`, `jsonl` | yes | yes | values and JSON types; roles lost | — |
| JSON | `json` — one shape: an array of row objects | yes | yes | as NDJSON | — |
| Markdown | `md`, `markdown` — GFM pipe table | yes | yes | presentation: headers are labels; types inferred; null = empty text | — |
| HTML | `html` | yes | **no** (`not_supported`) | presentation only | — |
| Arrow IPC file | `feather`, `ipc`, `arrow_ipc`, `arrow` | yes | yes | **lossless**: types, nulls, schema with roles, `chunk_id` | `ipc` |
| Parquet | `parquet` | yes | **not here** — `liquers-lib` reads it through polars | values and types; roles lost when read back | `parquet` (write) |

Without its feature, `Ipc` / `Parquet` refuse with `not_supported`, naming the feature. All writers
keep schema order; the text writers read cell by cell through `RecordView::value`, the IPC and
Parquet writers materialize the view first.

### Text cells (CSV, TSV, and the JSON renderings)

| Type | Written as |
|---|---|
| `Int`, `UInt` | decimal |
| `Float` | Rust `{:?}`: shortest round-trip form, always with a point or exponent (`1.0`, `1e300`); `NaN`, `inf`, `-inf` |
| `Bool` | `true` / `false` |
| `Date` | `YYYY-MM-DD` |
| `Timestamp` | RFC 3339, microseconds, `Z` (read: any RFC 3339 offset, converted to UTC) |
| `Text` | as is |
| `Binary` | base64, padded; read strictly — a truncated, over-padded or non-canonical value is an error |
| `Vector` | a JSON array in one cell |

**CSV** is hand-written (RFC 4180). **Null convention** (PostgreSQL `COPY … CSV`): an unquoted empty
field is null, a quoted `""` is the empty string. The writer quotes exactly when needed — separator,
quote, CR, LF, or an empty string — and ends rows with `\n`; the reader accepts `\n` and `\r\n`,
quoted newlines and doubled quotes, and **strips a leading UTF-8 BOM**. Headers are field **names**.
A row wider than the header is refused naming its line; a shorter row reads its missing cells as
null ([`CSV-ROW-NUMBERS-COUNT-RECORDS-AND-SHORT-ROWS-READ-AS-NULL`](../issues/CSV-ROW-NUMBERS-COUNT-RECORDS-AND-SHORT-ROWS-READ-AS-NULL.md)).
With `header: false` the schema-less reader names columns `col0`, `col1`, …. The schema-aware reader
matches columns to fields by header name (by position without a header), refuses an undeclared
column and a missing non-nullable field, and reads a missing nullable field as nulls. Formula
injection (`=`, `+`, `-`, `@`) is not sanitized.

### Schema-less inference

Per column, the first type that fits **every** non-null cell:

| Step | Rule (CSV / TSV / Markdown, `formats/infer.rs`) |
|---|---|
| `Bool` | `true` / `false`, any case |
| `Int` | **canonical**: parses as `i64` and formats back to the same text — so `01234`, `+5`, `1e3` and a number too large for `i64` are not `Int` |
| `Float` | a **spelling**: `-?(0\|[1-9]\d*)(\.\d+)?([eE][+-]?\d+)?` with a point or an exponent, finite — or exactly `NaN`, `inf`, `-inf` — or a canonical `Int` in a column that also holds such decimals. So `1.0`, `0.10`, `1e3` are `Float`; `+5`, `01.5`, `.5`, `5.`, `nan` are not |
| `Date`, `Timestamp` | `YYYY-MM-DD`; RFC 3339 |
| otherwise | `Text` |
| all null | nullable `Text` |
| nullable | true exactly when a null was seen |

JSON and NDJSON: JSON's types decide — a number that is an `i64` is `Int`, any other number
`Float`; only strings are tried as `Date` / `Timestamp` (so `"42"` stays text); an array of numbers
of one length in every row is a `Vector`; any other array or object is `Text` holding its JSON. The
column set is the **union** of keys over all rows, in alphabetical order
([`SCHEMA-LESS-JSON-READS-SORT-COLUMNS-ALPHABETICALLY`](../issues/SCHEMA-LESS-JSON-READS-SORT-COLUMNS-ALPHABETICALLY.md)).

Never inferred: `UInt`, `Binary` (text formats), and the **`Id` role** — a table read without a
schema has no `Id`; its rows have `RowId`s. Labels are derived from names, roles are default.

### JSON

The JSON writer refuses a non-finite `Float` or vector element (JSON has none). The schema-aware
JSON reader refuses a key the schema does not declare and coerces losslessly: a number into
`Float`, a string into `Date` / `Timestamp`, base64 into `Binary`, an array into `Vector`.

**JSON shapes are conversions**, not formats: `liquers_records::{to_json, from_json}` with
`JsonOrient` (`FromStr` from its lowercase name), exposed as `ns-rec/to_json` / `from_json`.

| Orient | Shape | Notes |
|---|---|---|
| `records` | `[{col: v, …}, …]` | = the `json` format |
| `list` | `{col: [v, …], …}` | |
| `split` | `{"columns": [...], "index": [...], "data": [[...], ...]}` | index = the `Id` values, else row positions; `columns` / `data` hold payload fields only |
| `values` | `[[v, …], …]` | every field |
| `columns` | `{col: {index: v}}` | index keys are strings; an `Id` read back is text unless a schema declares its type; all-integer keys sort numerically |
| `index` | `{index: {col: v}}` | as `columns` |
| `table` | Frictionless Table Schema + `data` | lossless; see below |
| `auto` | read-only | array of objects → `records`; array of arrays → `values`; object with `schema` + `data` → `table`; with `columns` + `data` → `split`; object of equal-length arrays → `list`; an object of objects is refused as ambiguous. `to_json` refuses `auto` |

Without a declared `Id`, the index of `split` / `columns` / `index` reads back as a plain column
named `index`. The **`table`** orient writes each field's `name`, Table-Schema `type` (`UInt` is
`integer`, `Binary` is `string` with `format: binary`, `Vector` is `array`), `title` (= label),
`description`, `constraints.required` for a non-nullable field, **`tz: "UTC"` on a `Timestamp`**, and
a **`liquers`** property `{type, key, role}` carrying what Table Schema cannot say; `primaryKey` names
the `Id`. The reader ignores the `schema` argument (the document's own schema wins), honours the
`liquers` property when present, and reads a pandas-style naive datetime as UTC.

### Markdown and HTML

**Markdown**: headers are field **labels**; `Bool`, `Int`, `UInt`, `Float` columns are right-aligned
in the delimiter row. Escaping, exactly inverted by the reader: `\` `|` `<` `&` backslash-escaped,
LF → `<br>`, CR → `&#13;`, a leading / trailing space or tab → `&#32;` / `&#9;`. The reader also
accepts hand-written escapes and entities; turns a header into a field name by lowercasing and
replacing spaces with `_` (a default label round-trips); refuses a row of the wrong width; and reads
the **first** table in the document. Markdown has no null: a null and an empty `Text` both write an
empty cell and both read back as null
([`MARKDOWN-TABLE-CANNOT-DISTINGUISH-NULL-FROM-EMPTY-TEXT`](../issues/MARKDOWN-TABLE-CANNOT-DISTINGUISH-NULL-FROM-EMPTY-TEXT.md)).

**HTML** (write-only): `<table class="liquers-records">` with `<thead>` / `<tbody>`, headers are
labels with the description as `title`, numeric cells `class="num"`, nulls `class="null"`; every
cell, label and description escapes `& < > " '`.

### Arrow IPC (Feather v2)

`formats/ipc.rs`, feature `ipc`. The writer materializes the view and writes one record batch per
file. The `RecordSchema` travels as JSON under `liquers.schema` in the Arrow schema's
`custom_metadata`, the `chunk_id` under `liquers.chunk_id` in the batch message's; `rows` and
`sources` are not written.

**Supported subset (read):** `Int(64, signed/unsigned)`, `FloatingPoint(Double)`, `Utf8`, `Binary`,
`Bool`, `Date(Day)`, `Timestamp(Microsecond)` (the zone is ignored), `FixedSizeList<Float32>`, with
any number of record-batch messages (concatenated). **Refused, by name:** dictionary encoding, a
compressed body, `LargeUtf8` / `LargeBinary`, `Utf8View` / `BinaryView`, other integer and float
widths or time units, and any other nesting. Because polars writes text only as `Utf8View` or
`LargeUtf8`, **no polars-written text column can be read**
([`IPC-READER-CANNOT-READ-ANY-POLARS-STRING-COLUMN`](../issues/IPC-READER-CANNOT-READ-ANY-POLARS-STRING-COLUMN.md)).

**The input is untrusted**: the reader is a hand-written, bounds-checked flatbuffer decoder (no
`unsafe`); every count, offset and length is checked against the bytes present before anything is
allocated or indexed, `FieldNode.length` is honoured, and every column then passes
`Column::validate`. Malformed input is an error, never a panic or an abort. **Schema used:** a
declared schema (fields matched by name; a declared nullable field the file lacks reads as nulls),
else `liquers.schema` when it still describes the Arrow fields (otherwise it is ignored and the
schema is derived from the Arrow fields), else one derived from the Arrow fields.

### Parquet

`formats/parquet.rs` + `thrift.rs`, feature `parquet`, **write-only** here: `PAR1`, one row group,
one PLAIN v1 data page per column, GZIP, min/max statistics, definition levels for nullable fields,
`liquers.schema` in key-value metadata. Types: `Bool` → `BOOLEAN`; `Int` → `INT64`; `UInt` →
`INT64` + `INTEGER(64, unsigned)` / `UINT_64`; `Float` → `DOUBLE`; `Text` → `BYTE_ARRAY` +
`STRING`; `Binary` → `BYTE_ARRAY`; `Date` → `INT32` + `DATE`; `Timestamp` → `INT64` +
`TIMESTAMP(isAdjustedToUTC: true, MICROS)`; **`Vector` is refused**.

Reading Parquet is `liquers_lib::records::read_parquet_record_batch`: polars' reader, then
`dataframe_to_record_batch` — used by `RecordView` deserialization and by `to_record`. Without
`polars` it refuses, naming the feature. It ignores a declared schema
([`RECORDS-PARQUET-POLARS-READ-IGNORES-DECLARED-SCHEMA`](../issues/RECORDS-PARQUET-POLARS-READ-IGNORES-DECLARED-SCHEMA.md))
and `liquers.schema`.

## Value types and commands

### `ExtValue` variants

| Variant | Identifier / type name | Default format, filename | Data formats |
|---|---|---|---|
| `ExtValue::RecordView { value: Arc<dyn RecordView> }` | `RecordView` / `record_view` | `csv`, `data.csv` | `csv`, `csv:comma`, `tsv`, `csv:tab`, `ndjson`, `jsonl`, `json`, `md`, `markdown`, `html`; `ipc`, `feather`, `arrow_ipc`, `arrow` with `records-ipc`; `parquet` with `records-parquet` |
| `ExtValue::RecordSource { value: Arc<dyn RecordSource> }` | `RecordSource` / `record_source` | `yaml`, `manifest.yaml` | `yaml`, `json` |

Both are gated on `records`. The web UI renders a view as an HTML table of at most 100 rows, egui as
a 20-row grid; a source renders as a one-line summary.

### What a record command accepts

Every `ns-rec` command that takes an input converts it through one of two async helpers
(`liquers_lib::records::convert`), dispatching on the value's type identifier:

| Input | `to_record` → `RecordView` | `to_record_source` → `RecordSource` |
|---|---|---|
| `RecordView` | itself | `InMemorySource` of it |
| `RecordSource` | **refused**, naming `ns-rec/materialize` | itself; a manifest source is re-keyed when the state's key ends in `.manifest.yaml` |
| `Bytes`, `Text` | read as a table in `ToRecordOptions::format`, else the metadata's data format; `schema` makes it schema-aware | a `ManifestSource` when the parsed YAML has `manifest: record-stream`, else the table in an `InMemorySource` |
| `Array`, `Object` (structured JSON) | `from_json(.., Auto, ..)` | a manifest when it has the discriminator, else as `to_record` |
| `Key` | the key's state, fetched through the context (a recorded dependency), converted recursively | the same |
| anything else | conversion error | conversion error |

### `ns-rec` commands

Thirteen commands, namespace `rec`, registered by `register_records_commands!`
(`liquers-lib/src/records/commands.rs`); signatures as in `specs/command_registry.yaml`. **Every one
is `async … context`**, because the conversion helpers are async.

| Command | Signature | Returns |
|---|---|---|
| `rec_id` | `(state, id: String)` | the first row, walking chunks in order, whose declared `Id` equals `id` — a **materialized** one-row batch. Needs the source's declared schema (`uniform_schema` for a manifest) and an `Id` field, else refuses naming `rowid`. `id` is parsed as the `Id` field's type; a `Date` or `Timestamp` id is its raw integer (days / µs); a `Binary` or `Vector` id is refused |
| `row` | `(state, n: i64)` | row `n` of a view, materialized. A source is refused |
| `select_columns` | `(state, columns: Vec<String> multiple)` | a `ColumnsView` (not materialized); keeps `Id` / `Source`; refuses an empty list |
| `head` | `(state, n: i64 = 5)` | the first `min(n, len)` rows, materialized |
| `slice` | `(state, offset: i64, length: i64)` | a `RowRangeView`, clamped to the rows that exist (a range past the end comes back short) |
| `rowid` | `(state, chunk: i64, row: i64)` | the row with that implicit id, materialized. Over a manifest source: only that chunk is opened (`describe_chunk`, evaluate, `uniform_schema` checked, placed at its index with no row number); a template chunk beyond the known prefix is refused. Over another source: the stream is walked up to that chunk. Over a view: a scan of `row_id` |
| `to_record_source` | `(state, format: String = "")` | the input as a source |
| `materialize` | `(state, max_rows: i64 = 1000000)` | every row as one batch; refused past `max_rows` or when chunk schemas differ |
| `records_schema` | `(state)` | the schema as pretty JSON **text** (so it links into a `schema` argument) |
| `to_json` | `(state, orient: String = "records")` | a structured JSON value |
| `from_json` | `(state, orient: String = "auto", schema: String = "")` | a batch; the input may be JSON, text, bytes or a key |
| `to_record` | `(state, format: String = "", header: bool = true, schema: String = "")` | the input as a view |
| `file_records` | `(state)` | one row per file directly under the state's directory key: `file_id` (`Id`, the key), `file_name`, `size_bytes`, `modified_timestamp` |

`schema` arguments are text: empty for none, else a YAML or JSON `RecordSchema` document
(`Option<Value>` cannot bind — [`REGISTER-COMMAND-OPTION-VALUE-CANNOT-BIND`](../issues/REGISTER-COMMAND-OPTION-VALUE-CANNOT-BIND.md)).
Negative counts are refused.

Queries (all validated with `liquers-validate`):

```text
-R/data/sales/daily.manifest.yaml/-/ns-rec/materialize            all rows of a keyed manifest
-R/data/sales/daily.manifest.yaml/-/ns-rec/materialize-5000000    with a raised limit
-R/data/sales/daily_0042.csv                                      one template chunk, by its key
-R/data/sales/daily.manifest.yaml/-/ns-rec/rowid-3-17             row 17 of chunk 3, opening chunk 3 only
-R/data/orders.feather/-/ns-rec/rec_id-42/select_columns-price    one cell, by Id (IPC keeps the Id)
-R/data/orders.csv/-/ns-rec/head-10                               first ten rows, schema inferred
-R/data/orders.csv/-/ns-rec/select_columns-price-qty/orders.feather   a projection, written as IPC
-R/data/orders.csv/-/ns-rec/to_json-split                         a JSON shape
```

`-R/data/orders.csv/-/ns-rec/rec_id-42` validates but is refused at run time: a CSV read without a
schema has no `Id`. A directory is listed through the `sdir` header —
`-R-sdir/data/-/ns-rec/file_records/files.csv` — which yields the store's listing and carries the
directory's key into the command; a plain `-R/data/…` asks for a value *at* the key, which a
directory does not hold, and fails.

## Serialization

| Value | Writes | Reads back as |
|---|---|---|
| `RecordView` (any view) | `write_table` in the requested format, directly from the view (no materialize, except IPC and Parquet, which materialize internally) | a `RecordBatch`: the **schema-less** reader for every text format (so roles, labels, `Id` and `UInt` are not restored); IPC with `liquers.schema`; Parquet through polars |
| `RecordSource` with a manifest | its `ManifestSpec`, as `yaml` or `json` — never its rows | a **keyless** `ManifestSource`; `to_record_source` re-keys it |
| any other `RecordSource` | refused (`SerializationError`) naming `ns-rec/materialize` | — |

A source that is not a manifest is kept as metadata only and re-derived from its recipe. Its rows
reach bytes only by `materialize` — `ns-rec/materialize`, `RecordSource::materialize` or
`RecordStreamExt::materialize` — bounded by `max_rows` (`DEFAULT_MATERIALIZE_MAX_ROWS =
1_000_000`), checked after each chunk, and refused for chunks whose field names or types differ.
Nullability differences are widened, not refused. An empty stream with a declared schema
materializes to an empty batch; without one it is an error.

## Limits

- **Uniformity is declared, not assumed.** `schema()` is `None` unless a manifest declares
  `uniform_schema` (or every in-memory view shares one); `materialize` and `rec_id` are the
  operations that need it.
- **Not built:** the Arrow C Data Interface export (and any `liquers-py` path); a command bridging
  records and polars; streaming a source over HTTP — a source reaches HTTP only materialized
  ([`VALUE-SERIALIZATION-IS-SYNCHRONOUS-AND-WHOLE-VALUE`](../issues/VALUE-SERIALIZATION-IS-SYNCHRONOUS-AND-WHOLE-VALUE.md),
  [`VALUE-SERIALIZATION-HAS-NO-INCREMENTAL-WRITER`](../issues/VALUE-SERIALIZATION-HAS-NO-INCREMENTAL-WRITER.md));
  filtering / mapping source wrappers ([`RECORD-SOURCE-WRAPPERS-UNSPECIFIED`](../issues/RECORD-SOURCE-WRAPPERS-UNSPECIFIED.md));
  predicate pushdown into a source
  ([`NO-RELATIONAL-DATABASE-ACCESS-LAYER`](../issues/NO-RELATIONAL-DATABASE-ACCESS-LAYER.md)); use
  of `FieldRole` by any index; producers that fill `ChunkOrigin` / `LocatorRule`; qualified field
  names outside `resolve_field`.
- **Arrow subset:** 64-bit integers, `Float64`, 32-bit-offset `Utf8` / `Binary`, `Bool`, `Date32`,
  `Timestamp(µs)`, `FixedSizeList<Float32>`. No dictionaries, compression, large or view types,
  decimals, times, durations, lists, structs or maps.
- **Time zones:** a `Timestamp` is a UTC instant in the text formats, `table` JSON and Parquet, but
  written naive in IPC and the polars bridge
  ([`RECORD-TIMESTAMP-TIME-ZONE-DIFFERS-BY-FORMAT`](../issues/RECORD-TIMESTAMP-TIME-ZONE-DIFFERS-BY-FORMAT.md)).
- **Interop:** polars-written IPC text is unreadable
  ([`IPC-READER-CANNOT-READ-ANY-POLARS-STRING-COLUMN`](../issues/IPC-READER-CANNOT-READ-ANY-POLARS-STRING-COLUMN.md));
  `Vector` columns cannot cross to polars
  ([`POLARS-BRIDGE-VECTOR-COLUMNS-REFUSED`](../issues/POLARS-BRIDGE-VECTOR-COLUMNS-REFUSED.md))
  or into Parquet.
- **Copies:** `Column::slice` copies, so every `column_range` on a batch copies its range; writers
  read cell by cell.
- **Values:** a null cell read as optional text gives `Some("None")`
  ([`NULL-CELL-READS-AS-THE-TEXT-NONE`](../issues/NULL-CELL-READS-AS-THE-TEXT-NONE.md));
  `InMemorySource::materialize` of one view leaks its placeholder `chunk_id`
  ([`RECORD-BATCH-FIELDS-CAN-BE-MUTATED-PAST-VALIDATION`](../issues/RECORD-BATCH-FIELDS-CAN-BE-MUTATED-PAST-VALIDATION.md)).
- **Assets:** a query on a directory key cannot be evaluated
  ([`DIRECTORY-KEY-CANNOT-BE-EVALUATED-AS-A-RESOURCE`](../issues/DIRECTORY-KEY-CANNOT-BE-EVALUATED-AS-A-RESOURCE.md));
  an uncached key's stored-copy expiry can race an in-flight evaluation
  ([`UNCACHED-STORED-COPY-EXPIRY-RACES-AN-INFLIGHT-EVALUATION`](../issues/UNCACHED-STORED-COPY-EXPIRY-RACES-AN-INFLIGHT-EVALUATION.md));
  an ad-hoc result with no filename declares a `bin` format it cannot write
  ([`FREE-FUNCTION-EVALUATE-BAKES-A-BIN-DATA-FORMAT`](../issues/FREE-FUNCTION-EVALUATE-BAKES-A-BIN-DATA-FORMAT.md)).
- **Feature gate:** everything here needs `records`; IPC needs `records-ipc`, Parquet writing
  `records-parquet`, Parquet reading `polars`. A `match` over `ExtValue` needs a gated arm for both
  variants.

## History

| Date | Change | Source |
|---|---|---|
| 2026-09-27 | PR #72 review: a directory is listed through `-R-sdir/…` (the key now carries across that header's boundary); `concat` refuses a differing key role; `Binary` base64 is read strictly. | PR #72 review |
| 2026-09-27 | Created from the implementation at HEAD, following `design/record-streams/` Phase 2's documentation contract; checked against `liquers-records`, `liquers-lib/src/records/`, `liquers-lib/src/value/mod.rs` and `liquers-web/src/records.rs`. | phase-5 |
