---
id: HTTP-ACCEPT-HEADER-NOT-NEGOTIATED
kind: feature
title: The HTTP API reads Accept only for its envelope encoding, never to convert and serialize the value
status: draft
priority: P3
complexity: L
area: [axum, core/value]
design: http-accept-negotiation
created: 2026-10-26
github:
---

## Problem

`liquers-axum` reads the `Accept` header only on `DataEntry` routes, to choose the envelope's
encoding (`liquers-axum/src/api_core/format.rs`, `select_format` / `negotiate_format`: CBOR,
bincode or JSON). The routes that serve a value's bytes never read it: the Query API's
`serve_query` (`liquers-axum/src/query/handlers.rs`) takes no headers at all, and the Assets API's
`q/data` goes through `asset_bytes` (`assets/common.rs`), which serves `AssetRef::get_binary`. A
response carries the asset's own data format, or the one a filename in the query selects, so a
client cannot ask for NDJSON or CSV unless the query itself says so.

## Problem Example

```
GET /liquer/q/-R/data/people.json/-/ns-rec/to_record      Accept: application/x-ndjson
```

Today (`cd6d5da`): the body is the record view's default, `text/csv`, whatever the header says.
Should be: a `RecordView` can be written as `ndjson`, so the answer is NDJSON. For a value that
cannot be written as asked (an `Array` of objects requested as `text/csv`), the server converts it
by an automatic edge (`Array → RecordView`) and then serializes (`type-conversion-draft.md`
§"Relationship to the encoding axis"), or answers `406 Not Acceptable` naming what it can produce.

## Impact

Clients must encode the wanted format into the query, so one query cannot serve a browser and a
data tool alike.

## Expected behaviour

Content negotiation on the Query API and `q/data` routes: media types from `Accept` mapped to the
value type's data formats (`media_type.rs`), conversion only by automatic edges, serialization
through `DefaultValueSerializer`; no header keeps today's behaviour. The conversion half requires
`value-conversion-registry`; the serialization half does not. Design: `http-accept-negotiation`.

## Discovery

Split out of `VALUE-NOT-CONVERTED-AUTOMATICALLY` by its bulk design (2026-10-26,
`automatic-value-conversion`), which covers only the command state and link arguments. Checked
against `api_core/format.rs`; not verified route by route.
