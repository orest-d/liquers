# Phase 1: High-Level Design — Reading relational databases as record sources

## Design Readiness

- **Readiness:** phase2-blocked
- **Automatic fixing:** not-eligible — XL; new dependency, feature, value or store type and
  commands (auto-fix rules 1, 3, 4)
- **Leading issue:** **Blocking - database identity and freshness:** how a query names a database
  table (and its credentials), and when a result read from it is stale.
- **Explanation:** the read mechanics fit (sqlx streams, keyset chunks, `RecordSource` with
  `ChunkList::Unbounded`), but two contract questions decide the user-visible shape and cache
  semantics, and their plausible answers are incompatible (examples below). Phases 3-4 would have
  to invent that contract.
- **Open questions:**
  1. **Blocking - identity:** where a connection is declared and how a query names it. Example:
     `-R/pg/sales/orders` (a database as a store mount: configured like stores, credentials via
     `${VAR}`, but a store returns bytes, not a lazy `RecordSource`), versus
     `-R/config/pg.yaml/-/ns-db/connect/table-orders` (a connection value built from a stored
     config: lazy and typed, but credentials live in the store and the connection is a
     non-serializable value), versus a new `databases:` section of `EnvironmentConfig` read by a
     command (`ns-db/table-sales-orders`: a core config change for a lib feature).
  2. **Blocking - freshness:** `ns-db/table-sales-orders` evaluated at 09:00 is cached; at 10:00 a
     row changes. Answer (a) volatile: every request re-reads, recipes depending on it never cache
     — "make for data" breaks. (b) a TTL `expires:` — stale within the window, silently. (c) a
     database change marker (PostgreSQL `xmin`/`pg_stat_*`, SQLite `data_version`) as the asset
     version — exact, but backend-specific and itself a query. Each gives different results for
     the same query, and VISION's "trust the result" depends on the choice.
  3. **Open design - exact types:** `NUMERIC` must not become `Float`. Recommended: read it as
     `Text` with a warning until `RECORD-FIELD-TYPE-HAS-NO-DECIMAL` adds `Decimal`; that issue is a
     prerequisite of a faithful first release.
  4. **Proposed resolution - first backend and client:** SQLite and PostgreSQL through `sqlx`, the
     client `store-database-records` already plans for `liquers-store` (feature `sqlite-store`);
     SQLite first, since it needs no server in tests.

## Purpose

Let a query treat a relational table, or the result of a read-only SQL statement, as a Liquers
record source, so a database can be a recipe input and its rows can be filtered, joined with
stored data and served like any other asset. Serves VISION priority area 3 ("more storage
backends — databases and cloud services") and area 2 (records).

## Problem Example

A team keeps orders in PostgreSQL and wants
`<orders from database sales>/ns-rec/eq-status-open/to_csv/open_orders.csv` as a link. Today there
is no way to write the first segment: no database client is linked by any crate at HEAD (no
`sqlx`/`rusqlite` use in any `src/`; `sqlx` appears in `Cargo.lock` only through OpenDAL's
optional database services, which store blobs, not tables). Should be: the segment yields a
`RecordSource` whose chunks are keyset pages of `orders`, and the rest of the query works
unchanged.

## Scope and Acceptance Criteria

- **AC-1** A table is a record source
  WHEN a query names table `orders` of a configured database (spelling: blocking question 1)
  THEN it yields a `RecordSource` with the table's columns as fields and `ChunkList::Unbounded`
  keyset chunks over the table's single-column primary key, marked `KeyRole::Id`
- **AC-2** A read-only statement is a record source
  WHEN a query supplies a `SELECT` (as a link to a stored `.sql` file)
  THEN it yields the same, chunked when the statement is ordered by a unique key, otherwise one chunk
- **AC-3** Types are not silently changed
  WHEN a column is `NUMERIC`, `UUID`, `JSONB`, an array or an interval
  THEN it is read losslessly or refused/converted with a warning naming the column — never read as
  `Float`
- **AC-4** Freshness is declared
  WHEN a cached result is requested after the table changed
  THEN it behaves as the freshness answer (blocking question 2) says, and the asset's metadata
  says which rule applied
- **AC-5** No credentials in queries or metadata
  WHEN any query, error, log or stored metadata of AC-1/AC-2 is inspected
  THEN no password or connection string appears
- **AC-6** Feature-gated and absent from wasm
  WHEN the feature is off, or the target is `wasm32`
  THEN the build succeeds without a database client and naming a database fails with an error
  naming the feature

Non-goals: writes, transactions and conflicts (split off:
[`NO-WRITE-PATH-FROM-RECORDS-INTO-A-RELATIONAL-DATABASE`](../../issues/NO-WRITE-PATH-FROM-RECORDS-INTO-A-RELATIONAL-DATABASE.md));
SQL over Liquers data (`sql-over-tables`); keeping Liquers' own metadata in a database
(`store-database-records`); SurrealDB's nested documents and graph edges; schema migrations.

## Core Interactions

- **Records:** a new `RecordSource` implementation; possibly a `FieldType` extension (via its own issue).
- **Commands / Store:** one or the other names the database (question 1).
- **Assets:** freshness rule (question 2). Query language: unchanged.

## Crate Placement

Not `liquers-records` (core-only, wasm-clean) and not `liquers-core`. Either `liquers-lib` behind
a new native-only feature, or a new crate (`liquers-sql`) depending on `liquers-records` and
`liquers-core`, so that `liquers-axum` deployments can opt in without `liquers-lib`'s weight — a
maintainer decision, recorded in Phase 2.

## Documentation Intent

- Reference: new `specs/reference/RELATIONAL_SOURCES.md` once the contract exists.
- Guide: a section of `specs/guides/RECORD_STREAM_GUIDE.md` (connecting a database).
- Other documents: none until unblocked.
- Documents to update: `RECORD_STREAMS.md` (a new source kind), `ENVIRONMENT_CONFIG.md` or
  `STORE_CONFIG_FSD.md` depending on question 1.

## Design Dependencies

- `record-streams` (complete) — **requires**: `RecordSource`, `ChunkList::Unbounded`, `KeyRole::Id`.
  It listed this source issue; its Phase 2 resolved the source's gap 1 and left gaps 2-3, which are
  this design's.
- `store-database-records` — **overlaps**: brings `sqlx` (SQLite) into `liquers-store`; the first
  backend here should share that dependency and version. No ordering.
- `sql-over-tables` — **overlaps** (weak): the mirror image (SQL over Liquers data). Shares only the
  record abstraction.
- `external-index-sync` — **overlaps**: an external SQL *mirror* fed from Liquers is that design's
  sink, not this source.
- `RECORD-FIELD-TYPE-HAS-NO-DECIMAL` (issue, no design) — **requires** for AC-3 in full.

## References

- Source: [`NO-RELATIONAL-DATABASE-ACCESS-LAYER`](../../issues/NO-RELATIONAL-DATABASE-ACCESS-LAYER.md)
- `specs/design/record-streams/engine-survey.md` §3; `specs/reference/RECORD_STREAMS.md`;
  `specs/reference/ENVIRONMENT_CONFIG.md`; `specs/reference/VISION.md` (non-goals: OLTP, stateful language)
