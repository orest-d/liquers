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

## Open Questions

1. How far does laziness go in the first version? A chunk-at-a-time stream is achievable now; a
   *row-group-at-a-time* read of a large file needs `openbin`, which no store implements.
2. Is the stream `Value` variant a materialized set of chunks, a lazy handle, or a type that can be
   either? A lazy handle is not cloneable or cacheable, which is what a `Value` must be.
3. Does provenance live in a `Metadata` per chunk, or in a narrower record-specific structure?
   Reusing `Metadata` buys the dependency machinery and costs 704 bytes per chunk.
4. Which column types are in the first version, and does `FieldType` need `Date` and `Decimal` for
   GlueSQL and Arrow fidelity?
5. Does the schema carry a uniformity promise across chunks, which CSV serialization needs and search
   does not?
6. How much DataFrame is enough? `select`/`filter`/`slice`/`concat` is the floor; group-by and join
   are a query engine and belong to the SQL task.
7. Should a chunk's records be addressable individually as a query, and if so is that a locator rule
   on the chunk or a general capability?

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
