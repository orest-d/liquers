---
id: QUERY-DOCS-CLAIM-LINK-PARAMETERS-DO-NOT-PARSE
kind: issue
title: query.rs module docs say link parameters do not parse, but they do
status: closed
priority: P3
complexity: S
area: [core/query, docs]
design: 
created: 2026-10-13
github:
---
## Problem

The module documentation of `liquers-core/src/query.rs` (around line 98) says link parameters
(`~X~<query>~E`) "do not yet round-trip through `crate::parse::parse_query`" and that "the omission
is tracked as `QUERY-ACTION-PARAMETER-LINK-PARSER`". Both are stale: the parser has a production for
links (design `QUERY-LINK-PARSER`, complete), `liquers-validate` parses `text-~X~-R/a.txt~E/upper`,
and no issue with that id exists.

## Impact

Rustdoc tells a reader that a working, documented feature is missing, and points at a record that
does not exist. Cosmetic, but it is the module docs of the query type. No workaround needed.

## Expected behaviour

The paragraph states that link parameters parse and round-trip, citing the parser production, and the
dangling id is removed.

## Discovery

Noticed on 2026-10-13 while listing what the template grammar of `design/liquers-mustache/` would
extend; the stale id was about to be cited as an overlap.

## Resolution

Fixed 2026-10-13. The paragraph in the `liquers-core/src/query.rs` module docs now states that link
parameters parse and round-trip through `parse_query`, pointing at the `link-parameter` production
documented in `liquers-core/src/parse.rs`, and gives `text-~X~-R/a.txt~E/upper` as an example
(checked with `liquers-validate --no-registry`: status `Ok`, `encoded` unchanged, parameter is a
`Link`). The id it cited, `QUERY-ACTION-PARAMETER-LINK-PARSER`, did exist (now in
`specs/archive/2026-08-08-issues.md`, resolved 2026-08-06 by `design/query-link-parser/`); the
sentence citing it as open was removed rather than repointed. Round-trip coverage:
`liquers-core/tests/action_parameter_link.rs`.
