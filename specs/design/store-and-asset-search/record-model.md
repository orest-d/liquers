# The record model: records, streams, chunks, batches and schema

Companion to [Phase 1](./phase1-high-level-design.md) and to
[`interoperability-layer.md`](./interoperability-layer.md), which established that a record is the
common denominator between full-text search, an external engine, a vector store and SQL. This
document answers what a record actually **is**, how a stream is partitioned so that part of it can be
refreshed, and how memory is bounded when a single dependency-natural unit is far too large to
materialize.

Every query shown here was checked with `liquers-validate`.

---

## 0. The shape of the problem

The motivating usage is:

1. an external engine is configured with a Liquers query that produces a stream of records;
2. it consumes the whole stream once, at initialization;
3. a change — surfaced as an expiration event — triggers a **full or partial** update.

Step 3 forces a unit smaller than the stream. Memory forces a second one: the unit that is natural
for *dependencies* (one parquet file) may be far too large to hold at once, and the unit that is
natural for *identity* (one row) is far too small to version. The model therefore has three scales,
and keeping them distinct is most of the design.

The worked example, and it is a good one because the two query forms in it already mean exactly the
right things at HEAD:

| Role | Query | What it resolves to |
|---|---|---|
| Stream | `-R-key/some/folder/-/csv_records` | `Step::UseKeyValue` — the *key* is the value, so `csv_records` receives a key and walks the folder itself. Its dependency is the **directory**, not the file contents |
| Chunk | `-R-bin/some/folder/specific_file.csv/-/csv_file_records` | `Step::GetAssetBinary` — the command receives one file's bytes, so its dependency is **that one file** |

That difference is not incidental; it is the mechanism. `-R-key` hands over an address and keeps the
data out of the dependency set, while `-R-bin` pulls exactly one file in. The chunk is narrower
because its *query* is narrower, and the planner already computes that.

---

## 1. What is a record?

**A record is a projection of something addressable into named fields, identified by where it came
from.**

| Part | What it is | Why it cannot be folded into the others |
|---|---|---|
| **Identity** | the asset, plus an asset-dependent **record id** | A hit that is not addressable is a dead end (`use-cases.md` A7). Identity must be structurally guaranteed, not a field name every consumer agrees on |
| **Fields** | named, typed values — `Value::Object` | What predicates test and what a SQL column *is*. Named and typed is the one requirement the SQL task imposes |
| **Text** | what full-text matching runs over | Distinct from fields, because tokenizing a status code is wrong and exact-matching a paragraph is useless |

### Identity is a pair, and the expensive half is not stored per record

An evaluable locator per record — `-R/some/folder/f.csv/-/ns-csv/row-42` — is the right *concept*
and the wrong *representation*. Constructing and holding a query per row costs an allocation and a
string per record, which for a large parquet file is a large multiple of the data it describes.

So identity is **(asset, record id)**, where the record id is small and asset-dependent: a row number
for CSV or parquet, a line number or byte offset for text, a pointer path for JSON, and **nothing at
all** when the asset itself is the record.

Two consequences that make this cheap rather than merely smaller:

1. **The asset is carried by the chunk, not by the record.** In the common case every record in a
   chunk comes from one asset, so the reference is stored once. A chunk that spans assets (a folder
   of many tiny files in one chunk) carries a small asset table and each record holds an index into
   it — ordinary dictionary encoding, and free in the common case where the table has one entry.
2. **The locator query is derived on demand, not stored.** The chunk declares *how*: a command to
   apply to the asset, with the record id as a parameter. Rendering
   `-R/some/folder/f.csv/-/ns-csv/row-42` is then a construction, done for the handful of records a
   consumer actually wants to cite or fetch.

The derivation must go through `ActionRequest`, never string templating. The escaping guide is
explicit that query text is not built by hand — a record id that is a string containing a `-`, a `/`
or a space would otherwise produce a corrupt query.

### What a record id is

A small enum, not a `Value`: an index (`u64`) covers row and line numbers, a short name covers JSON
pointers and named entries, and absent covers "the asset is the record". A full `Value` per row would
give back exactly the weight the pair was chosen to avoid.

**Stability is narrower than it first appears, and that is a relief.** Because refresh replaces a
whole chunk (§3), a record id does not need to be stable across content changes — only *within* a
version. Row 42 of version X is a well-defined thing; whether it is still row 42 after the file
changes is a question only a citation asks, and a citation is to a version.

### Is `Value::Object` enough?

**As the fields, yes. As the whole record, no.** `Value::Object(BTreeMap<String, Value>)` already
exists, already serializes, is schemaless by nature and maps directly onto a GlueSQL row, so there is
no case for inventing a map. What a flat map cannot express:

1. **Guaranteed identity** — nothing enforces the presence or type of an agreed `asset_key` entry,
   so every consumer re-derives a convention that will drift.
2. **The field/text distinction** — an engine configuring itself from a stream must know what to
   tokenize. A map cannot say, so the knowledge moves into hand-written engine configuration, and
   now the stream and the engine can disagree silently.
3. **Provenance versus payload** — fold the address into the map and `SELECT *` returns query
   strings next to data.

> **Recommended:** a thin struct — identity, fields, optional text — whose fields are a
> `Value::Object`. A new type, but no new *representation*.

---

## 2. What is a record stream?

**A stream is a query that yields records, plus the partition that says what it is made of.**

Nothing more is needed: a Liquers query is already an address *plus* a derivation, and the planner
already computes its dependency set. A stream needs no identity of its own — it *is* its query.

```
stream query  ──▶  partition:  [ chunk descriptor, … ]        split by DEPENDENCY
chunk         ──▶  batches:    [ batch, … ]                   split by SIZE
batch         ──▶  records:    [ record, … ]                  materialized
```

A stream with one chunk, and a chunk with one batch, are the simple cases — one shape, not three.

---

## 3. Chunks and batches: one mechanism, two criteria

> **A chunk is the unit of refresh. A batch is the unit of memory. A record is the unit of
> retrieval.**

Conflating the first two is the mistake the previous draft made. A parquet file is the right chunk —
it is what a dependency is *about* — and the wrong thing to hold in memory. Partitioning by
dependency and partitioning by size are different questions with different answers, and applying one
mechanism twice keeps the model small.

### The chunk

| Field | Purpose |
|---|---|
| **id** | stable across refreshes, so a sink can replace a chunk's records wholesale |
| **query** | how to rebuild exactly this chunk — the refresh query |
| **version** | whether it needs rebuilding, answerable **without** rebuilding it |
| **asset** (or asset table) | the identity half that records do not repeat |
| **locator rule** | the command that turns a record id into an evaluable query |

Refresh: for each chunk whose version differs from what the sink recorded, re-evaluate and **replace
all of that chunk's records**. Replacement rather than merge is deliberate — record-level diffing
would need record-level versions, which costs a full read, which is the cost the chunk exists to
avoid.

**Why not version individual records?** A record is derived and has no independent existence. A chunk
is precisely the smallest unit whose staleness is decidable **from metadata alone**, which is what
makes it the right granularity rather than an arbitrary batching convenience.

**Chunk versions come nearly free.** `MetadataRecord.dependencies` is already
`Vec<DependencyRecord { key, version }>` and `Version` is a content hash, so a chunk version is a
hash over its dependencies' current versions — one metadata read per source file, no data reads.

### The batch

A batch exists only to bound memory. It is not a dependency unit, it is not addressed by a sink for
refresh purposes, and its boundaries may move between evaluations without meaning anything. For
parquet a batch is naturally a row group; for CSV or NDJSON, *n* rows.

---

## 4. Streaming, honestly

A chunk "being a stream" means different things on the two sides of a query boundary, and the
difference is not cosmetic.

**In process**, a chunk can be a genuine async iterator yielding batches: a consumer pulls, the
producer reads incrementally, nothing large is ever resident. This is the shape the trait should
have.

**Across a query or HTTP boundary**, a query returns a *value*, and Liquers values are materialized
`Arc`-wrapped things that are cached, versioned and serialized. A stream is none of those. So the
streaming form must be **addressable batches** — a batch is an ordinary value with an ordinary query
address, cacheable and composable like anything else. Iteration becomes enumeration.

Three constraints at HEAD that this design must state rather than assume away:

1. **`openbin` is unimplemented in every store** (`CORE-STORE-OPENBIN-MISSING`, P3): four `// TODO:
   implement openbin` markers in `liquers-core/src/store.rs` and
   `liquers-store/src/opendal_store.rs`. So the *source* cannot yet be read incrementally. Batching
   therefore bounds the **consumer's** memory today, not the reader's: producing batch *i* of a large
   parquet file still reads the file. That is a real limit, and this design is a reason to raise that
   issue's priority rather than to work around it.
2. **Value-level serialization is whole-value.** `DefaultValueSerializer::as_bytes(&self,
   data_format) -> Vec<u8>` returns the entire encoding in memory. A writer-based path exists inside
   the polars module (`serialize_dataframe_to_writer`) but not at the value or asset level — and is
   itself called with a `Vec<u8>` — so serializing a large stream to CSV through the ordinary path
   materializes it. Filed as `VALUE-SERIALIZATION-HAS-NO-INCREMENTAL-WRITER`.
3. **There is no streaming value type, and adding one is not obviously right.** A value that cannot
   be cloned, cached, hashed or re-read is not a Liquers value; making it one would weaken the
   contract every other part of the system relies on. Addressable batches get the benefit without
   that cost.

---

## 5. Schema and field roles

**Optional, advisory, attached to the stream** — and its most valuable content is not types, it is
**field roles**:

| Role | To a search engine | To SQL / serialization |
|---|---|---|
| `id` | not indexed, returned | a key column |
| `text` | tokenized, matched by the text clause | a text column |
| `keyword` / facet | exact match, facetable, not tokenized | a column |
| `stored` | returned but not searched | a column |
| `numeric` | range clauses | a numeric column |
| `vector` | similarity clause | opaque |

These are the field options every engine already has — Lucene, Tantivy and Elasticsearch mappings all
say the same thing in their own words — and they are the minimum an external engine needs to
**configure its index from a query**. That closes the loop with the motivating usage: the
configuration query yields not only records but the mapping to configure the engine with, so the two
cannot drift into disagreement.

A schema may also carry a **uniformity promise**: that every chunk in this stream has the same
fields. Search does not need it; serializing a whole stream as one CSV does, since a CSV has one
header. Heterogeneous streams remain legal and serialize as NDJSON.

---

## 6. Records as a tabular interchange layer

This is the part that takes the record model beyond search. **A chunk is a table, and a stream is a
table in parts**, so one mechanism reinterprets anything as tabular data:

| Target | Fit | Note |
|---|---|---|
| **NDJSON** | exact | One record per line, no global structure — the natural streaming form, batch-aligned with no buffering |
| **CSV** | good | Needs the uniformity promise for a single header |
| **Parquet** | good | Needs a schema; a batch maps onto a row group, which is what row groups are for |
| **GlueSQL** | good | A stream is a table; its schemaless support covers heterogeneous rows |
| **DataFrame** | good | Behind the `polars` feature; a conversion, not the primary form |

So the record model has **four consumers, of which search is one**: search, external sinks, SQL, and
serialization. That is an argument for placing the types in `liquers-core`, and it raises a
structural question worth deciding explicitly rather than by drift — see §8.

---

## 7. Two levels — and a separate axis that is easy to confuse with them

The model is general; the first version does not have to be. But the split has to be along the right
seam, and an earlier draft cut it in the wrong place.

**The level is about cardinality, and nothing else.**

**Level 0 — one record per asset.** Record id absent, the asset is the identity. No chunking beyond
"a directory of assets", no batching, no locator rules, no schema. **This is what the essential
search use cases need** — agent memory and a user's search box are about finding *documents*, not
rows.

**Level 1 — many records per asset.** Ids, chunks, batches, locator rules, schema. Needed by SQL, by
external sinks over tabular data, by serialization, and by searching *inside* structured data — all
optional in `use-cases.md`.

**Level 0 must be the degenerate case of Level 1, not a second type**: a record with no id, in a
chunk with one batch, in a stream with one chunk per asset.

### The axis that is not the level: where a record's fields come from

The earlier draft defined Level 0 as "fields are the metadata fields", which quietly fused two
independent things. Extracting YAML front-matter from a Markdown file produces **one record per
asset** — unambiguously Level 0 — and yet its fields do not come from metadata. Cardinality and
field provenance are orthogonal:

| Field source | What it costs | Level |
|---|---|---|
| **Metadata** — whatever `MetadataRecord` and `AssetInfo` already carry | nothing; already read during enumeration | any |
| **A materialized projection** — a command's result, stored and content-hash versioned | one evaluation per document *version*, paid outside the search | any |
| **An inline projection** — applying a pure command to bytes the search is already reading | at most one parse per candidate whose bytes were read anyway | any |

### The invariant is about producing assets, not about doing work

An earlier draft forbade inline projection on the grounds that it is "an evaluation". That was
wrong, and the distinction it missed is structural rather than a matter of degree.

`CommandExecutor::execute(command_key, &State, arguments, context) -> Value`
(`liquers-core/src/commands.rs:540`) applies a command to a state and returns a value. **It creates
no asset, persists nothing, runs no recipe and cascades no dependency.** Evaluating a *query* —
`Context::evaluate`, `AssetManager::get_asset` — does all four. The invariant is about the second:

> **A search may apply a pure projection to a candidate it is already entitled to read. It never
> evaluates a query, because that produces an asset and may run an arbitrarily expensive recipe.**

The cost argument points the same way. A text clause already reads every candidate's bytes; parsing
YAML front-matter out of bytes that have just been read is strictly cheaper than the substring match
performed on them. Forbidding the parse while permitting the match is incoherent.

### How a field resolves

In order, stopping at the first that answers:

1. **Metadata carries it** — free; it was read during enumeration.
2. **A materialized projection exists and is fresh** — one extra metadata read.
3. **A declared pure projection over the candidate's bytes** — computed in memory, cached in process
   by content version, never persisted.
4. **Nothing** — the field is absent. The predicate does not match, and the result says the field was
   *unavailable* rather than silently reporting false.

So materialization is an **optimization, not an enabler**: it removes the content read, turning a
field-only search from O(corpus bytes) into O(corpus metadata). That is the real value of
`CORE-METADATA-NO-APPLICATION-ATTRIBUTES` — it makes filtering by front-matter *cheap*, not
*possible*. This repository already relies on exactly that trade by hand: `specs/index.csv` is a
materialized projection of every document's front-matter, regenerated and committed because computing
it per query would be wasteful.

### The gap this exposes

`CommandExecutor::execute` takes a `Context`, and a `Context` offers `evaluate` and
`get_async_store` (`liquers-core/src/context.rs:746`, `:314`). So although the *executor* creates no
asset, the command it runs could evaluate queries or touch the store through its context. Nothing
structurally distinguishes a pure projection from one with side effects.

Two ways to close it, and Phase 2 must pick: **declare** purity on the command (a contract checked by
review), or **restrict** the context passed on the search path so that evaluation is refused. The
second is much stronger and no such restricted context exists today — filed as
`COMMAND-CANNOT-BE-RUN-WITH-A-RESTRICTED-CONTEXT`.

---

## 8. What this asks of the rest of the design

1. Records are **structurally addressable**: identity is part of the type, not a convention — but the
   expensive half of an address is derived on demand, not stored per record.
2. Fields are **named and typed** — the one thing the SQL task requires
   (`NO-SQL-QUERY-CAPABILITY-OVER-STORED-AND-DERIVED-DATA`).
3. The reconciliation `(id, version)` pairs of `interoperability-layer.md` §3 are **chunk** ids and
   versions. Batches never appear in a diff.
4. **Projection identity** (`interoperability-layer.md` §7) belongs in the chunk version: change the
   projection rule and every chunk version must change, or a re-tokenized index silently looks fresh.
5. Field **roles** are what let one record serve a scan, an engine, a table and a serializer.
6. **A structural question for Phase 2 or for the user:** with four consumers, the record model is
   arguably its own capability with search as its first client, rather than a part of search. The
   pragmatic answer is that Level 0 ships inside this design and Level 1 graduates to its own design
   when SQL or serialization work starts — but that should be a decision, not an accident.

## 9. Open decisions for Phase 2

1. The record id representation — index, short name, absent — and whether anything needs more.
2. Whether a chunk carries one asset or an asset table, and whether the table is worth its complexity
   before a consumer needs it.
3. Where the locator rule lives — chunk, schema, or the stream command's metadata.
4. Whether the partition is itself a record stream (one mechanism, recursive base case) or a
   separate, lighter type.
5. Whether batches are addressable as queries, enumerated by count, or cursor-driven — and what a
   cursor would be stable against.
6. Whether chunk versions derive generically from the chunk query's dependency set, or are computed
   by the stream command. Generic derivation is far better if possible: it cannot be got wrong per
   command.
7. Whether `text` is a field carrying the `text` role rather than a separate part of the record.
8. Whether the uniformity promise is part of the schema or a separate assertion a serializer checks.
