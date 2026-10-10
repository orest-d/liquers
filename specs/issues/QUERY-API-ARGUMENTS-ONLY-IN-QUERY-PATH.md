---
id: QUERY-API-ARGUMENTS-ONLY-IN-QUERY-PATH
kind: feature
title: Query API arguments cannot be passed as JSON or URL parameters
status: draft
priority: P2
complexity: M
area: [axum]
design: 
created: 2026-10-10
github:
---
## Problem

The query API takes command arguments only from the Liquers query in the URL path, positionally.
`GET {base}{*query}` reads the path and nothing else, so `?query=foo&limit=10` is dropped silently.
`POST {base}{*query}` parses a JSON body and discards it:
`liquers-axum/src/query/handlers.rs` `post_query_handler` carries
`// TODO: Implement query parameter modification once API is designed`, and
`specs/reference/WEB_API_SPECIFICATION.md` documents the body as "logged but not used".

The `web-api-library` design planned "Parse JSON body for additional arguments → Merge arguments
into query" (`specs/design/web-api-library/phase2-architecture.md`, around line 985); that step was
never built. `command-declaration` decision C3 kept keyword arguments out of the *query language*,
which is a separate question from an HTTP request naming them.

## Impact

Any command with free-text or several optional arguments is awkward to call over HTTP. A search is
the worst case: `ns-search/search` takes an expression containing spaces, quotes, colons and
parentheses, each of which must be escaped (`~.`, `~nquot~`, `~ncolon~`, `~nlpar~`) inside the
path, and every positional argument before the one wanted must be repeated. A browser search box
or an agent tool can build such a path with `ActionRequest`, so there is a workaround; a person
with `curl` effectively has none. It also makes the URL the only place an argument can go, so a
long query text runs into URL length limits.

## Expected behaviour

The request may name arguments of the **last action** of the query, by argument name, in either
form:

- `GET /q/ns-search/catalog/search?query=expiration%20safety&limit=20`
- `POST /q/ns-search/catalog/search` with `{"query": "expiration safety", "limit": 20}`

The mechanism already exists in core: `Plan::override_value(name, Value)` and
`Plan::override_link(name, Query)` (`liquers-core/src/plan.rs`) override a named parameter of the
last action, and recipes use them for `Recipe.arguments` / `links`. What needs deciding:

- Whether an argument may be given both in the path and by name (error, or name wins).
- How a URL string value is converted to the argument's type (`ArgumentType` from the command
  metadata), and how a `multiple` argument is given (repeated key, or a JSON array).
- Whether reserved names (`format`, used by some routes today) collide with argument names, and
  how a query-string argument is distinguished from a route option.
- Whether a POST body may also carry links (`{"links": {...}}`), as a recipe can.
- How the overridden plan is keyed for caching: the asset's identity must include the overrides,
  or two requests with different arguments would share one asset.

## Discovery

Checked while designing `store-and-asset-search` revision 8 (2026-10-10), where calling
`ns-search/search` over HTTP was the motivating case. Verified against `liquers-axum/src/query/`
at HEAD: no route maps a JSON body or URL query string onto action arguments.
