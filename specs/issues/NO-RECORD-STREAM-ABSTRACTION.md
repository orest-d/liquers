---
id: NO-RECORD-STREAM-ABSTRACTION
kind: feature
title: No record stream abstraction
status: draft
priority: P2
complexity: XL
area: [core/value, core/commands, lib/value, web]
design: record-streams
created: 2026-09-19
github:
---
## Problem

Liquers has no tabular value. A DataFrame exists only as `ExtValue::PolarsDataFrame`, behind the
optional `polars` feature in `liquers-lib` — so:

- **`liquers-web` has no way to hold a table at all.** It is wasm32-only and polars cannot be
  bundled there, which leaves the browser build with bytes, text and JSON.
- **Nothing streams.** Every value is materialized, so a table larger than memory cannot be
  processed. `CORE-STORE-OPENBIN-MISSING` means the source cannot even be read incrementally, and
  `VALUE-SERIALIZATION-HAS-NO-INCREMENTAL-WRITER` means the result cannot be written incrementally.
- **Interoperation is one-off.** Each integration that wants rows — an external search engine, a
  vector store, a SQL mirror, a serializer — invents its own shape, because there is no common one.
- **Rows have no provenance.** A DataFrame carries no record of which query and which dependency
  versions produced it, so nothing can say whether its contents are still valid.

## Impact

P2 because there is a workaround for the native build — use polars — and nothing is incorrect today.
It is XL because it is a new subsystem in `liquers-core` with consumers in four crates.

What it costs is a whole class of capability rather than a feature. `design/store-and-asset-search/`
reached this conclusion the long way: six architecture revisions established that the record stream,
not the search, was the load-bearing abstraction, and that it serves four consumers of which search
is one — search, external engines, SQL
(`NO-SQL-QUERY-CAPABILITY-OVER-STORED-AND-DERIVED-DATA`) and serialization.

## Expected behaviour

A record stream that is an ordinary Liquers value: addressable, cacheable, produced by commands,
serialized by the ordinary machinery. Designed in `design/record-streams/`, with five requirements:

1. **Interoperable** with GlueSQL, polars, pandas, Arrow, Qdrant, Tantivy, Lucene and external
   relational databases, through an Arrow-compatible memory layout **without a heavy Arrow
   dependency**.
2. **A `Value` variant**, `Arc`-wrapped so the enum does not grow (`CORE-VALUE-ENUM-OVERSIZED`).
3. **A lightweight DataFrame** with no reliance on polars, so it works on wasm32.
4. **Lazy and chunked**, so a multi-gigabyte table is processed with one chunk resident at a time.
5. **Provenance and validity per chunk**, flyweighted to the record — a chunk carries a `Metadata`,
   so provenance is its query and dependency versions and validity is the staleness check the
   dependency manager already performs.

Questions for the design are enumerated in that folder's Phase 1; the two that bear on other work
are how far laziness can go before `openbin` exists, and whether the `Value` variant holds a
materialized set or a lazy handle — a lazy handle being neither cloneable nor cacheable, which a
value must be.

## Discovery

Split out of `store-and-asset-search` on 2026-09-19 by explicit decision: records are stabilized
first and search is rebuilt on top. Verified at HEAD: the only tabular type is
`ExtValue::PolarsDataFrame` behind an optional feature, `liquers-web` is excluded from
`default-members` and is wasm32-only, and no store implements `openbin`.
