# Phase 1: High-Level Design — Argument overrides over HTTP and WebSocket

## Purpose

Let a client name arguments of a query's **last action** instead of spelling every one
positionally, escaped, in the URL path — on the Query API, on the Assets API's `q/data`,
`q/entry` and `q/submit`, and on the WebSocket's `subscribe`, which keeps the evaluation it starts
so the client can follow it. Core already has the mechanism —
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
  Over HTTP and the WebSocket an override applies to that request only; in a recipe it is part of
  the recipe.
- **Managed asset**: an asset the asset manager holds in its key map or query map, so it can be
  found by its address — by `q/info`, `key/info`, `q/cancel`, or a later `submit`, `data` or
  subscription, which join it instead of starting another evaluation.
- **Unmanaged asset**: an asset in neither map. Only the holder of its `AssetRef` can see it: the
  HTTP request evaluating it, or the WebSocket subscription that requested it. Nothing else can
  observe, join or cancel it, and `q/submit` of one returns its status but leaves no way to read
  its result later. An asset is unmanaged when:
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
it requested itself — but it carries no overrides. Nothing can start an evaluation with overrides
and follow it.

## Scope and Acceptance Criteria

Names beginning with `_` are reserved for options. Two are defined, each with one meaning wherever
it appears: `_link_<name>` (a link override) and `_format` (the encoding of a `DataEntry` envelope —
`cbor`, `json` or `bincode` — on every route that reads or writes one; it replaces today's `format`,
AC-22). The value's own format stays where it is: the query's filename extension, and the
`data_format` metadata field. Scenarios AC-1 to AC-12 hold on every route in scope that evaluates:
`{base}` (Query API), `q/data`, `q/entry`, `q/submit` and WebSocket `subscribe`.

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
- **AC-13** The entry route answers value and metadata
  - WHEN `q/entry` is requested, with or without overrides
  - THEN the answer is a `DataEntry` — data plus metadata, log included — negotiated by
    `_format`, then `Accept`, then CBOR
- **AC-14** The entry route keeps the log of a failure
  - WHEN the evaluation behind `q/entry` fails
  - THEN the error envelope carries the failed asset's metadata in `error.metadata`
- **AC-15** `q/data` and `q/entry` accept overrides
  - WHEN `GET` (query string) or `POST` (body, query string, or both) `q/data` / `q/entry` is
    requested with overrides
  - THEN the value is evaluated with them, as on `{base}`
- **AC-16** `q/submit` with overrides starts an unmanaged evaluation
  - WHEN `q/submit` carries overrides
  - THEN it starts the evaluation and answers its `AssetInfo` marked unmanaged, with a message
    that neither its status nor its result can be read later; a later `q/info` of the same query
    does not report it. (Its use is a query run for a side effect, such as writing to a database;
    to read the result, use `q/data` or WebSocket `subscribe`.) The same holds for `q/submit` of a
    volatile or otherwise unmanaged query without overrides, as today
- **AC-17** WebSocket subscribe with overrides starts and follows an evaluation
  - WHEN a client sends `subscribe` with a query and a non-empty `arguments` object
  - THEN the asset is requested with those overrides (unmanaged) and the connection follows it —
    `Initial`, progress, the terminal notification — and the reply and every notification carry a
    subscription id; a `subscribe` with no `arguments`, or `{}`, behaves exactly as today
- **AC-18** Independent subscriptions on one connection
  - WHEN one connection subscribes to the same query twice with different overrides
  - THEN two evaluations are followed independently, told apart by their ids
- **AC-19** Cancelling and unsubscribing by id
  - WHEN the client sends `cancel` (or `unsubscribe`) with a subscription id
  - THEN that evaluation is cancelled as `q/cancel` would cancel a managed one (or no longer
    followed); disconnecting stops following without cancelling, as today; an id unknown on this
    connection is an `Error` reply that reveals nothing about other connections
- **AC-20** Managed or not is visible
  - WHEN any route in scope answers about an evaluation it started
  - THEN the client can tell whether the asset is managed
- **AC-21** Recipe overrides keep their keyed asset managed
  - WHEN a keyed recipe with `arguments` is requested by key (HTTP, WebSocket)
  - THEN its asset is managed — found by `key/info`, joined by a later `key/submit` — as today
- **AC-22** `_format` replaces `format`
  - WHEN a route that reads or writes a `DataEntry` (Store `entry`, Assets `q/entry`, `key/entry`,
    `key/recover`, Recipes `entry`) is given `_format=json` — or the old `format=json`
  - THEN `_format` selects the encoding as `format` did; `format` is not an alias: on `q/entry` it is
    an override like any other name, and on routes without overrides it is `400 ParameterError`
    naming `_format`, so a stale client fails loudly instead of silently getting CBOR

Non-goals: overrides on the `key/` family (a key's asset is defined by its recipe and globally
shared; overrides would give one key two values); caching or sharing an asset with overrides;
overrides on `q/info`, `q/metadata`, `q/version`, `q/cancel` (they observe managed assets, which
an overridden evaluation never is); a plan-preview route.

## Core Interactions

- **Web/API**: Query API route; Assets API `q/data`, `q/entry` (gain `POST`), `q/submit`; every
  entry route (`_format`); WebSocket protocol (`arguments` on `subscribe`, `cancel`, ids).
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
  with overrides (curl, Python `params=` / `json=`, `_link_`), and says plainly that overrides make
  the evaluation unmanaged. §5 explains `submit` of an unmanaged asset (overrides, volatile,
  `cached: false`): it cannot be subscribed to or polled, so there is no way to get its result —
  it is still useful when the query has a side effect (writing to a database, a store, a file);
  to get a result, use `q/data` (waits) or WebSocket `subscribe` with `arguments` (follows). §8
  (WebSocket) `arguments`, ids and `cancel`. Every `?format=` example becomes `?_format=`.
- Reference, recipe documentation (`api/DOC_08_RECIPES_PLANS.md`): mismatched JSON types in
  `arguments` convert, with a warning.
- Other: `liquers-axum/README.md` endpoint list and its `?format=` lines;
  `liquers-axum/examples/WEBSOCKET_EXAMPLE.md` and `examples/assets_recipes_basic.rs` (`_format`).

## Design Readiness

`needs-decision` — Q15 and A3 remain.

**Decided by the user (2026-10-12):** Q1 an override replaces the path value (AC-5); Q2 `_` names
reserved (AC-10); Q3 links as `_link_<name>` (AC-11); Q5 convert and warn (AC-12; cost: the
conversion moves into `Recipe::to_plan`, ~60–80 lines, one registry lookup per override); Q6
`q/data`, `q/entry`, `q/submit` in scope and following an overridden evaluation solved here
(AC-15 to AC-19); Q7 no separate Query API entry route — `q/entry` serves (AC-13, AC-14); Q8
`q/submit` accepts overrides, for side effects (AC-16); Q9 overrides go on `subscribe` as
`arguments`, no separate submit action (AC-17); Q11 cancel by id (AC-19); Q12 `_format`, no alias,
one meaning everywhere (AC-22); Q13 `"managed": bool` in the `q/submit` answer, the `subscribe`
reply and notifications, and an `x-liquers-managed` header on value answers (AC-20); Q14 the terms
above; A1 a name in both query string and body is an error (AC-9).

**Decided by the codebase:** Q4 an evaluation with overrides is unmanaged (`apply`) — verified:
`apply` inserts into no map; `q/info` & co. use `lookup_query_asset`; the WebSocket holds the
`AssetRef` per subscription task. Prior work: `design/axum-assets-endpoints/phase2-architecture.md:88-95`
decided that unmapped (volatile, `cached: false`) assets are followed on the WebSocket; no document
designs overrides on the Assets API or the WebSocket.

**Q10 — subscription ids, assessed (user asked for a critical check):**

- *Managed assets need no id — verified, with one caveat.* A managed asset is found by its
  address: a query through the query map (`lookup_query_asset`), a key through the key map; a
  pure-key query goes the key way. The caveat: an address names *the current asset for that
  address*, not one evaluation — `get_asset` evicts an `Expired`, `Error`, `Cancelled` or
  `Volatile` entry and creates a fresh one (`assets.rs:8037-8042`). For a managed asset that is
  the wanted meaning, and an id would be worse: after an eviction it would pin a stale evaluation
  and disagree with `q/info`. Ids are therefore issued **only for unmanaged assets**.
- *Unmanaged is not always known in advance.* Overrides are known before the request; volatility,
  `cached: false` and a non-retaining cache strategy only after planning. The server issues an id
  whenever the requested asset turns out unmanaged (it is not what `lookup_query_asset` returns for
  the address). A volatile `subscribe` without overrides keeps working by address, as today, and
  additionally receives an id.
- *Scope of an id — the security question.*
  - **(a) Connection-scoped (recommended).** Ids name subscriptions of one connection and are
    looked up only in that connection's table, so another connection cannot reach them whatever
    it guesses — the protection does not depend on the id being secret. The server still generates
    a UUID v4, which avoids collisions and costs nothing. Limit: no following across a reconnect,
    and `q/submit` over HTTP stays fire-and-forget.
  - **(b) Server-wide registry.** Unmanaged `AssetRef`s kept in a map keyed by UUID v4; `q/submit`
    returns the id; any connection may `subscribe` / `cancel` by it (and HTTP `id/info`, `id/data`
    could follow). It solves submit-then-follow over HTTP. Critically: the id becomes a bearer
    capability — whoever has it reads the result and can cancel. UUID v4's 122 random bits defeat
    guessing, but ids leak through logs, proxies, URLs and shared links, and there is no
    authentication or ownership to fall back on (`CORE-SESSION-AND-KEY-ACL`). It is not worse than
    today for the *query* — a managed asset is readable by anyone who knows its query — but
    override values (search terms, parameters) can be more sensitive than the query. It also needs
    a retention policy (TTL after the terminal status, a capacity cap against memory exhaustion),
    which stretches what "unmanaged" and "volatile: used once" mean.
- **Q15 (needs decision)** (a) or (b). Recommended: **(a)** here, and (b) filed as a follow-up
  issue carrying this analysis, to be designed once sessions exist so an id can be checked against
  its owner.
- **A3 (assumption)** Replacing `format` by `_format` on every entry route breaks clients that send
  `?format=`. In this repository only tests, the README, the guide and two examples do; no binding
  (`liquers-web`, `liquers-py`) sends it. Taken as intended by "no need to keep the alias"; AC-22
  makes a stale `format` a loud error rather than a silent default.

## Scope Changes

- **2026-10-12 — Assets API and WebSocket submit.** The design started as the Query API only
  (issue `QUERY-API-ARGUMENTS-ONLY-IN-QUERY-PATH`, complexity M). The user extended it to `q/data`,
  `q/entry`, `q/submit`, and to following an overridden evaluation through WebSocket `subscribe`;
  the `qe` entry route came with Q5 and went with Q7; `_format` replaces `format` on every entry
  route (Q12). Added AC-13 to AC-22, the
  terminology, and documentation in four documents. Size: L; converted from the compact to the
  full form (each phase in its own file). Approval: pending at this gate.

## References

- Issue `specs/issues/QUERY-API-ARGUMENTS-ONLY-IN-QUERY-PATH.md`
- `specs/design/axum-assets-endpoints/` — unmapped assets, the WebSocket subscription model
- `specs/reference/WEB_API_SPECIFICATION.md` §5.3, §5.8, §7; `specs/reference/ASSET_LIFECYCLE.md`
- `specs/issues/AXUM-WEBSOCKET-HARDENING.md` — limits that also bound `submit`
