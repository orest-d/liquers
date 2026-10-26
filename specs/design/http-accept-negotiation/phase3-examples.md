# Phase 3: Examples & Use-cases — Accept negotiation on query routes

## Overview Table

| # | Kind | Name | Shows / checks | Scenarios |
|---|---|---|---|---|
| 1 | Example | Another format of the same type | `RecordView` as NDJSON | AC-1, AC-9 |
| 2 | Example | Conversion | `Array` as CSV through `RecordView` | AC-3 |
| 3 | Example | Not acceptable | `Array` as `image/png` | AC-5 |
| 4 | Test | `negotiate.rs::tests::parse_accept_orders_by_q_and_specificity` | parsing | AC-6 |
| 5 | Test | `…::tests::own_type_acceptable_is_as_is` | browser default, `*/*`, own type | AC-2 |
| 6 | Test | `liquers-axum/tests/accept_negotiation.rs::other_format_of_same_type` | `Text` as HTML | AC-1, AC-9 |
| 7 | Test | `…::no_header_is_byte_identical` | four header shapes | AC-2 |
| 8 | Test | `…::quality_order_and_zero_q` | `q` order, `q=0` | AC-6 |
| 9 | Test | `…::not_acceptable_lists_producible` | `406` body | AC-5 |
| 10 | Test | `…::explicit_filename_wins` | filename in query | AC-7 |
| 11 | Test | `…::other_routes_unchanged` | `key/data`, `q/entry`, store | AC-8 |
| 12 | Test | `…::automatic_edge_converts` | test edge `Array → Text` (`Structural`) | AC-3 |
| 13 | Test | `…::lossy_and_fallible_edges_refused` | test edges of those classes | AC-4 |

## Example 1: another format of the same type

With `liquers-lib`'s environment (feature `records`) serving `data/people.json` =
`[{"name":"Ann","age":31},{"name":"Bo","age":27}]`:

```
curl -H 'Accept: application/x-ndjson' http://localhost:3000/liquer/q/-R/data/people.json/-/ns-rec/to_record
→ 200, Content-Type: application/x-ndjson, Vary: Accept
{"name":"Ann","age":31}
{"name":"Bo","age":27}
```

Without the header, or with `Accept: text/csv` or a browser's default, the answer is today's CSV.
Validated: `-R/data/people.json/-/ns-rec/to_record` and `-R/data/people.json/-/ns-rec/to_record/people.csv`
(the latter with filename `people.csv`, AC-7).

## Example 2: conversion

A command `people` returning that array as an `Array` (own type `application/json`):

```
curl -H 'Accept: text/csv' http://localhost:3000/liquer/q/people
→ 200, Content-Type: text/csv
name,age
Ann,31
Bo,27
```

`Array` writes only `json`; `Array → RecordView` is `Structural` (automatic) and `RecordView`
writes `csv`. (`liquers-validate --command people -- 'people'`: `Ok`.)

## Example 3: not acceptable

```
curl -H 'Accept: image/png' http://localhost:3000/liquer/q/people
→ 406, Vary: Accept
{"status":"ERROR","message":"Not acceptable","query":"people",
 "error":{"type":"NotAcceptable","message":"cannot answer in image/png; available: application/json, application/x-ndjson, …, text/csv, …", …}}
```

## Edge and Error Cases

| Case | Expected | Scenario |
|---|---|---|
| `Accept: text/csv;q=0` only | `406` (zero excludes; own type not acceptable) | AC-5, AC-6 |
| `Accept: text/*` on a `Text` value | own `text/plain` matches → as is | AC-2 |
| `Accept: application/octet-stream` on a `RecordView` | no format maps to it → `406` | AC-5 |
| malformed element (`text`, `;q=x`) beside a valid one | malformed skipped, valid used | AC-6 |
| media type parameters (`text/csv; charset=utf-8`) | ignored for matching | AC-1 |
| value fails to serialize in the chosen format | that error (`422`), no fallback | AC-1 |
| structural edge fails on this value (array of numbers) | `ConversionError` `422` | AC-3 |
| evaluation error | today's error answer | AC-2 |
| query with argument overrides (unmanaged) | negotiated the same way; `x-liquers-managed: false` kept | AC-1 |

## Test Plan

Unit, `liquers-axum/src/api_core/negotiate.rs` `mod tests` (core `Value`, no server):
`parse_accept_orders_by_q_and_specificity`, `parse_accept_skips_malformed`,
`own_type_acceptable_is_as_is`, `direct_format_chosen_in_declared_order`,
`octet_stream_matches_only_exactly`.

Integration, new `liquers-axum/tests/accept_negotiation.rs`, using `tests/common` servers with
core `Value` (no `liquers-lib` dependency): a memory store with `data/a.txt` = `hello`, a command
`hello` returning `Text`, a command `people` returning an `Array`.

- `other_format_of_same_type` — `GET q/hello` with `Accept: text/html` (core `Text` writes
  `html`; own type `text/plain` not acceptable): `200`, `Content-Type: text/html`, `Vary: Accept`. AC-1, AC-9.
- `no_header_is_byte_identical` — no header, `*/*`, `text/html,application/xhtml+xml,*/*;q=0.8`,
  `text/plain`: bodies and `Content-Type` equal to a request without the header. AC-2.
- `quality_order_and_zero_q` — `text/plain;q=0, text/html` → HTML; `text/html;q=0.2, application/json`
  on `hello` → JSON. AC-6.
- `not_acceptable_lists_producible` — `Accept: image/png` on `people`: `406`, `error.type`
  `NotAcceptable`, message lists `application/json`. AC-5.
- `explicit_filename_wins` — `GET q/hello/hello.txt` with `Accept: text/html` → `text/plain`. AC-7.
- `other_routes_unchanged` — `key/data/data/a.txt`, `q/entry/hello` (envelope still chosen by
  `Accept: application/json`), store `data` route, each with `Accept: text/html`: as without it. AC-8.
- After step 5 (registry): `automatic_edge_converts` — the environment's registry extended with a
  test edge `Array → Text` (`Structural`, joins lines); `Accept: text/plain` on `people` → `200`
  `text/plain`. `lossy_and_fallible_edges_refused` — the same edge registered as `Lossy`, then as
  `Fallible`: `406`. AC-3, AC-4.

The `RecordView` examples (1, 2) are run by hand against `liquers-lib`'s environment in step 7;
they are not added as tests because `liquers-axum` does not depend on `liquers-lib`.

Run: `cargo test -p liquers-axum --lib negotiate`, `cargo test -p liquers-axum --test accept_negotiation`,
then `cargo test -p liquers-axum`.
