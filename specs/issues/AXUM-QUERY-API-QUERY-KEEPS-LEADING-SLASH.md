---
id: AXUM-QUERY-API-QUERY-KEEPS-LEADING-SLASH
kind: issue
title: The Query API mounted without a trailing slash evaluates the query with a leading slash
status: closed
priority: P3
complexity: S
area: [axum]
design: axum-query-api-base-slash
created: 2026-10-12
github:
---

## Problem

`QueryApiBuilder::new(base)` registers the route `{base}{*query}` (`liquers-axum/src/query/builder.rs`).
With the conventional base `/liquer/q` — used by `examples/assets_recipes_basic.rs` and by
`WEB_API_SPECIFICATION.md` §7–§8 — the wildcard captures the separating slash, so
`GET /liquer/q/text-hi` evaluates the query `/text-hi`. That parses, but encodes as `/text-hi`,
not `text-hi` (checked with `liquers-validate`), and the error envelopes show it
(`"query":"/text"`). The Assets API's `q/` routes capture after a literal `/`
(`{b}/q/data/{*query}`), so the same request there evaluates `text-hi`.

## Problem Example

```bash
# server built with QueryApiBuilder::new("/liquer/q"), as in examples/basic_server.rs
curl http://localhost:3000/liquer/q/no_such_command
```

Today the handler parses `/no_such_command` (the wildcard captured the separating slash), so the
asset is cached under, and errors report, `/no_such_command`. It should evaluate `no_such_command`,
as `QueryApiBuilder::new("/liquer/q/")` already does.

## Impact

The two APIs address the same query by two different identities, so a value computed through the
Query API is cached under `/text-hi` and not found by `{assets}/q/info/text-hi` (and vice versa) —
each computes it once more. Error messages also show a query the client never wrote. Mounting the
Query API at `/liquer/q/` avoids it, but nothing says so; `QUERY-ABSOLUTE-FIELD-NAME-AMBIGUOUS`
shows a leading slash has a meaning in the query language, so it should not appear by accident.

## Expected behaviour

The Query API evaluates the path after its base without the separating slash, whatever the base
is written as — or the builder documents and enforces a base ending in `/`.

## Discovery

Running the guide's examples against `examples/assets_recipes_basic.rs` while documenting
`design/axum-query-named-arguments/` (Phase 5, 2026-10-12).

## Resolution

2026-10-17, `design/axum-query-api-base-slash/`. `QueryApiBuilder::build` now registers
`{base}/{*query}` through the private helper `join_base` (`liquers-axum/src/api_core/mod.rs`),
which drops a trailing `/` from the base and inserts exactly one. `"/liquer/q"` and `"/liquer/q/"`
serve the same route, the query loses the separating `/`, and `/liquer/qX` no longer matches.
`WEB_API_SPECIFICATION.md` §7 and Appendix A updated. Tests in
`liquers-axum/tests/query_api_routes.rs`: `qar09_base_without_slash_evaluates_query_without_leading_slash`,
`qar10_base_with_and_without_trailing_slash_are_the_same_route`, `qar11_base_is_a_whole_segment`,
`qar12_root_base`.
