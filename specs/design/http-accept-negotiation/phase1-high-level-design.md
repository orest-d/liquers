# Phase 1: High-Level Design — Accept negotiation on query routes

## Design Readiness

- **Readiness:** needs-decision
- **Automatic fixing:** not-eligible — changes what HTTP routes answer (rule 4), spans two crates
  (rule 6), and its conversion half depends on an unimplemented design (rule 5).
- **Leading issue:** **Open design - when the header overrides the value's own format**
  (question 1).
- **Explanation:** the serialization half uses only what exists at HEAD (`TypeInfo` supported
  formats, `file_extension_to_media_type`, `ValueInterface::as_bytes`); the conversion half uses
  `ConversionRegistry::{targets_from, convert}` as `value-conversion-registry` Phase 2 fixes
  them. A working solution is fully described under the recommended answers.
- **Open questions:**
  1. **Open design - the own format wins whenever it is acceptable:** if any `Accept` range with
     `q > 0` matches the value's own media type (`*/*` and `type/*` included), the answer is
     today's bytes. Negotiation runs only when the own type is not acceptable. Recommended: every
     browser sends `…,*/*;q=0.8`, and existing clients send no header or `*/*`, so nothing that
     works today changes. Alternative: strict preference order (RFC 9110 lets the server choose)
     — a browser would then receive `text/html` renderings of tables and text it gets as CSV or
     plain text today.
  2. **Open design - a filename in the query is explicit:** a query ending in a filename
     (`…/to_record/people.csv`) is answered as it says, whatever the header, never `406`.
     Recommended: the query is the more specific request, and a link that works in a browser
     must work in a script. Alternative: `406` when the header excludes it.
  3. **Open design - `406` body:** status `406 Not Acceptable`, the usual `ApiResponse` error
     body whose `error.type` is `"NotAcceptable"` and a message listing the media types this value
     can be answered in. Recommended: built in `liquers-axum` without a new `ErrorType` (it is an
     HTTP outcome, not an evaluation error). Alternative: a core `ErrorType::NotAcceptable` —
     touches every exhaustive match over `ErrorType`, including the Python and JavaScript maps.
  4. **Proposed resolution - routes:** the Query API (`GET`/`POST {base}/{*query}`) and the
     Assets API `q/data` routes. `key/data` answers a key's stored representation and is
     unchanged; `entry` routes keep `Accept` for the envelope encoding; store routes serve stored
     bytes without a value.
  5. **Proposed resolution - automatic edges only:** a conversion is used only when its class is
     automatic (`Lossy` and `Fallible` refused), so `Accept` can never silently lose information.
  6. **Proposed resolution - no purposes:** the target set is "types whose `TypeInfo` writes a
     format with that media type", read from the type registry; no `table` identifier is needed
     (same answer as `ui-display-conversion` question 1).
  7. **Proposed resolution - `Vary: Accept`** on every answer of a negotiated route, so a cache
     never serves one client's format to another.

## Purpose

One query should serve a browser, a spreadsheet and a data tool, each in the format it asks for,
without encoding the format into the query (VISION: the web API is a primary access path;
priority 2, records and DataFrames). Today a client gets the value's default format unless it
spells a filename into the query.

## Problem Example

```
GET /liquer/q/-R/data/people.json/-/ns-rec/to_record      Accept: application/x-ndjson
```

The query validates (`GetAsset[data, people.json]`, `Action{ns-rec/to_record}`). At HEAD
(`cd6d5da`) `serve_query` (`liquers-axum/src/query/handlers.rs`) never reads the headers: the
answer is the `RecordView`'s default, `text/csv`. Should be: a `RecordView` can be written as
`ndjson`, whose media type is `application/x-ndjson`, so the answer is NDJSON with that
`Content-Type` and `Vary: Accept`. With a value that cannot be written as asked — an `Array` of
objects (`json` only) requested as `text/csv` — the server converts it by the automatic
`Array → RecordView` edge and writes CSV; requested as `image/png`, it answers `406` naming
`application/json` and `text/csv`.

## Scope and Acceptance Criteria

- **AC-1** The header selects another format of the same type
  WHEN a query's value is a `RecordView` and `Accept: application/x-ndjson`
  THEN the body is that view as NDJSON, `Content-Type: application/x-ndjson`
- **AC-2** Unchanged without a header, or when the own type is acceptable
  WHEN `Accept` is absent, `*/*`, a browser's default, or lists the own media type with `q > 0`
  THEN the answer is byte-for-byte today's, with today's `Content-Type`
- **AC-3** Conversion by an automatic edge
  WHEN the value's type cannot write the requested media type but an automatic edge reaches a type that can
  THEN the value is converted for this answer only and serialized; the asset is unchanged
- **AC-4** Lossy and fallible edges are not used
  WHEN the only edge to a type writing the requested media type is `Lossy` or `Fallible`
  THEN that edge is not taken
- **AC-5** Not acceptable
  WHEN no range with `q > 0` can be produced
  THEN the answer is `406` with `error.type` `"NotAcceptable"` and the producible media types
- **AC-6** Quality order
  WHEN `Accept: text/csv;q=0.5, application/x-ndjson` and both are producible
  THEN NDJSON is chosen; a range with `q=0` is never chosen
- **AC-7** An explicit filename wins
  WHEN the query ends in a filename (`…/to_record/people.csv`) and `Accept: application/x-ndjson`
  THEN the answer is CSV, `200`
- **AC-8** Other routes unchanged
  WHEN `key/data`, `q/entry`, `key/entry` or a store route is requested with any `Accept`
  THEN the answer is today's
- **AC-9** Caches are told
  WHEN a Query API or `q/data` route answers
  THEN the response carries `Vary: Accept`

Non-goals: negotiating `Accept-Charset` / `Accept-Encoding` / `Accept-Language`; a URL option
alternative to the header; negotiation on key, entry or store routes; lossy conversion on
request; streaming the negotiated body (`value-serialization-streaming`); transitive conversion.

## Core Interactions

Web/API (the two serving paths); Value types (`TypeInfo` formats, `as_bytes`); the conversion
registry (read only); media types (`liquers-core/src/media_type.rs`, read only).

## Crate Placement

`liquers-axum` only, in a new `api_core/negotiate.rs` and the two call sites; it reads
`liquers-core` APIs. The registry is in the environment (`value-conversion-registry`), so no
`liquers-lib` dependency is added.

## Documentation Intent

- Reference: `specs/reference/WEB_API_SPECIFICATION.md` §2.3 "Value transfers" (negotiation,
  `406`, `Vary`), §3.1 status codes (`406`), §7 Query API; History, `reviewed:`.
- Guide: `specs/guides/WEB_API_GUIDE.md` — asking for a format by header versus by filename.
- Other documents: none.

## Design Dependencies

- `value-conversion-registry` — **requires** for AC-3/AC-4 only (Phase 4 steps 5-6):
  `Environment::get_conversion_registry`, `targets_from`, `convert`, `ConversionPolicy::Automatic`.
  AC-1, AC-2 and AC-5-AC-9 ship without it.
- `value-serialization-streaming` and `record-source-row-streaming` — **overlaps** (T3 judged
  weak): both rewrite `serve_query` and `asset_bytes` to stream. Negotiation is a decision taken
  before serialization, so whichever lands second calls it ahead of the other's serializer;
  merging would make an `XL` (E2).
- `shared-metadata-and-bytes` — **overlaps**: removes the clones in the same two functions; rebase second.
- `ui-display-conversion` — **overlaps**: the UI consumer of automatic conversion; question 6 is
  answered the same way.
- `automatic-value-conversion` — **overlaps**: sibling consumer, no shared code.

## Consolidated Findings

- `serve_query` takes no `HeaderMap` at HEAD; the Query API ignores `Accept` entirely, and only
  `DataEntry` routes read it (`api_core/format.rs`). The issue's wording is corrected accordingly.
- The reverse map media type → format is derivable per value from `TypeInfo::supported_data_formats`
  and `file_extension_to_media_type` (the part before `:` of a format), so no new core table is
  needed; formats whose media type is `application/octet-stream` match only that exact range.
- Negotiation needs the value, not the cached bytes, only when it changes the answer; the
  common path (AC-2) still reads `get_binary`, so caching of serialized bytes is unaffected.
- Conversion and serialization are synchronous CPU work on the request task, as `as_bytes` is today.
- Order: steps 1-4 at any time; steps 5-6 after `value-conversion-registry`.

## References

`specs/issues/HTTP-ACCEPT-HEADER-NOT-NEGOTIATED.md`; `specs/design/value-type-system/type-conversion-draft.md`
§"Relationship to the encoding axis"; RFC 9110 §12.5.1 (`Accept`), §15.5.7 (`406`).
