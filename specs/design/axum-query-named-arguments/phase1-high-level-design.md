# Phase 1: High-Level Design — Argument overrides over HTTP and WebSocket

## Purpose

Let a client name arguments of a query's **last action** instead of spelling every one
positionally, escaped, in the URL path — on the Query API, on the Assets API's `q/data`,
`q/entry` and `q/submit`, and on the WebSocket, through a new **submit** that keeps the
evaluation it starts so the client can follow it. Core already has the mechanism —
`Recipe.arguments` / `links` applied through `Plan::override_value` / `override_link`
(`liquers-core/src/recipes.rs` `Recipe::to_plan`); this design connects the requests to it, and
names and documents the difference it makes: an evaluation with overrides is **unmanaged**.

## Terminology

These terms are defined here and, at implementation, in `reference/ASSET_LIFECYCLE.md`, which the
Web API reference and guide cite. Neither exists in the reference today: "ad-hoc" is used there
for both `apply` assets and top-level queries (`ASSET_LIFECYCLE.md:66`, `ENVIRONMENT_CONFIG.md:65`),
and "unmanaged" appears once, undefined (`api/DOC_08_RECIPES_PLANS.md:259`).

- **Argument override** (*override*): a value, or a link, for a named argument of the query's
  last action, supplied outside the query text — by an HTTP request (URL query string, POST JSON
  body), a WebSocket `submit`, or a recipe's `arguments` / `links`. A positional parameter written
  in the query is not an override. A request **carries overrides** when at least one override name
  is present; options (names beginning with `_`, other than `_link_<name>`) are not overrides.
- **Managed asset**: an asset the asset manager holds in its key map or query map, so it can be
  found by its address — by `q/info`, `key/info`, `q/cancel`, or a later `submit`, `data` or
  subscription, which join it instead of starting another evaluation.
- **Unmanaged asset**: an asset in neither map. Only the holder of its `AssetRef` can see it: the
  HTTP request evaluating it, or a WebSocket subscription. An asset is unmanaged when:
  1. the request carries overrides — the query is evaluated with `AssetManager::apply`, which is
     "inserted into no key or query cache, is never reused" (`assets.rs:5664-5673`);
  2. its plan is volatile (`get_volatile_query_asset`, `assets.rs:7883`);
  3. a command in it declares `cached: false` (`QueryRetention::decide`, `assets.rs:4973`);
  4. `query_cache_strategy` is `none` and nobody else holds it.

  **Recipes are the edge case.** A keyed recipe with `arguments` yields a *managed* keyed asset
  (in the key map); only the recipe's query-with-overrides has no query identity, and the prefix
  before its last action is still a shared query asset (`Plan::cut_predecessor`).

"Immediate" is avoided: `ImmediateAssetManager` / `EvalMode::Inline` already mean inline
evaluation, which is unrelated to whether an asset is held.

## Problem Example

`ns-search/search` takes `query: string = ""`, `rank: bool = true`, `case_sensitive: bool = false`,
`limit: int = 50`, `fields: multiple string`. A client wanting 20 results for `expiration safety`
must write today:

```
GET /q/-R/docs/catalog.csv/-/ns-search/search-expiration~.safety-true-false-20
```

— escaping the space and repeating `rank` and `case_sensitive` at their defaults. These are
accepted and **silently ignore** the arguments, running the search with `query = ""`, `limit = 50`:

```
GET  /q/-R/docs/catalog.csv/-/ns-search/search?query=expiration%20safety&limit=20
POST /q/-R/docs/catalog.csv/-/ns-search/search      {"query": "expiration safety", "limit": 20}
```

After this design both run with `query = "expiration safety"`, `limit = 20`. (Path queries
validated with `liquers-validate`.)

**The second half of the problem.** A long search should be submitted and followed, not awaited.
With overrides that is impossible over HTTP even after the change above: `q/submit` would start an
unmanaged asset, and `q/info` / `q/cancel` look the query up without creating one
(`lookup_query_asset`, `query_handlers.rs:84-172`) and answer 404 `NotAvailable`. A WebSocket
`subscribe` requests the asset itself (`websocket.rs:266`, `get_asset`) and the subscription task
holds the `AssetRef` (`websocket.rs:335-404`) — which is why it already follows a *volatile* asset
— but it carries no overrides. Nothing can start an evaluation with overrides and follow it.

## Scope and Acceptance Criteria

Names beginning with `_` are reserved for options: `_link_<name>` (a link override) and `_format`
(an entry route's serialization). Scenarios AC-1 to AC-12 hold on every route in scope that
evaluates: `{base}` (Query API), `q/data`, `q/entry`, `q/submit` and WebSocket `submit`.

- **AC-1** URL query parameters are overrides
  - WHEN a request carries `?a=x&b=y` and `a`, `b` are arguments of the last action
  - THEN it is evaluated as if the last action had `a = x`, `b = y`, each string converted by the
    argument's `ArgumentType` as a path parameter would be, the rest positional or default
- **AC-2** A POST JSON object body carries overrides
  - WHEN a POST carries a JSON object body `{"a": <json>, ...}`
  - THEN each member overrides argument `a` with that JSON value
- **AC-3** Variadic arguments
  - WHEN a `multiple` argument is given by a repeated URL key or a JSON array
  - THEN it receives one element per occurrence or entry, in order
- **AC-4** Unknown or non-overridable name
  - WHEN a name is not an argument of the last action, names an `injected` argument, or the query
    has no action
  - THEN `400 ParameterError` naming it, and nothing is evaluated
- **AC-5** An override replaces the path value
  - WHEN an argument is given in the path and as an override (e.g.
    `…/search----20?query=expiration%20safety`: empty slots leave `query` as `""` and `rank`,
    `case_sensitive` at their defaults)
  - THEN the override is used
- **AC-6** Conversion failure
  - WHEN an override cannot be converted (`?limit=abc`)
  - THEN `422 ConversionError`, and nothing is evaluated
- **AC-7** No overrides, no change — and no accidental unmanaged evaluation
  - WHEN a request carries no override: no query string, an empty `?`, an empty body, `{}`, or
    options only (`_format`)
  - THEN it is served exactly as today, as a managed asset where today's rules make it one
- **AC-8** Different overrides are not shared
  - WHEN two requests for one query carry different overrides
  - THEN each is evaluated with its own; neither is served the other's value
- **AC-9** Malformed body or duplicated name
  - WHEN a non-empty body is not a JSON object, or a name is in both the query string and the body
  - THEN `400 ParameterError`
- **AC-10** Reserved names
  - WHEN a name begins with `_` and is not a defined option
  - THEN `400 ParameterError` stating the name is reserved
- **AC-11** Link overrides
  - WHEN `_link_a=<query>` is given
  - THEN argument `a` is computed from the linked query; an unparsable query is `400 ParseError`;
    `a` given both as a value and as a link is `400 ParameterError`
- **AC-12** Typed-JSON conversion is logged
  - WHEN a body value's JSON type does not match the argument's type but converts (`"20"` for an
    `int`, `20` for a `string`)
  - THEN it is converted and the log carries a warning naming the argument and both types; URL
    values, always text, convert without a warning
- **AC-13** Entry routes answer value and metadata
  - WHEN an entry route is requested (`q/entry`; `{entry_base}` if kept, Q7)
  - THEN the answer is a `DataEntry` — data plus metadata, log included — negotiated by
    `_format`, then `Accept`, then CBOR
- **AC-14** An entry route keeps the log of a failure
  - WHEN the evaluation behind an entry route fails
  - THEN the error envelope carries the failed asset's metadata in `error.metadata`
- **AC-15** `q/data` and `q/entry` accept overrides
  - WHEN `GET` (query string) or `POST` (body, query string, or both) `q/data` / `q/entry` is
    requested with overrides
  - THEN the value is evaluated with them, as on `{base}`
- **AC-16** `q/submit` with overrides starts an unmanaged evaluation
  - WHEN `q/submit` carries overrides
  - THEN it starts the evaluation and answers its `AssetInfo` marked unmanaged, with a message
    that it cannot be observed over HTTP and pointing to WebSocket `submit`; a later `q/info` of
    the same query does not report it
- **AC-17** WebSocket submit starts and follows an evaluation
  - WHEN a client sends `submit` with a query, an id, and optionally overrides
  - THEN the asset is requested — unmanaged with overrides, as `subscribe` would without — and the
    connection follows it: `Initial`, progress and the terminal notification, each carrying the id
- **AC-18** Independent submits on one connection
  - WHEN one connection submits the same query twice with different overrides and ids
  - THEN two evaluations are followed independently, told apart by id
- **AC-19** Cancelling a submitted evaluation
  - WHEN the client sends `cancel` with a submit's id
  - THEN that evaluation is cancelled as `q/cancel` would cancel a managed one; unsubscribing or
    disconnecting stops following without cancelling, as today
- **AC-20** Managed or not is visible
  - WHEN any route in scope answers about an evaluation it started
  - THEN the client can tell whether the asset is managed
- **AC-21** Recipe overrides keep their keyed asset managed
  - WHEN a keyed recipe with `arguments` is requested by key (HTTP, WebSocket)
  - THEN its asset is managed — found by `key/info`, joined by a later `key/submit` — as today

Non-goals: overrides on the `key/` family (a key's asset is defined by its recipe and globally
shared; overrides would give one key two values); caching or sharing an asset with overrides;
overrides on `q/info`, `q/metadata`, `q/version`, `q/cancel` (they observe managed assets, which
an overridden evaluation never is); a plan-preview route.

## Core Interactions

- **Web/API**: Query API route(s); Assets API `q/data`, `q/entry` (gain `POST`), `q/submit`;
  WebSocket protocol (`submit`, `cancel`, ids on notifications).
- **Assets**: `AssetManager::apply` for overridden evaluations; `get_asset` otherwise.
- **Plans / recipes**: `Recipe::to_plan` gains type conversion with `Plan::init_warning` (AC-12;
  the only route into the asset's log).
- **Commands**: argument metadata (`ArgumentType`, `multiple`, `injected`) to resolve names.

## Crate Placement

`liquers-axum` for the HTTP and WebSocket mapping; `liquers-core` `recipes.rs` for the conversion
and warnings (also benefiting `recipes.yaml` arguments). No dependency change.

## Documentation Intent

Checked against HEAD on 2026-10-12. Each statement below becomes false when this ships, so all are
updated in the implementation PR — `reference/` must be true at HEAD.

- Reference, `reference/ASSET_LIFECYCLE.md`: define *argument override*, *managed* and
  *unmanaged asset* (the four causes, the recipe edge case).
- Reference, `reference/WEB_API_SPECIFICATION.md`: §2 a convention section on overrides and
  options; §5.3 `q/data`, `q/entry` (POST), `q/submit` with overrides and AC-16; §5.8 the `submit`
  and `cancel` messages and ids; §7 rows no longer "logged but not used"; one table of **when an
  asset is managed** and what cannot then follow it (overrides, volatility, `cached: false`,
  `query_cache_strategy: none`) with the workaround — WebSocket `submit` (or `subscribe` without
  overrides).
- Guide, `guides/WEB_API_GUIDE.md`: §4 replaces "`?name=value` is **not** passed to commands"
  with overrides (curl, Python `params=` / `json=`, `_link_`); §5 adds "following an evaluation
  with overrides: submit on the WebSocket", and says plainly that overrides make the evaluation
  unmanaged; §8 (WebSocket) the `submit` message.
- Reference, recipe documentation (`api/DOC_08_RECIPES_PLANS.md`): mismatched JSON types in
  `arguments` convert, with a warning.
- Other: `liquers-axum/README.md` endpoint list.

## Design Readiness

`needs-decision`.

**Decided by the user (2026-10-12):** Q1 an override replaces the path value (AC-5); Q2 `_` names
reserved (AC-10); Q3 links as `_link_<name>` (AC-11); Q5 convert and warn (AC-12; cost: the
conversion moves into `Recipe::to_plan`, ~60–80 lines, one registry lookup per override); Q6
`q/data`, `q/entry`, `q/submit` in scope, and following an overridden evaluation solved here by a
WebSocket submit (AC-15 to AC-19); Q8 `q/submit` accepts overrides (AC-16).

**Decided by the codebase:** Q4 an evaluation with overrides is unmanaged (`apply`) — verified:
`apply` inserts into no map; `q/info` & co. use `lookup_query_asset`; the WebSocket holds the
`AssetRef` per subscription task. Prior work: `design/axum-assets-endpoints/phase2-architecture.md:88-95`
already decided that unmapped (volatile, `cached: false`) assets are followed on the WebSocket; no
document designs a WebSocket submit or overrides on the Assets API, and the follow-up Q6 proposed
earlier was never filed — it is absorbed here instead.

**Needs decision (recommended answer assumed by the scenarios):**

- **Q7** Keep the Query API entry route `{entry_base}` (`/liquer/qe/`)? Recommended: **drop it** —
  `q/entry` with overrides (AC-13 to AC-15) gives the same answer through an existing route.
- **Q9** WebSocket submit shape. Recommended: a **`submit` action** on the existing `ws/q`
  (`{"action":"submit","id":"s1","query":"…","arguments":{…}}`, links as `_link_<name>` members),
  explicit so an empty `arguments` never changes what `subscribe` does. Alternative: an `arguments`
  field on `subscribe`, or a separate `ws/q/submit` endpoint.
- **Q10** Subscription identity. Recommended: the client supplies `id` on `submit` (required) and
  every notification for it echoes `id`; today's subscriptions stay keyed by address.
- **Q11** Cancel. Recommended: a `cancel` action by id (AC-19), the only way to cancel an
  unmanaged evaluation.
- **Q12** `?format=` on `q/entry` today is a route option, so `format` would collide with an
  argument of that name. Recommended: keep `format` as an accepted alias of `_format` on `q/entry`
  for compatibility; an argument named `format` is then given in a POST body.
- **Q13** How AC-20 is shown. Recommended: `"managed": bool` in the `submit` answer and the
  WebSocket notifications, and a `x-liquers-managed` header on value answers; `AssetInfo` itself is
  unchanged (core type).
- **Q14** Terms: *argument override*, *managed* / *unmanaged asset*, as defined above.
- **A1 (assumption)** The same name in both the query string and the body is an error (AC-9).

## Scope Changes

- **2026-10-12 — Assets API and WebSocket submit.** The design started as the Query API only
  (issue `QUERY-API-ARGUMENTS-ONLY-IN-QUERY-PATH`, complexity M). The user extended it to `q/data`,
  `q/entry`, `q/submit`, and to following an overridden evaluation, which only a WebSocket submit
  can provide; the `qe` entry route came with Q5 and may go with Q7. Added AC-13 to AC-21, the
  terminology, and documentation in four documents. Size: L; converted from the compact to the
  full form (each phase in its own file). Approval: pending at this gate.

## References

- Issue `specs/issues/QUERY-API-ARGUMENTS-ONLY-IN-QUERY-PATH.md`
- `specs/design/axum-assets-endpoints/` — unmapped assets, the WebSocket subscription model
- `specs/reference/WEB_API_SPECIFICATION.md` §5.3, §5.8, §7; `specs/reference/ASSET_LIFECYCLE.md`
- `specs/issues/AXUM-WEBSOCKET-HARDENING.md` — limits that also bound `submit`
