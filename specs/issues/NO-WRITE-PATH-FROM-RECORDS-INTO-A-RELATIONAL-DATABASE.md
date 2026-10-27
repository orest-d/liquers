---
id: NO-WRITE-PATH-FROM-RECORDS-INTO-A-RELATIONAL-DATABASE
kind: feature
title: Records cannot be written into a relational database (no INSERT, UPDATE, DELETE or transactions)
status: draft
priority: P3
complexity: XL
area: [records, lib/commands, store/backends]
design: relational-database-writes
created: 2026-10-21
github:
---

## Problem

`RecordSource` and every record command are read-only (`RECORD_STREAMS.md` §"Sources, manifests
and keyed chunks"). An access layer into a relational database implies writes —
`INSERT`/`UPDATE`/`DELETE`, transactions and conflict handling — and none of it is designed.
`NO-RELATIONAL-DATABASE-ACCESS-LAYER` names this as its largest part.

## Problem Example

`data/orders.manifest.yaml` lists twelve monthly CSV chunks that a reporting team wants in a
PostgreSQL table. Today (HEAD cd6d5da) no crate links a database client and every record command is
read-only, so the team exports `-R/data/orders.manifest.yaml/-/ns-rec/materialize/orders.csv` and
reloads the whole table by hand; a month removed from the manifest stays in the table. Whether the
answer is a reconciled mirror (only changed months rewritten, removed months deleted, no query
writes) or write commands (`ns-db/insert-…`, which make a query non-reproducible) is the open
decision (`design/relational-database-writes/`).

## Impact

None today: the read path does not exist yet either. Writes also sit uneasily with VISION's
non-goals (no OLTP, no stateful query language), so whether Liquers should offer them at all is
itself a question.

## Expected behaviour

A decision first: whether writes belong in Liquers (perhaps as volatile, payload-requiring
commands, or only as an external-sink feed — now `design/external-index-reconciliation/`, whose
`IndexSink` a SQL mirror would implement), then a design.

## Discovery

Split out of `NO-RELATIONAL-DATABASE-ACCESS-LAYER` by the 2026-10-21 bulk design
(`design/relational-database-access/`), so that design can scope a read-only first slice.
