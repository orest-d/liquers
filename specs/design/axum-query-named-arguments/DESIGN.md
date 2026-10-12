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
area: [axum, core/assets]
issues: [QUERY-API-ARGUMENTS-ONLY-IN-QUERY-PATH]
created: 2026-10-11
---
# Named arguments for the Query API, from the URL query string and a JSON body

## Phase 1: High-Level Design

### Purpose

Let an HTTP client of the Query API (`GET/POST {base}{*query}`, `liquers-axum/src/query/`) name
arguments of the query's **last action** instead of spelling every one positionally, escaped, in
the URL path: as URL query parameters on either method, and as a JSON object body on `POST`.
A companion entry route answers the value together with its metadata, so the
evaluation's log is not thrown away. Core already has the mechanism — `Recipe.arguments` applied through `Plan::override_value`
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

In scope: the Query API — its existing route `{base}{*query}` and a new **entry route**
(`{entry_base}{*query}`, e.g. `/liquer/qe/…`) that answers the value *and* its metadata, so the
log, including the warnings below, is no longer thrown away. Out of scope: the Assets API
(`{assets}/q/…`) and the WebSocket (Q6). No change to the query language: named arguments exist
only in the HTTP request (`command-declaration` decision C3 is untouched).

Names beginning with `_` are reserved for options (Q2). This design defines two: `_link_<name>`
(a link to argument `<name>`, Q3) and `_format` (the entry route's serialization, which the Assets
API spells `?format=` — a name an argument could have).

- **AC-1** URL query parameters name arguments
  - WHEN `GET` or `POST {base}<query>?a=x&b=y` is requested and `a`, `b` are arguments of the
    last action
  - THEN the value is computed as if the last action had `a = x`, `b = y`, each string converted
    by the argument's `ArgumentType` exactly as a path parameter would be, the rest positional or
    default
- **AC-2** A POST JSON object body names arguments
  - WHEN `POST {base}<query>` carries a JSON object body `{"a": <json>, ...}`
  - THEN each member overrides argument `a` of the last action with that JSON value
- **AC-3** Variadic arguments
  - WHEN a `multiple` argument is named by a repeated URL key (`?f=a&f=b`) or a JSON array
    (`{"f": ["a", "b"]}`)
  - THEN the argument receives one element per occurrence or array entry, in order
- **AC-4** Unknown or non-overridable name
  - WHEN a name is not an argument of the last action, names an `injected` argument, or the query
    has no action at all
  - THEN the response is `400` with error type `ParameterError` naming the argument, and nothing
    is evaluated
- **AC-5** A named argument overrides the path (Q1)
  - WHEN an argument is given positionally in the path and also by name — e.g.
    `…/ns-search/search----20?query=expiration%20safety`, where the empty slots leave `query` as
    `""` and `rank`, `case_sensitive` at their defaults (validated: the plan shows exactly that)
  - THEN the named value is used, and the path value is ignored
- **AC-6** Conversion failure
  - WHEN a named value cannot be converted to the argument's type (`?limit=abc`,
    `{"limit": "abc"}`)
  - THEN the response is `422` with error type `ConversionError`, and nothing is evaluated
- **AC-7** Requests without named arguments are unchanged
  - WHEN a request carries no URL query parameters and no (or an empty) body
  - THEN it is served by the existing path, `Environment::evaluate`, sharing the query asset
    across requests as today
- **AC-8** Different named arguments are not shared
  - WHEN two requests for the same path query carry different named arguments
  - THEN each is evaluated with its own arguments; neither is served the other's value (Q4)
- **AC-9** Malformed body or duplicated name
  - WHEN a non-empty POST body is not JSON, or not a JSON object, or a name appears both in the
    URL query string and in the body
  - THEN the response is `400 ParameterError`
- **AC-10** Reserved names
  - WHEN a name begins with `_` and is neither `_link_<name>` nor, on the entry route, `_format`
  - THEN the response is `400 ParameterError` stating the name is reserved
- **AC-11** Links by name (Q3)
  - WHEN `_link_a=<query>` is given (URL or body; in a body its value is a JSON string)
  - THEN argument `a` is computed from the linked query, as a recipe's `links` entry or an
    `~X~…~E` path parameter would be; a query that does not parse is `400 ParseError`, and naming
    `a` both as a value and as a link is `400 ParameterError`
- **AC-12** Typed-JSON conversion is logged (Q5)
  - WHEN a body value's JSON type does not match the argument's type but converts — `"20"` for
    an `int`, `20` for a `string`, `"true"` for a `bool`
  - THEN it is converted and the evaluation's log carries a warning naming the argument, the
    given and the expected type; URL values, which are always text, convert without a warning
- **AC-13** The entry route answers value and metadata
  - WHEN `GET` or `POST {entry_base}<query>` is requested, with or without named arguments
  - THEN the answer is a `DataEntry` (data plus metadata, log included), negotiated by `_format`,
    then `Accept`, then CBOR, as the Assets API's `q/entry` is; the timeout applies as on `{base}`
- **AC-14** The entry route keeps the log of a failure
  - WHEN the evaluation behind `{entry_base}` fails
  - THEN the §3 error envelope carries the failed asset's metadata, log included, in
    `error.metadata`

Systems touched: `liquers-axum` (`src/query/`, tests) and `liquers-core` `Recipe::to_plan`
(`src/recipes.rs`), which gains the type conversion and its warnings so that they reach the
asset's log through `Plan::init_warning` — the only route into it (Q5). Crate placement follows the
rule that core keeps the mechanism and axum the HTTP mapping.

Documentation intent (checked against HEAD on 2026-10-12; each statement below becomes false when
this ships, so all are updated in the implementation PR, not before — `reference/` must be true at
HEAD):

| Document | Today | Change |
|---|---|---|
| `specs/reference/WEB_API_SPECIFICATION.md` §7 | `POST` row: body "accepted and logged but not used" | Both rows name the argument sources; new §7.1 *Named arguments* (sources, conversion and warnings, variadics, override of the path, `_` options, errors, ad-hoc evaluation: not cached, not visible to `q/info`); new §7.2 *Entry route*; `## History` row and `reviewed:` |
| `specs/guides/WEB_API_GUIDE.md` §4 *Parameters* | "A `?name=value` query string is **not** passed to commands; … `POST` … does not use it yet" | A short "naming arguments" part: curl `?query=…&limit=…`, `-d '{…}'` and `_link_…` forms, Python `requests.get(…, params=…)` / `requests.post(…, json=…)`, the entry route for reading the log, and the §5 limitation (Q6); examples run against the example server; `## History` row and `reviewed:` |
| `specs/reference/` recipe documentation (located in Phase 2) | recipe `arguments` must already have the argument's JSON type | Mismatched JSON types convert, with a warning |
| `liquers-axum/README.md` §Query Execution API | "Execute query with optional JSON body" | Named arguments and the entry route, linking §7.1–7.2 |

The source issue is closed at Phase 5.

### Design Readiness

`needs-decision` — Q1, Q2, Q3 and Q5 were decided by the user on 2026-10-12; Q6 and the
assumptions A1–A2 remain open with a recommended answer.

- **Q1 (decided: override)** A named argument overrides a positional one (AC-5): a client reaches a
  later argument by leaving earlier path slots empty (they take defaults, or `""` for a string)
  and naming the one it wants; refusing the combination would make some argument sets
  inexpressible.
- **Q2 (decided: reserve)** Names beginning with `_` are reserved for options (AC-10).
- **Q3 (decided: `_link_<name>`)** A link is named by the reserved option `_link_<name>` (AC-11),
  in the URL or the body, which keeps the body a flat object.
- **Q4 (decided by the codebase)** Caching: a request with named arguments is evaluated **ad hoc**
  (`AssetManager::apply` of a `Recipe` carrying `arguments` / `links`), which inserts the asset in
  no cache and never reuses it — so AC-8 holds by construction and no cache-key scheme is invented.
  The last action is recomputed per request; its predecessor is still cut off as the shared,
  cached query asset, because the evaluation starts from an empty state
  (`interpreter::finalize_plan`). Caching overridden results is not attempted.
- **Q5 (decided: convert and warn)** A mismatched JSON type is converted, with a warning in the
  log (AC-12). Cost, investigated: the log is reachable only through the plan
  (`Plan::init_warning` → asset log on first application), and the plan is built inside the
  ad-hoc asset by `Recipe::to_plan`, so axum cannot add to it. The conversion therefore moves into
  `Recipe::to_plan`: per overridden slot, look up the argument's `ArgumentType` (one registry
  lookup per named argument, only when overrides exist — negligible at run time), convert, and
  `init_warning`. Roughly 60–80 lines in `liquers-core/src/recipes.rs` plus tests. Side effect:
  `recipes.yaml` arguments with a mismatched JSON type, which today fail inside the command with a
  `ConversionError`, convert with a warning instead — strictly more permissive, and documented.
  The warning is visible only where metadata is served, which is why the entry route (AC-13) is
  in scope.
- **Q6 (needs decision)** The timeout message and guide §5 send long evaluations to the Assets API
  (`q/submit`, `q/info`, `ws/q`), which takes no named arguments, and an ad-hoc evaluation is in no
  cache `q/info` could find. Recommended: **out of scope**; the guide and reference say so, and a
  follow-up issue is filed (named arguments on `q/submit` need an asset identity that includes the
  overrides). Alternative: widen this design to the Assets API's `q/` family.
- **A1 (assumption)** The same name in both the URL query string and the body is an error (AC-9)
  rather than one source winning: the override rule of Q1 is about a positional versus a named
  value, while two named values for one argument are a client bug.
- **A2 (assumption)** The entry route is opt-in, mounted by `QueryApiBuilder::with_entry_base(path)`
  (e.g. `"/liquer/qe/"`), since the base path is arbitrary and a sibling cannot be derived from it.
  The example server mounts it.
