---
id: NO-WRITE-PATH-FROM-RECORDS-INTO-A-RELATIONAL-DATABASE
kind: feature
title: Records cannot be written into a relational database (no INSERT, UPDATE, DELETE or transactions)
status: draft
priority: P3
complexity: XL
area: [records, lib/commands, store/backends]
design:
created: 2026-10-21
github:
---

## Problem

`RecordSource` and every record command are read-only (`RECORD_STREAMS.md` §"Sources, manifests
and keyed chunks"). An access layer into a relational database implies writes —
`INSERT`/`UPDATE`/`DELETE`, transactions and conflict handling — and none of it is designed.
`NO-RELATIONAL-DATABASE-ACCESS-LAYER` names this as its largest part.

## Impact

None today: the read path does not exist yet either. Writes also sit uneasily with VISION's
non-goals (no OLTP, no stateful query language), so whether Liquers should offer them at all is
itself a question.

## Expected behaviour

A decision first: whether writes belong in Liquers (perhaps as volatile, payload-requiring
commands, or only as an external-sink feed through `design/external-index-sync/`), then a design.

## Discovery

Split out of `NO-RELATIONAL-DATABASE-ACCESS-LAYER` by the 2026-10-21 bulk design
(`design/relational-database-access/`), so that design can scope a read-only first slice.
