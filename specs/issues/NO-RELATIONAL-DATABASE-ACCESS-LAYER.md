---
id: NO-RELATIONAL-DATABASE-ACCESS-LAYER
kind: feature
title: No access layer from Liquers into relational databases
status: draft
priority: P2
complexity: XL
area: [lib/value, lib/commands, store/backends]
design:
created: 2026-09-20
github:
---
# No access layer from Liquers into relational databases

## What is missing

Liquers can read files through `AsyncStore` and derive values through commands, but it cannot treat a
**relational database** as a source of data. There is no way to express "the rows of this Postgres
table, as a Liquers value" — so a query cannot join stored data against a database, and a database
cannot be a recipe input.

`record-streams` designs a tabular abstraction that is the natural interface for this, and
`specs/design/record-streams/engine-survey.md` §3 assesses the fit against sqlx, Postgres, MySQL,
SQLite and SurrealDB. The **read** path fits well. Three things do not, and they are why this is a
separate task rather than an extension of that design.

## Why it is not simply an extension of `record-streams`

1. **~~`SourceBacking` has no variant for it.~~ Resolved by `record-streams` Phase 2 (2026-09-24).**
   `RecordSource` is now a trait, so a SQL source — a connection plus a query, streamed lazily, whose
   chunk boundaries are **not known in advance** — is simply another implementation, and the natural
   place for pushdown into `WHERE` should it be wanted. What remains is designing that
   implementation, which is items 2 and 3.

2. **`Decimal` becomes mandatory.** `record-streams` defers it. Reading a `NUMERIC`/`DECIMAL` column
   as `Float` is a correctness bug, not an approximation, and it is the correct type for money.
   `uuid`, `jsonb`, arrays and intervals are likewise absent, as is the `TIME`/`TIMESTAMPTZ`
   distinction.

3. **Writing is entirely undesigned.** An access layer implies `INSERT`/`UPDATE`/`DELETE`,
   transactions and conflict handling. `record-streams` is read-only throughout and `RecordSource`
   has no write counterpart. This is the largest part of the work and has no design at all.

## What already fits, and is worth not re-deriving

- **sqlx is the right client.** `query(…).fetch(&pool)` returns a `BoxStream` of rows — row-at-a-time,
  already the shape a record batch builder consumes, with the driver owning the cursor. Postgres,
  MySQL and SQLite reach it through one API.
- **Keyset chunking works because of an invariant that already exists.** `LIMIT`/`OFFSET` pagination
  degrades quadratically; keyset pagination (`WHERE id > $last ORDER BY id LIMIT n`) does not, and it
  needs exactly one ordered unique key — which `RecordSchema::new` already enforces for
  reconciliation. The `Id` field is the key that makes SQL chunking efficient.
- **Field roles read naturally in reverse.** `record-streams` frames a role as the access paths the
  data *affords*; for a database those are the indexes that *exist*, discovered rather than declared,
  and the same vocabulary serves.

## Where SurrealDB sits

It fits least. Record ids are `table:id` things rather than scalars, documents nest, graph edges are
first-class, and schemafull is optional. A `SELECT` returns rows that project into a batch, so the
flat subset works — but nested documents and edges have no representation in a column model whose
Arrow subset deliberately excludes `Struct` and `List`. Usable for flat data; not a faithful
interface to what SurrealDB is.

## Scope when this starts

Read-only, one backend, no schema discovery beyond what a prepared statement reports, would already
be useful and is a reasonable first cut. Writes, transactions and multi-backend support are each
large enough to be their own step.

Related: `NO-SQL-QUERY-CAPABILITY-OVER-STORED-AND-DERIVED-DATA` is the mirror image — SQL *over*
Liquers data rather than Liquers over SQL data. The two share the record abstraction and little else.
