---
id: NO-SQL-QUERY-CAPABILITY-OVER-STORED-AND-DERIVED-DATA
kind: feature
title: No SQL query capability over stored and derived data
status: draft
priority: P2
complexity: M
area: [core/store, lib/commands]
design: sql-over-tables
created: 2026-09-17
github:
---
## Problem

Liquers can address, derive and cache data, and — once `store-and-asset-search` lands — select it
by a predicate. It cannot *query* it in the relational sense: no joins, no grouping, no
aggregation, no projection across entries. A question like "which stored tables have a column named
`price`, and how many rows does each have" is expressible only as a program, not as a query.

The gap is felt in two different places, and conflating them is the main risk:

1. **Querying data held in Liquers.** Stored CSV, Parquet and JSON entries are tables in all but
   name. `liquers-lib`'s `pl` namespace can filter a single DataFrame, but nothing spans entries.
2. **Querying a corpus that has been mirrored into an external SQL database.** A different problem:
   the SQL engine is someone else's, and what Liquers owes it is a feed and a freshness guarantee.

## Problem Example

`-R/data/sales.csv/-/ns-pl/sql-~X~-R/queries/by_region.sql/-/to_text~E`, with the file holding
`SELECT region, sum(amount) AS total FROM self GROUP BY region`, should give one row per region.
Today there is no `sql` command (none in `specs/command_registry.yaml`, no SQL engine enabled in
any crate at 18732dd), and `ns-pl` has no grouping at all.

## Impact

P2: there is a workaround for (1) — load each entry and use the `pl` namespace — and (2) does not
block anything today. It is complexity L rather than M because a query language, however borrowed,
brings a type mapping, a schema story and an error surface with it.

The cost of leaving it open is that every consumer wanting a cross-entry answer writes a command,
and those commands accumulate into a private, undocumented query layer.

## Expected behaviour

Two separable pieces, and the second is nearly free once `store-and-asset-search` exists.

**A SQL engine over records.** GlueSQL is the obvious candidate: a Rust SQL library with parser and
execution layer whose custom backends implement `Store` (SELECT) and optionally `StoreMut`,
`AlterTable`, `Index` and `Transaction`, with support for schemaless and semi-structured data
(`MAP`, `LIST`) and joins between schema'd and schemaless tables. **Its `Store` is a
rows-and-tables trait and is unrelated to Liquers' `AsyncStore`** — the adapter exposes a Liquers
record set as a GlueSQL table.

**An external SQL mirror.** Covered by the interoperability layer designed in
`design/external-index-sync/interoperability-layer.md`: an external database is a sink with a
rows projection, fed by the same `(id, version)` diff and reconciliation as a search engine or a
vector store. Nothing SQL-specific is needed for the feed.

Questions for the design:

- How a record's fields map to columns, and what happens to entries whose fields differ — a
  schemaless table, a union schema, or one table per type identifier.
- Whether a SQL result is a new value type, a DataFrame (which would put an optional dependency on
  the path), or `Value::Array`/`Value::Object`.
- How a SQL query is spelled in a Liquers query given the escaping rules, since SQL text contains
  spaces, commas, quotes and asterisks.
- Whether write-back (`INSERT`/`UPDATE` reaching a store) is in scope at all; it interacts with
  `STORE-WRITE-HAS-NO-PRECONDITION`.
- Whether the engine is feature-gated, given that `liquers-web` is wasm32-only.

## Discovery

Split out of `store-and-asset-search` on 2026-09-17 by explicit decision: SQL is a separate task,
considered there only where it intersects. The intersection is recorded in that design's
`research-questions.md` §4 and `interoperability-layer.md` §6, and it imposes exactly one
requirement on the search design — records must carry named, typed fields, so that a SQL column and
a search predicate's field are the same thing.

## Triage 2026-10-21 (bulk design)

Still reproduces at HEAD (evidence above). Re-sized L → M: the design
[`sql-over-tables`](../design/sql-over-tables/) takes the first bounded slice — one SQL query
statement over one input table through polars' own SQL engine (`polars/sql`, already in
`Cargo.lock`). The rest is split or covered:

- joins across several entries → [`SQL-COMMAND-CANNOT-JOIN-SEVERAL-TABLES`](SQL-COMMAND-CANNOT-JOIN-SEVERAL-TABLES.md);
- a SQL engine for wasm builds (GlueSQL over records, the candidate named above) →
  [`NO-SQL-ENGINE-IN-WASM-BUILDS`](NO-SQL-ENGINE-IN-WASM-BUILDS.md);
- the external SQL mirror → `design/external-index-sync/` (unchanged);
- write-back → out of scope (a non-goal of the design).

Answers to the questions above, as proposed in the design: a SQL result is a `polars.DataFrame`;
SQL is spelled as a link to a stored `.sql` file (inline with entities also works); the engine sits
under the `polars` feature, so it is absent from wasm builds; record fields map through the
existing records → polars bridge (one table, one schema).
