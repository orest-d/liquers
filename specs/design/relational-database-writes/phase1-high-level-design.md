# Phase 1: High-Level Design — Writing records into a relational database

## Design Readiness

- **Readiness:** phase2-blocked
- **Automatic fixing:** not-eligible — `XL`; a new sink type, dependency and feature (auto-fix
  rules 1, 3, 4).
- **Leading issue:** **Blocking - scope and form of writes.** Whether Liquers writes into
  databases, and if so whether as a reconciled mirror or as write commands. The plausible answers
  give incompatible meanings to the same query:
  - **(A) mirror sink** (recommended): a table is a projection of a record source, kept equal to it
    by `reconcile` — re-running changes nothing unless the source changed; no query writes.
  - **(B) write commands**, e.g. `-R/data/new_orders.csv/-/ns-db/insert-orders`: evaluated twice,
    it inserts twice — or, if cached, the second request silently inserts nothing. Making it
    `volatile` and payload-requiring only chooses the first; either way the query is no longer a
    stateless, reproducible description (VISION non-goals: "No stateful query language";
    "Transactional/OLTP applications").
  - **(C) a database as a writable store mount** (`-R/pg/sales/orders` written through
    `AsyncStore::set`): a store write is bytes under a key, so a table would be replaced whole as
    a blob, or the store contract must grow a row path.
- **Explanation:** the mechanics of (A) are settled by the reconciliation contract and `sqlx`, but
  the choice among A-C is a product decision, and every form also needs the database identity and
  credentials that `relational-database-access` has not settled. Examples and a plan written now
  would invent both.
- **Open questions:**
  1. **Blocking - scope and form** (above). Recommended: (A) only; record (B) as not planned with
     the VISION reason; (C) rejected.
  2. **Blocking - database identity and credentials** (inherited from `relational-database-access`
     question 1): mount, connection value, or environment section. The mirror's configuration must
     use the same answer.
  3. **Open design - conflict handling:** recommended — Liquers owns the mirror table; one writer;
     each chunk replaced in its own transaction (`DELETE … WHERE _liquers_chunk = $1` then the
     inserts), so readers see a chunk either old or new; other writers to the table are
     unsupported, and a row whose `_liquers_chunk` the feed does not know is deleted on a known-list
     run. Multi-writer leasing is out of scope (`interoperability-layer.md` §7).
  4. **Open design - schema ownership:** create the table from `RecordSchema` when absent, or
     require an existing table and validate it. Recommended: validate an existing table by
     default, create only when configured to; never `ALTER`.
  5. **Proposed resolution - exact types:** as `relational-database-access` question 3 — a type
     that cannot be written exactly (`Vector`, a decimal read as `Text`) is refused naming the
     column, until `RECORD-FIELD-TYPE-HAS-NO-DECIMAL` lands.

## Purpose

Let a record source be mirrored into a relational table that other tools (BI, SQL clients,
applications) read, kept correct by Liquers' versions rather than by a full reload, without making
Liquers an OLTP system. Serves VISION priority area 3 (databases) within its non-goals.

## Problem Example

`data/orders.manifest.yaml` lists twelve monthly CSV chunks. A reporting team wants them in a
PostgreSQL table `reporting.orders`. Today (HEAD cd6d5da) no crate links a database client and
every record command is read-only (`RECORD_STREAMS.md` §"Sources, manifests and keyed chunks"), so
the team exports CSV (`…/ns-rec/materialize/orders.csv`) and reloads the whole table by hand; a
month removed from the manifest stays in the table.

Should be (recommended form A): a configured mirror reconciles the source into `reporting.orders`;
after March's CSV is corrected, the next run replaces only March's rows in one transaction, and a
removed month's rows are deleted. No Liquers query performs an `INSERT`.

## Scope and Acceptance Criteria

Written for form A; they stand only if question 1 is answered (A).

- **AC-1** A source mirrored into a table
  WHEN a manifest source of N chunks is reconciled into an empty configured table
  THEN the table holds every row with a `_liquers_chunk` column naming its chunk, and the sync
  table records each chunk's version and the mirror's projection id
- **AC-2** A changed chunk is replaced atomically
  WHEN one chunk's version changed
  THEN only that chunk's rows are deleted and re-inserted, in one transaction
- **AC-3** A removed chunk is deleted
  WHEN the source's known chunk list no longer contains a chunk the table holds
  THEN its rows are deleted
- **AC-4** No query writes
  WHEN any query is evaluated
  THEN no database write happens; only a reconciliation run writes, and `ns-db` read commands
  refuse non-`SELECT` SQL (`relational-database-access`)
- **AC-5** Schema mismatch refuses before writing
  WHEN the table's columns do not match the source's declared schema
  THEN the run fails before any write, naming the column
- **AC-6** No credentials leak
  WHEN errors, logs, reports or metadata of a run are inspected
  THEN no password or connection string appears
- **AC-7** Feature-gated, native only
  WHEN the feature is off, or the target is wasm32
  THEN the build succeeds without a database client

Non-goals: `INSERT`/`UPDATE`/`DELETE` commands; arbitrary SQL execution; transactions spanning
several chunks or Liquers and the database; several writers to one table; schema migration;
SurrealDB or other non-relational targets.

## Core Interactions

- **Records:** an `IndexSink` implementation (`external-index-reconciliation`).
- **Store / config:** the database identity (question 2).
- Query language, assets, commands: none, under form A.

## Crate Placement

The crate `relational-database-access` chooses for its database client (a new `liquers-sql`, or
`liquers-lib` behind a native-only feature), so the read and write sides share one `sqlx`
dependency and one connection configuration. Not `liquers-records` (wasm-clean, no client).

## Documentation Intent

- Reference: the relational reference `relational-database-access` plans
  (`specs/reference/RELATIONAL_SOURCES.md`) gains a "Mirroring a source" section.
- Guide: `specs/guides/RECORD_STREAM_GUIDE.md`, a subsection beside "Writing an index sink".
- Other documents: `specs/reference/VISION.md` is not changed; form A sits inside its non-goals.
- Documents to update: none until unblocked (`affects_docs` empty).

## Design Dependencies

- `external-index-reconciliation` — **requires**: `IndexSink`, `reconcile`, `ProjectionId`. Its
  contract choices (projection on the sink, time-based versionless entries) apply unchanged.
- `relational-database-access` — **requires**: database identity, credentials, the client and
  crate placement; blocked on the same identity question.
- `store-database-records` — **overlaps** (weak): brings `sqlx` (SQLite) into `liquers-store`;
  share the version.
- `RECORD-FIELD-TYPE-HAS-NO-DECIMAL` (issue) — **requires** for exact numeric writes (question 5).
