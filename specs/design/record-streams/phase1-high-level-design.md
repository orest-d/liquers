# Phase 1: High-Level Design — Record streams

## Feature Name

Record streams

## Purpose

Give Liquers a **tabular value**: a stream of records that can be defined, stored, manipulated and
serialized like any other value, that interoperates with the systems tabular data actually goes to,
and that processes datasets far larger than memory. Liquers has no such abstraction today — a
DataFrame exists only behind the optional `polars` feature, which `liquers-web` cannot bundle at all,
so the wasm build has no way to hold a table.

Extracted from `store-and-asset-search`, where six architecture revisions established that the
record stream — not the search — was the load-bearing capability, and that it serves four consumers
of which search is one.

## The five requirements

1. **Interoperable** with GlueSQL, polars, pandas, Arrow, Qdrant, Tantivy, Lucene and external
   relational databases — through an Arrow-compatible memory layout, **without a heavy Arrow
   dependency**.
2. **A `Value` variant**, so a stream is addressable, cacheable, produced by commands and serialized
   by the ordinary machinery.
3. **A lightweight DataFrame** that does not rely on polars, and therefore works on wasm32.
4. **Lazy and chunked**, so a multi-gigabyte table is processed with one chunk resident at a time.
5. **Provenance and validity per chunk**, flyweighted to the record.

## The model

A **schema** owns field names, logical types and roles; **columns** hold values in Arrow's buffer
layout; a **chunk** is the unit of refresh and of provenance; a **batch** is the unit of memory; a
**record** is the unit of retrieval and has no independent storage. Requirements 1, 3 and 4 all
follow from the columnar layout, and requirement 5 from the chunk.

**Provenance costs almost nothing new.** A chunk carries a `Metadata`, which already has `query`,
`version`, `dependencies: Vec<DependencyRecord { key, version }>`, `status` and `updated`. So
provenance is "the query and dependency versions this chunk was produced from" and validity is the
staleness check the dependency manager already performs — and a record's provenance is its chunk's,
which is the flyweight. No per-record storage, no new vocabulary.

## Core Interactions

**Value types** — one new `Value` variant holding a record stream, `Arc`-wrapped so the enum does not
grow (`Value` is already 704 bytes, `CORE-VALUE-ENUM-OVERSIZED`). A `TypeInfo` entry is required.

**Command system** — commands produce, transform and consume streams: readers for CSV, NDJSON and
JSON, projections, and writers. Transformations are `select`, `filter`, `slice` and `concat` — enough
to be a usable DataFrame, not a query engine.

**Store and asset system** — a stream is read from and written to stores like any value. Large
sources need incremental reads, which is `CORE-STORE-OPENBIN-MISSING` (P3, stubbed in every store),
and large sinks need incremental writes, which is `VALUE-SERIALIZATION-HAS-NO-INCREMENTAL-WRITER`
(P2). **Both move from "deferred" to prerequisite for requirement 4**, and the first should be
re-prioritized.

**Query system** — no grammar change. A chunk's refresh query is an ordinary query; the partition is
a list of them.

**Web/UI** — `liquers-web` gains a tabular value for the first time, and the columnar buffers view
into JS as typed arrays with no copy. No new endpoint.

## Crate Placement

`liquers-core` — the layout primitives (aligned buffers, bitmaps), the schema, columns, chunks, the
stream traits, and the `Value` variant. Core stays minimal in the sense that matters: **no heavy
dependency and no `unsafe`** — its library code has none today and this adds none, with `bytemuck`
(already in the lockfile) making the aligned casts safe.

`liquers-lib` — format readers and writers, and conversion to polars behind the existing feature.
`liquers-py` — the Arrow C Data Interface export, the only place FFI `unsafe` belongs.
`liquers-web` — typed-array views.

## Documentation Intent

**Reference:** create `specs/reference/RECORD_STREAMS.md` — the schema, roles, column layout, the
chunk/batch/record scales, the provenance and validity contract, and what is guaranteed of the Arrow
layout. New rather than an extension: no existing reference owns a tabular value, and
`VALUE_TYPE_SYSTEM.md` is about value identity rather than about table structure.

**Guide:** create `specs/guides/RECORD_STREAM_GUIDE.md` — how to write a command that produces or
consumes a stream, how to build a schema with roles, and how to stay within one chunk of memory. The
audience is a contributor adding a data source, which is a repeatable task.

**Other documents to create:** none.

**Specific documents to update:** `specs/reference/VALUE_TYPE_SYSTEM.md` (the new type identifier),
`specs/reference/PROJECT_OVERVIEW.md` (a tabular value is a core concept), `specs/README.md`
(capability map), `specs/command_registry.yaml` (regenerated).

Audience: a contributor writing a data source or sink, and an integrator connecting an external
system. Both should work from the reference and the guide without opening this folder.

## Open Questions — answered 2026-09-19

All seven were answered. Three changed the architecture; one was badly posed and is restated.

### 1. How far does laziness go in the first version? — **Chunk-at-a-time; `openbin` out of scope**

The goal is a solid basis for lazy record streams, not the deepest possible laziness now. `openbin`
stays out of scope, but nothing here may **foreclose** a future command that uses it. Consequence:
`open_chunk` returns a *stream of batches* rather than a chunk, so a row-group-at-a-time reader slots
in later as a different implementation of the same trait, not as a redesign.

### 2. Materialized, lazy handle, or either? — **Lazy handle, with eyes open**

A record stream is a lazy handle. It may **optionally** support rewind, which makes it cloneable in
those cases; otherwise sharing it is a hazard the user must understand and generally avoid. Stream
commands are therefore **typically `volatile`**.

Checked: `volatile` is fully wired in `assets.rs` — a volatile keyed asset is never stored and never
registered, and volatility propagates to everything downstream of it. So the marker the prototype
needs already works. (`CommandMetadata.cache` does **not** — see the defect noted below.)

### 3. Provenance in `Metadata` per chunk, or something narrower? — **Two variants, and the manifest is the preferred form**

The answer is now driven by the prototype at
[`liquer/ext/dataframe_batches.py`](https://github.com/orest-d/liquer/blob/master/liquer/ext/dataframe_batches.py),
which was read for this design. It is more directly relevant than expected: **it already contains
this split.**

What it actually does — worth stating precisely, because it differs from the recollection of it in
one instructive way:

- `StoredDataframeIterator` holds `key` (a directory), `item_keys` (one store key per batch),
  `extension` and `number_format`. It serializes to JSON under its own `.idf` type. So the shareable
  form is a **manifest**: a small document naming where the batches are, not the batches themselves.
- It is **keys, not queries**. Generalizing key → query is the right move for Liquers — a query
  covers a computed chunk as well as a stored one — but the narrower choice bought the prototype
  something real: a key can be `contains`-checked, listed and removed, which `_store_batches` relies
  on to clean its directory before writing. A general query cannot be cleaned up. **This design takes
  queries and accepts that cleanup is then not automatic.**
- `rewind()` exists, and `__iter__` returns `self.copy().rewind()` — so iteration is non-destructive.
  **That is only possible because the chunk list is materialized.** A generator-backed stream
  (`repackage_batches`) cannot rewind. Rewindability is a property of the *manifest*, not of
  streaming.
- The volatility markers sort the commands into exactly two classes: `concat_batches` and
  `store_batches` return materialized values and are cacheable; `repackage_batches` and
  `store_batches_pass_through` are generators and carry `@command(volatile=True, cache=False)`.
- `_store_batches` rewrites the manifest after **every** batch, so a long run that dies leaves its
  completed batches usable. A generator cannot be checkpointed; a growing list of queries can. This
  is a real argument for the manifest form in the multi-gigabyte case, and it was not in this design
  before reading the prototype.

**One caveat on the prototype's authority.** The store-append path at `master` is broken:
`_store_batches` does `sdfi = pd.concat([sdfi, df])`, concatenating a `StoredDataframeIterator` with
a DataFrame, while the correct `sdfi.append(df)` method sits unused directly above it and the
original hand-rolled store code is commented out below. So the *shape* is validated by use; that
specific path is not. Nothing in this design depends on it.

**The resolution: three forms, two of them `Value` variants.**

| Form | Shareable | Cacheable | Serializable | Rewindable | `Value`? |
|---|---|---|---|---|---|
| **Materialized chunk** — a table in memory | yes | yes | yes | n/a | `Value::RecordChunk` |
| **Manifest stream** — a list of queries, one per chunk | yes | yes | yes, as the query list | yes | `Value::RecordStream` |
| **Opaque stream** — a generator | no | no | no | no | `Value::RecordStream`, volatile |

The two streaming forms are **one variant with two backings**, not two variants, because they differ
only in whether the chunk sequence is known in advance:

> **Superseded by Phase 2 revision 2.** The sketch below put both backings inside one stream type.
> Phase 2 splits them: a **`RecordSource`** (serializable, re-openable) opens a **stream** (one-shot,
> never a value). The reasoning that produced the sketch stands; the shape changed.

```rust
enum StreamBacking {
    /// One query per chunk. Rewindable, serializable, cacheable, checkpointable.
    Manifest(Vec<Query>),
    /// A generator. One-shot; the command producing it must be `volatile`.
    Opaque(/* … */),
}
```

`rewind()` succeeds on `Manifest` and fails on `Opaque`; serialization likewise. That is exactly
"optionally provide a possibility to rewind, which would make it cloneable in some cases", expressed
in the type rather than in documentation.

**Provenance**, the question as originally posed, follows: a manifest chunk's provenance is the
`Metadata` of the query that produces it, which the asset layer already computes — no per-chunk
`Metadata` needs storing at all. An opaque chunk carries one inline. So `Metadata` is reused, and the
704-byte worry applies only to the form that is not cached anyway.

### 4. Which column types? — **Add `Date` and date-time; `Decimal` later**

`chrono` is already a direct `liquers-core` dependency (`Cargo.toml:73`) and dates appear throughout
the existing metadata, so `Date` is added now. Storage stays primitive and Arrow-shaped —
`Date` is Arrow `Date32` (days since epoch, `i32`), date-time is the existing `Timestamp`
(microseconds, `i64`); chrono is the conversion layer, not the storage. `Decimal` is deferred to
whenever the SQL task needs it.

### 5. Does the schema promise uniformity across chunks? — **No promise; declared, not assumed**

Mostly uniform in practice, not guaranteed. The prototype agrees in the strongest way available: on
a column-count mismatch `concat_batches` and `repackage_batches` **warn and continue** rather than
fail.

The "all CSV files in a folder become one record stream, one chunk per file" case raised alongside
this answer is not a separate question — **it is the manifest form's motivating example**, a
`Manifest(vec![…])` with one query per file. So it is useful rather than illegal. Non-uniformity is
legal and has consequences rather than being an error: a stream whose chunks disagree cannot
`concat`, and cannot serialize as a single CSV (NDJSON is unaffected). Uniformity is therefore a
property a stream **declares and a consumer checks**, not an invariant.

### 6. How much DataFrame is enough? — **The baseline**

`select`/`filter`/`slice`/`concat` closes it. Group-by and join are a query engine and belong to the
SQL task.

### 7. ~~Should a chunk's records be addressable individually as a query?~~ — **Badly posed; split in two**

The question conflated two things, which is why it did not read clearly. Separated:

- **Identity** — can a record be named? **Yes, always, and cheaply.** A unique record id, plus a
  **chunk id**, which is confirmed as desirable and cheap. This promotes the chunk id from an
  internal field of `ChunkDescriptor` to part of the record identity: a record is identified by
  **(chunk id, record id)**, and the chunk id is stored once per batch rather than per row.
- **Addressability** — can a *query* be constructed that returns exactly that one record? **Not
  always, and optional.** This is what `LocatorRule` is for: available when a projection can offer
  one (`-R/f.csv/-/ns-csv/row-42`), absent otherwise, with re-evaluating the chunk as the guaranteed
  fallback.

Only the second was ever in doubt, and it stays optional.

## Defect noted while answering

`CommandMetadata.cache` (`command_metadata.rs:1013-1019`) is **declared, documented, serialized into
`command_registry.yaml`, and read by nothing** — the only reference outside its own definition is an
equality assertion in a test (`command_declaration.rs:987`), and `register_command!` has no statement
to set it. A command author who sets it gets silence. It is not a blocker here, because `volatile`
already does what the prototype's `cache=False` was for, but it is a knob that does nothing. Filed as
`COMMAND-CACHE-FLAG-IS-DECLARED-BUT-NEVER-READ`.

## References

- [`record-model.md`](./record-model.md) — the model, carried over from where it was written
- [`../store-and-asset-search/`](../store-and-asset-search/) — the design this was extracted from,
  and the first consumer
- `specs/issues/NO-RECORD-STREAM-ABSTRACTION.md` — the capability gap
- `specs/issues/CORE-STORE-OPENBIN-MISSING.md`, `specs/issues/VALUE-SERIALIZATION-HAS-NO-INCREMENTAL-WRITER.md`
  — prerequisites for the multi-gigabyte requirement
- `specs/issues/CORE-VALUE-ENUM-OVERSIZED.md` — why the variant is `Arc`-wrapped
- `specs/issues/NO-SQL-QUERY-CAPABILITY-OVER-STORED-AND-DERIVED-DATA.md` — the consumer that
  requires named, typed fields
