---
id: AXUM-QUERY-NAMED-ARGUMENTS
kind: design
title: Named arguments for the Query API, from the URL query string and a JSON body
form: compact
workflow: liquers-project
status: in_review
phase: high-level
readiness: needs-decision
autofix: not-eligible
area: [axum]
issues: [QUERY-API-ARGUMENTS-ONLY-IN-QUERY-PATH]
created: 2026-10-11
---
# Named arguments for the Query API, from the URL query string and a JSON body

## Phase 1: High-Level Design

### Purpose

Let an HTTP client of the Query API (`GET/POST {base}{*query}`, `liquers-axum/src/query/`) name
arguments of the query's **last action** instead of spelling every one positionally, escaped, in
the URL path: as URL query parameters on either method, and as a JSON object body on `POST`.
Core already has the mechanism — `Recipe.arguments` applied through `Plan::override_value`
(`liquers-core/src/recipes.rs` `Recipe::to_plan`); this design connects the HTTP request to it.

### Problem Example

`ns-search/search` takes `query: string = ""`, `rank: bool = true`, `case_sensitive: bool = false`,
`limit: int = 50`, `fields: multiple string`. A client wanting 20 results for the free text
`expiration safety` must write today:

```
GET /q/-R/docs/catalog.csv/-/ns-search/search-expiration~.safety-true-false-20
```

— escaping the space (`~.`; quotes, colons and parentheses need `~nquot~`, `~ncolon~`, `~nlpar~`)
and repeating `rank` and `case_sensitive` at their defaults to reach `limit`. Both of these are
accepted today and **silently ignore** the arguments:

```
GET  /q/-R/docs/catalog.csv/-/ns-search/search?query=expiration%20safety&limit=20
POST /q/-R/docs/catalog.csv/-/ns-search/search      {"query": "expiration safety", "limit": 20}
```

The search runs with `query = ""` and `limit = 50`. After this design both run the search with
`query = "expiration safety"`, `limit = 20`, and the other arguments at their defaults. (Both path
queries validate with `liquers-validate`.)

### Scope and Acceptance Criteria

In scope: the Query API's two routes. Out of scope: the Assets API (`{assets}/q/...`), the
WebSocket, and links by name (Q3). No change to the query language: named arguments exist only in
the HTTP request (`command-declaration` decision C3 is untouched).

- **AC-1** URL query parameters name arguments
  - WHEN `GET {base}<query>?a=x&b=y` is requested and `a`, `b` are arguments of the last action
  - THEN the value is computed as if the last action had `a = x`, `b = y`, each string converted
    by the argument's `ArgumentType` exactly as a path parameter would be, the rest positional or
    default
- **AC-2** A POST JSON object body names arguments
  - WHEN `POST {base}<query>` carries a JSON object body `{"a": <json>, ...}`
  - THEN each member overrides argument `a` of the last action; a JSON number, bool, string,
    array or null is passed as that JSON value, and a JSON string given for a non-string argument
    is converted as in AC-1
- **AC-3** Variadic arguments
  - WHEN a `multiple` argument is named by a repeated URL key (`?f=a&f=b`) or a JSON array
    (`{"f": ["a", "b"]}`)
  - THEN the argument receives one element per occurrence or array entry, in order
- **AC-4** Unknown or non-overridable name
  - WHEN a name is not an argument of the last action, names an `injected` argument, or the query
    has no action at all
  - THEN the response is `400` with error type `ParameterError` naming the argument, and nothing
    is evaluated
- **AC-5** Conflicting sources
  - WHEN an argument is given both positionally in the path and by name, or both in the URL query
    string and in the POST body
  - THEN the response is `400 ParameterError` naming the argument (Q1)
- **AC-6** Conversion failure
  - WHEN a named value cannot be converted to the argument's type (`?limit=abc`)
  - THEN the response is `422` with error type `ConversionError`, and nothing is evaluated
- **AC-7** Requests without named arguments are unchanged
  - WHEN a request carries no URL query parameters and no (or an empty) body
  - THEN it is served by the existing path, `Environment::evaluate`, sharing the query asset
    across requests as today
- **AC-8** Different named arguments are not shared
  - WHEN two requests for the same path query carry different named arguments
  - THEN each is evaluated with its own arguments; neither is served the other's value (Q4)
- **AC-9** Malformed body
  - WHEN a non-empty POST body is not JSON, or is JSON but not an object
  - THEN the response is `400 ParameterError`
- **AC-10** Reserved names
  - WHEN a URL query parameter or body member name begins with `_`
  - THEN the response is `400 ParameterError` stating the name is reserved for route options (Q2)

Systems touched: `liquers-axum` only (`src/query/handlers.rs`, tests). No core change is expected;
Phase 2 confirms that `ParameterValue::from_string`, `Recipe::to_plan` and `AssetManager::apply`
suffice. Documentation: `specs/reference/WEB_API_SPECIFICATION.md` §Query API (replaces "logged but
not used"); the source issue is closed at Phase 5.

### Design Readiness

`needs-decision` — each question has a recommended answer, assumed by the scenarios above.

- **Q1 (needs decision)** An argument given positionally *and* by name: **error** (recommended;
  AC-5) — the request is ambiguous and the error is cheap to fix — or the name silently wins, as a
  recipe override does.
- **Q2 (needs decision)** Reserved names: the Query API has no route options today, so every query
  parameter is an argument name, and names beginning with `_` are **rejected as reserved**
  (recommended; AC-10), keeping a namespace for future options (`_format`, `_timeout`) without a
  breaking change. Alternative: reserve nothing, and later options collide with argument names.
- **Q3 (needs decision)** Links by name (`{"links": {"a": "<query>"}}`, as a recipe has): **out of
  scope** (recommended) — a link can already be written positionally as `~X~...~E`, and a flat
  argument object leaves no unambiguous place for links without a wrapper. Filed as a follow-up
  issue if deferred.
- **Q4 (decided by the codebase)** Caching: a request with named arguments is evaluated **ad hoc**
  (`AssetManager::apply` of a `Recipe` carrying `arguments`), which inserts the asset in no cache
  and never reuses it — so AC-8 holds by construction and no cache-key scheme is invented. The cost
  is that the last action is recomputed per request; its predecessor is still the shared, cached
  query asset. Caching overridden results is not attempted.
- **Q5 (recommended, low stakes)** JSON strings for typed arguments are converted (AC-2), so a form
  that posts every field as a string works; a JSON number for a string argument is a
  `ConversionError` rather than stringified.
