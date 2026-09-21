---
id: RECORD-SELECTION-IS-EAGER-NOT-A-VIEW
kind: feature
title: Selecting records from a stream is eager; there is no view that a source can push down
status: draft
priority: P2
complexity: L
area: [lib/value, lib/commands]
design:
created: 2026-09-21
github:
---
# Selecting records from a stream is eager; there is no view that a source can push down

## What is missing

`ns-rec/rec_id-<id>` selects one record from a record stream, and `specs/design/record-streams/`
specifies it eagerly: it consumes the stream and yields the matching record. Correct, and for a
billion-row source it produces every chunk to find one row.

The same is true of any selection over records. There is no way to express *"this stream, narrowed
by this predicate"* as a value a source can **inspect and satisfy cheaply** — a **view** — rather
than as work a consumer performs after the fact.

## Why it matters

Three cases where the difference is the whole cost:

- **A single record by id.** A keyed chunk whose id range is known can be chosen directly; a SQL
  source can answer with a `WHERE`; a parquet source can skip row groups by min/max. Eagerly, all of
  them read everything.
- **An external engine.** `store-and-asset-search`'s interoperability layer feeds an engine that
  could answer a selection itself, but only if the selection reaches it as a predicate.
- **A relational access layer.** `NO-RELATIONAL-DATABASE-ACCESS-LAYER` is the direction where this
  hurts most: the database is built to answer selections, and an eager model reads the table to do
  in Rust what SQL would have done in the engine.

## What already exists and should not be reinvented

The pieces are designed; what is missing is their composition at the record layer.

- **`SearchPredicate`** (`store-and-asset-search` Phase 2) is precisely "a selection carried rather
  than applied". `rec_id` is its simplest instance — an equality on the `Id` field.
- **The three-state pushdown report** — `Exact` / `Inexact` / `Unsupported` — adopted into
  `interoperability-layer.md` from DataFusion's `TableProvider`. `Inexact` is the state that makes
  pushdown safe: the source narrowed the candidates and the caller must still re-check.
- **`ChunkOrigin::locator`** is already the per-projection fast path for the single-record case.
- **`ChunkDescriptor`** is where per-chunk statistics (min/max of the `Id` column) would live, which
  is what lets a view skip a chunk without opening it — the same trick as parquet row-group pruning,
  and already noted as an open question in that design.

## What has to be decided

1. **Is a view a distinct value form, or a `RecordSource` carrying a predicate?** The second is
   smaller and keeps one value type; the first makes "this is not yet materialized" visible in the
   type.
2. **Is pushdown attempted for `rec_id` alone, or for any predicate?** The second makes this the
   record-layer half of the search design, and the two should then be designed together rather than
   meeting by accident.
3. **What does a view cost when nothing can be pushed down?** It must degrade to exactly the eager
   behaviour plus one indirection, or it is not worth having.
4. **Does a view compose?** Two selections over one source should narrow once, not twice.

## Not blocking

`rec_id` eager is correct. A view can replace its implementation without changing the query that
names a record — `<chunk query>/ns-rec/rec_id-42` stays the address either way — so this does not
hold up the record-streams implementation. It is filed because the eager cost is a real limit that
will be met as soon as a source is large enough to matter, and because the design should be done
once, across the record and search layers, rather than twice.
