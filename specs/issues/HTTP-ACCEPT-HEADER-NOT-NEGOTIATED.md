---
id: HTTP-ACCEPT-HEADER-NOT-NEGOTIATED
kind: feature
title: The HTTP API reads Accept only for its envelope encoding, never to convert and serialize the value
status: draft
priority: P3
complexity: L
area: [axum, core/value]
design:
created: 2026-10-26
github:
---

## Problem

`liquers-axum` reads the `Accept` header only to choose the encoding of a `DataEntry` envelope
(`liquers-axum/src/api_core/format.rs`, `select_format` / `negotiate_format`: CBOR, bincode or
JSON). The value inside is never negotiated: a response carries the asset's own data format, or the
one a query's filename extension selects, so a client cannot ask for `text/csv` and get a frame or
a record view as CSV unless the query itself says so.

## Problem Example

```
GET /liquer/q/-R/data/people.json/-/ns-rec/to_record      Accept: text/csv
```

Today: the body is the record view in its default format, whatever the header says. Should be: the
server asks whether the value can become a type that can be written as CSV (conversion, then
serialization, `type-conversion-draft.md` §"Relationship to the encoding axis") and answers CSV, or
`406 Not Acceptable` naming what it can produce.

## Impact

Clients must encode the wanted format into the query, so one query cannot serve a browser and a
data tool alike.

## Expected behaviour

Content negotiation on query and asset routes: media types from `Accept` mapped to data formats
(`media_type.rs`), conversion only by automatic edges, serialization through
`DefaultValueSerializer`; no header keeps today's behaviour. Needs a design (`L`, cross-crate).
Requires `value-conversion-registry`.

## Discovery

Split out of `VALUE-NOT-CONVERTED-AUTOMATICALLY` by its bulk design (2026-10-26,
`automatic-value-conversion`), which covers only the command state and link arguments. Checked
against `api_core/format.rs`; not verified route by route.
