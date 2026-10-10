---
title: Liquers Web API Specification
kind: reference
audience: internal
area: [axum, web]
reviewed: 2026-10-10
---
# Liquers Web API Specification

The HTTP and WebSocket interface of `liquers-axum`, as implemented at HEAD. Every route, shape and
status code below is checked against the code and exercised by the route suites in
`liquers-axum/tests/` (`store_api_routes.rs`, `assets_api_endpoints.rs`, `assets_websocket.rs`,
`recipes_api_routes.rs`, `query_api_routes.rs`). The design of the Assets API is
`specs/design/axum-assets-endpoints/`.

For a task-oriented introduction with curl and Python examples, see
[`guides/WEB_API_GUIDE.md`](../guides/WEB_API_GUIDE.md).

## Table of Contents

1. [Overview](#1-overview)
2. [Conventions](#2-conventions)
3. [Errors](#3-errors)
4. [Store API](#4-store-api)
5. [Assets API](#5-assets-api)
6. [Recipes API](#6-recipes-api)
7. [Query API](#7-query-api)
8. [Assembling a server](#8-assembling-a-server)
9. [Access control](#9-access-control)
10. [Implementation notes](#10-implementation-notes)
- [Appendix A: Migration from Python liquer](#appendix-a-migration-from-python-liquer)
- [Appendix B: Not implemented](#appendix-b-not-implemented)

---

## 1. Overview

`liquers-axum` provides four independent route builders, each generic over the `Environment` type
`E` and producing an `axum::Router<EnvRef<E>>`:

| Builder | Conventional base path | Serves |
|---|---|---|
| `StoreApiBuilder` | `/liquer/api/store` | the environment's `AsyncStore`, key by key |
| `AssetsApiBuilder` | `/liquer/api/assets` | the `AssetManager`: evaluation, observation, keyed mutations, WebSocket notifications |
| `RecipesApiBuilder` | `/liquer/api/recipes` | the `AsyncRecipeProvider`, read-only |
| `QueryApiBuilder` | `/liquer/q` | one-shot query evaluation |

Base paths are chosen by the caller; nothing is mounted by default. A server merges the routers it
wants and supplies the environment as state (§8).

---

## 2. Conventions

### 2.1 Addressing

Resources are addressed by path, never by query string: `GET /liquer/api/store/data/path/to/file.txt`.
Routes end in an axum 0.8 wildcard (`{*key}`, `{*query}`), so the address may contain `/`.

Two kinds of address are used, and each route family parses exactly one:

- a **key** (`notes/a.txt`) is parsed with `parse_key`. It names a stored or recipe-declared
  resource and carries no `-R/` prefix; a query-syntax path on a key route is a `ParseError` (400).
- a **query** (`make_text/upper`, `-R/notes/a.txt/-/upper`) is parsed with `parse_query`. A query
  may or may not be keyed.

### 2.2 The response envelope

Every status output — anything that is not a value transfer — is an `ApiResponse`:

```json
{
  "status": "OK",
  "result": { },
  "message": "Asset info",
  "query": "-R/notes/a.txt",
  "key": "notes/a.txt"
}
```

| Field | Type | Present |
|---|---|---|
| `status` | `"OK"` or `"ERROR"` | always |
| `result` | route-specific | on success |
| `message` | string | always |
| `query`, `key` | string | when the route addresses one (the Assets API always sets them; the Store API sets them on errors) |
| `error` | `ErrorDetail` (§3.2) | on error |

**Value transfers are not enveloped:** `data` routes answer the value's bytes; `entry` and
`recover` routes answer a `DataEntry` in the negotiated format.

### 2.3 Value transfers

A `data` route answers the value's serialized bytes with:

- `Content-Type`: the value's effective media type (`Metadata::get_media_type`), or
  `application/octet-stream` when there is none;
- `X-Liquers-Status`: the asset's status (`Ready`, `Source`, …).

An `entry` route answers a `DataEntry`:

```rust
pub struct DataEntry {
    pub metadata: serde_json::Value, // the MetadataRecord as JSON (a legacy document as it is)
    pub data: Vec<u8>,               // base64 in JSON, raw bytes in CBOR and bincode
}
```

The format is chosen by `?format=cbor|bincode|json` first, then the `Accept` header
(`application/cbor`, `application/x-bincode`, `application/json`; no quality values), then CBOR.
The response `Content-Type` names the format. A request carrying a `DataEntry` is decoded by
`?format=` first, then its `Content-Type`, then CBOR.

---

## 3. Errors

### 3.1 Status codes

An error's HTTP status follows its `liquers_core::error::ErrorType`
(`liquers-axum/src/api_core/error.rs`, `error_to_status_code`, matched exhaustively):

| HTTP | ErrorType |
|---|---|
| 400 | `ParseError`, `ParameterError`, `ArgumentMissing`, `TooManyParameters`, `UnknownCommand`, `ActionNotRegistered`, `KeyNotAbsolute` |
| 404 | `KeyNotFound`, `KeyNotSupported`, `NotAvailable` |
| 409 | `StatusConflict`, `CommandAlreadyRegistered`, `DependencyVersionMismatch`, `DependencyCycle` |
| 422 | `ConversionError`, `SerializationError` |
| 499 | `Cancelled` (client closed request; 503 where 499 cannot be represented) |
| 500 | `General`, `ExecutionError`, `UnexpectedError`, `KeyReadError`, `KeyWriteError` |
| 501 | `NotSupported`, `CacheNotSupported` |

- `KeyNotAbsolute` (400) and `KeyNotSupported` (404) are deliberately distinct: a key containing
  `.` or `..` is a malformed address, a key no store serves is an unrouted one.
- `StatusConflict` (409) means the operation exists but not for the asset's current status:
  removing a directory, expiring a `Source`, describing a computed value.
- The status is derived from the `error.type` string (`parse_error_type`); a type string that does
  not name an `ErrorType` gives 500. When a variant is added to `ErrorType`, both functions in
  `api_core/error.rs` gain an arm; the test `keyabs15b` round-trips every variant.

Routes that are not registered, or methods a registered path does not serve, get axum's own
**404** and **405** with an empty body.

### 3.2 Error body

```json
{
  "status": "ERROR",
  "message": "Failed to get asset",
  "key": "notes/a.txt",
  "error": {
    "type": "KeyNotFound",
    "message": "Key not found: 'notes/a.txt'",
    "key": "notes/a.txt"
  }
}
```

| Field | Type | Required |
|---|---|---|
| `error.type` | the `ErrorType` name | yes |
| `error.message` | string | yes |
| `error.query`, `error.key` | string | when the error names one |
| `error.traceback` | array of strings | only the Store API's `upload` fills it |
| `error.metadata` | object | never filled today |

The top-level `message` says what the route was doing; `error.message` says what went wrong.

---

## 4. Store API

`StoreApiBuilder::new(base).build()`. The store is `Environment::get_async_store()`; keys are
parsed with `parse_key`.

| Method and path | Result |
|---|---|
| `GET data/{*key}` | the bytes (§2.3); metadata from the store, default metadata if it has none |
| `PUT data/{*key}` | the key as a string; the body is stored with the key's existing metadata (default if none) |
| `DELETE data/{*key}` | the key as a string |
| `GET metadata/{*key}` | the `MetadataRecord` as JSON; legacy metadata as the stored document |
| `PUT metadata/{*key}` | the key as a string; body: a metadata JSON document (`Metadata::from_json_value`; a document that does not parse is a 422 `SerializationError`) |
| `GET entry/{*key}` | a negotiated `DataEntry` (§2.3; legacy metadata as the stored document) |
| `PUT entry/{*key}` | the key as a string; body: a `DataEntry` (422 `SerializationError` when it or its metadata does not decode) |
| `DELETE entry/{*key}` | as `DELETE data` |
| `GET listdir/{*key}` | array of the keys directly in the directory |
| `GET is_dir/{*key}` | `true` or `false` |
| `GET contains/{*key}` | `true` or `false` |
| `GET keys[?prefix=…]` | array of the keys directly in the prefix directory (root by default) — the same as `listdir`, not every key under it (`AXUM-STORE-KEYS-LISTS-ONLY-DIRECT-CHILDREN`) |
| `PUT makedir/{*key}` | the key as a string |
| `DELETE removedir/{*key}` | the key as a string; removes the directory and everything in it (`AsyncStore::removedir`) |
| `POST upload/{*key}` | `{uploaded: [keys], errors?: [messages]}`; `multipart/form-data`, each file part stored at `{key}/{filename}` with metadata naming its `filename`, plus a declared `media_type` only when the part's `Content-Type` differs from the type the filename implies and is not `application/octet-stream` (the client's type is then served back by the binary routes); if every part fails, 500 `KeyWriteError` with the messages in `error.traceback` |

All successes are 200. Errors are those of the store (§3.1).

**GET alternatives.** `StoreApiBuilder::with_destructive_gets()` (default off) adds
`GET remove/{*key}` (as `DELETE data`), `GET removedir/{*key}` and `GET makedir/{*key}`. Without
it, `GET removedir` and `GET makedir` answer 405 (the path serves `DELETE`/`PUT`) and
`GET remove` answers 404.

---

## 5. Assets API

`AssetsApiBuilder::new(base).build()`. The Assets API is the web face of
`Environment::get_asset_manager()`. Paths below are relative to the base.

### 5.1 Route families

| Family | Prefix | Parser | Addresses |
|---|---|---|---|
| query | `q/` | `parse_query` | any query, keyed or not: `q/data/make_text/upper`, `q/info/-R/notes/a.txt` |
| key | `key/` | `parse_key` | a keyed asset by its bare key: `key/data/notes/a.txt` |
| admin | `admin/` | `parse_key` where a key is taken | dependency audit and command-version refresh |
| WebSocket | `ws/q`, `ws/key` | as the families | notifications (§5.8) |

A key route given a query-syntax path (`key/data/-R/notes/a.txt`) answers 400 `ParseError`, whose
message points to `/q/`.

### 5.2 Access modes

Each family offers three modes, and **only the first two can start an evaluation**:

| Mode | Routes | Starts evaluation? | Answers |
|---|---|---|---|
| (a) request and wait | `data`, `entry` | yes, if the value is not available | the value (§2.3), or the evaluation's error |
| (b) submit | `submit` (`POST` and `GET`) | yes | **at once**: the asset's `AssetInfo` (`Submitted`, `Processing`, …, or `Ready` if cached), or the error of a submission that failed immediately |
| (c) observe | `info`, `metadata`, `version`; on `key/` also `contains`, `recover`, `listdir` | **never** | the current state |

- **Polling.** `submit` then `info` is the reliable way to follow an evaluation, with or without
  the WebSocket (notifications can be coalesced, §5.8). On `q/`, `info` of a query nobody has
  requested — or whose asset has been evicted — is 404 `NotAvailable` ("submit it first"), so
  `status: Ready` from `q/info` means `q/data` answers at once.
- **A pure-key query** (`q/info/-R/notes/a.txt`) on an observe route is answered like the key
  family: from the live asset, else the store, else the recipe.
- **Failing fast.** `submit` answers an error only for what fails synchronously: a parse error
  (400), a plan or volatility error, and a key that is neither stored nor declared by a recipe
  (404; `submit` checks `AssetManager::contains` first). Anything that fails during evaluation — an
  unknown command, a missing input — is reported as `status: Error` by `info`.
- **Inline managers.** With `ImmediateAssetManager` (`EvalMode::Inline`), `get_asset`/`get`
  evaluate before returning, so `submit` answers with the final status, and an evaluation error
  is returned by `submit` itself. The native default (`SimpleEnvironment`) queues and returns at
  once.
- **Assets that are never cached.** A volatile query or key, and a key whose recipe says
  `cached: false`, get a fresh asset per request that is not kept in the manager's maps. `submit`
  still answers with its `AssetInfo`, but the query-family observe routes and `q/cancel` answer 404
  for it, `key/info` reports the stored or recipe state, and a later `data` evaluates again. Follow
  such an asset on the WebSocket (the subscription holds it) or read it with `data`.
- **Inline expiry.** `ImmediateAssetManager` has no expiration monitor; it expires lazily inside
  `get_asset`/`get`. `q/info` does not apply that check, so on the inline manager it can report
  `Ready` for an entry whose expiration time has passed, which `q/data` then recomputes.

### 5.3 Query family routes

| Method and path | Mode | Result | Errors |
|---|---|---|---|
| `GET q/data/{*query}` | a | the bytes (§2.3) | 400 parse; the evaluation's error |
| `GET q/entry/{*query}` | a | a negotiated `DataEntry` | as `data` |
| `POST q/submit/{*query}`, `GET q/submit/{*query}` | b | `AssetInfo` | 400 parse or plan; 404 for an unknown key |
| `GET q/info/{*query}` | c | `AssetInfo` (with progress while `Processing`) | 400 parse; 404 `NotAvailable` |
| `GET q/metadata/{*query}` | c | the metadata record | as `info` |
| `GET q/version/{*query}` | c | `{"version": "<32 hex digits>"}`, all zeros until the asset has finished | as `info` |
| `POST q/cancel/{*query}` | — | `AssetInfo` after the cancel | 404 `NotAvailable` when the query is not cached |

`q/cancel` looks the asset up without creating one, so it never starts the evaluation it cancels.
Cancellation is best-effort and always answers 200: a queued asset ends `Cancelled`; a running
one is asked to stop and ends `Cancelled` if its command is suspended or checks the request; a
command that has already completed (or that cannot be interrupted and completes) leaves the asset
`Ready` and stored. Clients read the outcome from the returned or a later `info`. A `Cancelled`
asset's `info.error_data` is the cancellation error (`error_type` `Cancelled`, `is_error` false),
whose `query` names the asset whose cancel was requested — for a dependent cancelled by cascade,
that is the dependency. Subscribers see one terminal status.

### 5.4 Key family routes

| Method and path | Mode | Result | Errors |
|---|---|---|---|
| `GET key/data/{*key}` | a | the bytes (§2.3), via `AssetManager::get` | 400 parse (with a `/q/` hint for `-R/…`); 404; the evaluation's error |
| `GET key/entry/{*key}` | a | a negotiated `DataEntry` | as `data` |
| `POST key/submit/{*key}`, `GET key/submit/{*key}` | b | `AssetInfo` | 400; 404 when neither stored nor declared by a recipe |
| `GET key/info/{*key}` | c | `AssetInfo` of the live asset, else the stored entry, else the recipe; a cached `Expired`/`Error`/`Cancelled` entry is reported as it is, not re-evaluated | 404 `KeyNotFound` |
| `GET key/metadata/{*key}` | c | the live, stored or recipe metadata record | 404 |
| `GET key/version/{*key}` | c | `{"version": "…"}`; all zeros (`Version::unknown()`) when the key has no version | a store read error stays an error (500) |
| `GET key/contains/{*key}` | c | `{"contains": bool}` — stored, or listed by the recipe provider | — |
| `GET key/can_make/{*key}` | c | `{"can_make": bool}` — stored, or producible by the recipe provider (a template-generated key is producible but not listed) | — |
| `GET key/recover/{*key}` | c | a negotiated `DataEntry` of the last known value, whatever its status (an `Expired` one included) | 404 when there is no data-bearing state |
| `GET key/listdir`, `GET key/listdir/{*key}` | c | `{"assets": [AssetInfo…]}`, directories first; with `?deep=true`, `{"keys": ["a/b.md", …]}` | the store's error |
| `POST key/data/{*key}` | — | **201**, `AssetInfo` after the write | 400 unknown type; 422 a format the type cannot be written in |
| `POST key/entry/{*key}` | — | **201**, as `POST data` | 400 undecodable body or non-object metadata |
| `POST key/metadata/{*key}` | — | always **501** `NotSupported` | — |
| `DELETE key/data/{*key}`, `DELETE key/entry/{*key}` | — | `{"removed": true, "new_status": "Recipe" \| "None"}` | 404; 409 for a directory |
| `DELETE key/removedir/{*key}` | — | `{"removed": true}` | 404 absent; 409 not a directory; the first failing key's error |
| `PUT key/makedir/{*key}` | — | **201**, the directory's `AssetInfo` (`status: Directory`) | the store's error |
| `POST key/description/{*key}` | — | `AssetInfo` after the change | 400 neither field given; 404; 409 not a `Source` |
| `POST key/expire/{*key}` | — | `AssetInfo` after the change | 404; 409 a status that cannot expire |
| `POST key/override/{*key}` | — | `AssetInfo` after the change | 404 no data to pin |
| `POST key/cancel/{*key}` | — | `AssetInfo` after the cancel | 404 `NotAvailable` when no live asset holds the key |

- `key/listdir?deep=true` contains the shallow listing of every store directory under the key,
  recipe-declared keys included, and never the key itself.
- `key/submit` and the WebSocket `subscribe` accept any key `can_make` reports.
- Store API writes and removals refresh the recipe provider's directory caches, as mediated writes do.
- `key/cancel`, like `q/cancel`, never creates or starts an asset.

#### Writing a value (`POST key/data`, `POST key/entry`)

The write goes through `AssetManager::set_binary`. The status is not the client's: a key with a
recipe becomes `Override`, one without becomes `Source`. The version is the hash of the bytes, and
dependents of the key are expired.

Only five metadata fields are client-settable — `type_identifier`, `data_format`, `media_type`,
`title`, `description` (`ValueDescription`). `POST key/data` takes them as query parameters, with
the body as the raw value; `POST key/entry` takes them from the `DataEntry`'s `metadata` object.
Every other parameter or field is ignored and named in the response `message` ("ignored metadata
fields: status, version"); the record handed to the manager is built fresh from the five fields.

- `type_identifier` defaults to `Bytes`; an identifier the type registry does not know is 400
  `ParameterError`. `type_name` comes from the registry.
- A field the client leaves out is taken from the key's current value: `title` and `description`
  when non-empty; `type_identifier` and `data_format` only as a pair, only when the client gives
  neither, only from a value that holds data, and never a format that type cannot be written in.
  `media_type` is never inherited (it would turn a derived type into an override).
- The request `Content-Type` of `POST key/data` is not used as the media type.

`POST key/metadata` is refused: an asset's metadata is owned by the asset manager. A `Source`'s
title and description are set with `POST key/description`.

#### Removal (`DELETE key/data|entry`)

`AssetManager::remove` decides by the asset's status (the live asset's, or the stored one's when
nothing live has produced anything yet):

| Status | Recipe? | Effect | `new_status` |
|---|---|---|---|
| `Directory` | any | 409 `StatusConflict`, naming `key/removedir` | — |
| any other | no | deleted: live asset, dependency-graph entry and stored entry; dependents expired | `None` |
| `Source`, `Override` | yes | the user value is deleted, as above; the key falls back to its recipe | `Recipe` |
| computed (`Ready`, `Expired`, `Error`, `Cancelled`, `Volatile`, `Partial`, in flight) | yes | the value is dropped; the stored record is kept as `Recipe` with its version; dependents are **not** expired | `Recipe` |
| none, or stored `Recipe` | yes | nothing to do | `Recipe` |
| nothing live, nothing stored | no | 404 `KeyNotFound` | — |

`key/removedir` removes every key under the directory with the same rules (deepest first), then
the directory with everything left in it — including its `recipes.yaml` and the `Recipe` records
kept above. It is not atomic.

#### Other mutations

- `key/expire`: `Ready` and `Override` become `Expired` and dependents are expired; `Expired` is
  left as it is; a `Source` (no recipe to recover from), a recipe key with no value and an
  in-flight asset are 409.
- `key/override`: pins the current value as `Override` (whatever its status, `Expired` included),
  so it is no longer recomputed; a `Source` is left unchanged. 404 when there is no data.
- `key/description`: body `{"title": …, "description": …}` (JSON; either field may be omitted), or
  the query parameters of the GET alternative; body fields win. Data and version are unchanged.

### 5.5 Admin routes

| Method and path | Result |
|---|---|
| `POST admin/audit/{*key}` | `{"checked": [...], "expired": [...]}`: the recorded dependency versions reachable from the key, verified; what no longer holds is expired |
| `POST admin/audit` | as above, over every gap the dependency graph knows of |
| `POST admin/refresh_command_versions` | `result: null`; re-registers command versions and expires what changed |

### 5.6 GET alternatives and builder options

| Builder option | Default | Effect |
|---|---|---|
| `with_destructive_gets()` | off | adds a GET form of each operation that needs no body: `GET key/remove`, `key/removedir`, `key/makedir`, `key/description?title=&description=`, `key/expire`, `key/override`, `q/cancel`, `key/cancel`, `admin/audit[/{*key}]`, `admin/refresh_command_versions` |
| `read_only()` | off | omits every mutation route — `POST key/data|entry`, `DELETE key/data|entry`, `DELETE key/removedir`, `PUT key/makedir`, `POST key/description|expire|override` — their GET forms, and the admin routes. `q/cancel` and `key/cancel` stay |
| `with_admin(bool)` | `true` | includes or omits the `admin/` routes |
| `with_websocket_path(p)` | `{base}/ws` | moves the WebSocket endpoints to `p/q` and `p/key` |
| `without_websocket()` | — | removes the WebSocket endpoints |
| `with_websocket_limits(WebSocketLimits)` | 64 KiB, 256 | §5.8 |

`GET q/submit` and `GET key/submit` always exist, as do the reads, the observe routes and
`POST key/metadata`. An omitted route answers axum's 405 where its path still serves another method
(`GET key/expire` without the flag; `POST key/data` under `read_only()`) and 404 otherwise
(`GET key/remove` without the flag; `POST key/expire` under `read_only()`).

### 5.7 Result types

`AssetInfo` is `liquers_core::metadata::AssetInfo` serialized: `query`, `key`, `status`,
`type_identifier`, `type_name`, `data_format`, `message`, `title`, `description`, `is_error`,
`media_type`, `filename`, `unicode_icon`, `file_size`, `is_dir`, `progress`, `updated`,
`error_data`, `is_volatile`, `payload_required`, `expires`, `expiration_time`, `stored`, `cached`.
`status` is one of `None`, `Directory`, `Recipe`, `Submitted`, `Dependencies`, `Processing`,
`Partial`, `Error`, `Storing`, `Ready`, `Expired`, `Source`, `Cancelled`, `Override`, `Volatile`.
A version is 32 hexadecimal digits; all zeros means unknown.

### 5.8 WebSocket

| Endpoint | Subscriptions | Requested with |
|---|---|---|
| `GET ws/q`, `GET ws/q/{*query}` | queries (`parse_query`) | `AssetManager::get_asset` (a pure-key query must be stored or recipe-declared) |
| `GET ws/key`, `GET ws/key/{*key}` | keys (`parse_key`) | `AssetManager::get` (the key must be stored or recipe-declared) |

A path in the URL is subscribed on connect. `read_only()` does not affect the endpoints:
subscribing changes no data. Subscribing *requests* the asset; there is no observe-only
subscription.

Each subscription requests its asset in its own task, so the connection keeps answering `ping`
and `unsubscribe` while a request is pending. With an inline manager (`EvalMode::Inline`) the
request evaluates before it returns, so `Initial` arrives only once the evaluation has finished and
no intermediate progress is seen.

**Client messages** (JSON text, snake_case):

```json
{"action": "subscribe", "query": "make_text/upper"}
{"action": "subscribe", "key": "notes/a.txt"}
{"action": "unsubscribe", "query": "make_text/upper"}
{"action": "unsubscribe_all"}
{"action": "ping"}
```

The address field must match the endpoint (`query` on `ws/q`, `key` on `ws/key`). A message that
does not parse, names the wrong field, addresses an unknown key, or exceeds the subscription limit
is answered with an `Error` message. Subscribing to an address already subscribed replaces the
subscription.

**Server messages** (JSON text, tagged by `type`). Every asset notification is flat and carries:

| Field | Meaning |
|---|---|
| `asset_id` | the followed asset |
| `query` | the query as subscribed (`ws/q`) |
| `key` | the key as subscribed (`ws/key`), or the asset's key if it is keyed |
| `timestamp` | RFC 3339 |
| `info` | the asset's `AssetInfo` **re-read after the change** (`null` after a delete) |

| `type` | Extra fields | Cause |
|---|---|---|
| `Initial` | — | sent once, on subscribe |
| `JobSubmitted`, `JobStarted`, `ValueProduced`, `JobFinished` | — | evaluation progress |
| `StatusChanged` | `status` | a status change (a cancel arrives as `StatusChanged` with `Cancelled`) |
| `ErrorOccurred` | `error` (`ErrorDetail`) | the evaluation failed |
| `LogMessage` | — | a log entry was added; its text is in `info.message` |
| `PrimaryProgressUpdated`, `SecondaryProgressUpdated` | `progress` (`message`, `done`, `total`, `timestamp`, `eta`) | progress |
| `Expired` | — | the asset expired |
| `Removed` | — | the asset was removed (`DELETE key/…`) or replaced (`POST key/data|entry`) |
| `Pong`, `UnsubscribedAll` | `timestamp` | replies |
| `Error` | `timestamp`, `error` (`ErrorDetail`) | a client message could not be handled |

The core notification channel keeps only the latest message, so notifications may be coalesced;
`info` always reflects the current state, and `q/info` / `key/info` remain the authoritative
polling path. Right after `Initial`, the most recent notification the asset had already sent (if
any) is replayed, so an asset that finished before the subscribe still reports how it finished.

**Lifecycle.** A subscription follows **one asset** and ends with it: after the notification whose
`info.status` is `Error`, `Cancelled`, `Expired` or `Volatile`, or after `Removed` (the
subscription also ends at once if its `Initial` snapshot is terminal). The terminal notification
is always delivered. To keep following the query or key, subscribe again, which requests a fresh
asset.

**Limits** (`WebSocketLimits`): `max_message_size` (default 64 KiB) — a larger client message
closes the connection; `max_subscriptions` (default 256) per connection. Intermediate notifications
that find the connection's outgoing queue full are dropped (the next one carries a fresh `info`);
replies and terminal notifications wait. Disconnecting ends every subscription of the connection
but does not cancel the evaluations, which other clients may share.

---

## 6. Recipes API

`RecipesApiBuilder::new(base).build()`: a read-only view of `Environment::get_recipe_provider()`.
Keys are parsed with `parse_key`.

| Method and path | Result |
|---|---|
| `GET listdir` | array of the resource names that have a recipe **in the root directory** (`AsyncRecipeProvider::assets_with_recipes`) |
| `GET data/{*key}` | the recipe as text (`text/plain`), not enveloped |
| `GET metadata/{*key}` | the recipe's metadata: `AsyncRecipeProvider::get_asset_info` as a `MetadataRecord` (title, description, filename, planning diagnostics) — what the Assets API's `key/metadata` returns for a recipe key |
| `GET entry/{*key}` | a `DataEntry` with the recipe text as `data` and the same metadata, negotiated like the Assets API's: `?format=` (`cbor`, `json`, `bincode`), then `Accept`, then CBOR; `data` is base64 in JSON |
| `GET resolve/{*key}` | `{"key": …, "query": …, "plan": {…}}`, the recipe's execution plan (`AsyncRecipeProvider::recipe_plan`); a recipe whose commands are not registered is 400 `ActionNotRegistered` |

A key with no recipe answers the recipe provider's error; its type, and so the HTTP status,
depends on the provider.

---

## 7. Query API

`QueryApiBuilder::new(base).build()` registers one route, `{base}{*query}`: the query is
whatever follows the base path (`QueryApiBuilder::new("/liquer/q")` serves
`GET /liquer/q/make_text/upper`).

| Method | Behaviour |
|---|---|
| `GET {base}{*query}` | evaluates the query (`Environment::evaluate`) and answers the value (§2.3) |
| `POST {base}{*query}` | the same; an optional JSON body is accepted and logged but not used; a non-JSON body is 400 |

The value is read with `AssetRef::get_binary`: the request waits for the evaluation, the value is
serialized in its effective format, and an `Error`, `Cancelled`, `Expired` or `Directory` result
answers that asset's own error. The whole wait — evaluation included, which matters for an
inline manager — is bounded by `QueryApiBuilder::with_timeout(Duration)`
(default 30 s); on timeout the answer is 500 `ExecutionError`, whose message names the duration and
points to the Assets API's `q/submit`, `q/info` and `ws/q` for long evaluations.

---

## 8. Assembling a server

There is no single "full" builder: merge the routers you want and give the result the
environment as state. The routers are generic over any `E: Environment`; `EnvRef<E>` is
`liquers_core::context::EnvRef`, re-exported by `liquers-axum`.

```rust
use liquers_axum::{AssetsApiBuilder, QueryApiBuilder, RecipesApiBuilder, StoreApiBuilder};
use liquers_core::context::{Environment, SimpleEnvironment};
use liquers_core::value::Value;

let mut env = SimpleEnvironment::<Value>::new();
// … store, recipe provider, commands …
let envref = env.to_ref();

let app = axum::Router::new()
    .merge(QueryApiBuilder::new("/liquer/q").build())
    .merge(StoreApiBuilder::new("/liquer/api/store").build())
    .merge(AssetsApiBuilder::new("/liquer/api/assets").build())
    .merge(RecipesApiBuilder::new("/liquer/api/recipes").build())
    .with_state(envref);

let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await?;
axum::serve(listener, app).await?;
```

Middleware (tracing, CORS, authentication) is added with axum's own `.layer(…)`. Runnable
servers: `liquers-axum/examples/basic_server.rs`, `assets_recipes_basic.rs`, `websocket_client.rs`.

---

## 9. Access control

There is none in `liquers-axum`: no session, user or permission reaches the handlers, and every
route acts with the environment's full rights. The design is `CORE-SESSION-AND-KEY-ACL`.

Until it exists, the Assets API's `read_only()` and `with_admin(false)` (§5.6) and the Store API's
default without `with_destructive_gets()` limit what a deployment exposes, and an embedding
application can put its own authentication in front of the routers as an axum layer. A store
mounted from a directory stays writable through the Store API's `PUT`/`DELETE` routes and the
Assets API's mutations unless those are left out (`STORE-NO-READ-ONLY-ADAPTER`).

---

## 10. Implementation notes

- **Handlers** take `State<EnvRef<E>>` and a `Path<String>` for the wildcard, plus `HeaderMap`,
  `Query<HashMap<String, String>>` or `Bytes` as needed, and return `axum::response::Response`.
  Errors are turned into responses by `ApiResponse::error(error_to_detail(&e), message)`; there is
  no `ApiError` type.
- **`IntoResponse`** is implemented in `axum_integration.rs` for `ApiResponse<T>`,
  `BinaryResponse` and `DataEntry`; responses are finished with `build_or_500`, so a response that
  cannot be built (an invalid header value) is a 500, never a panic. Library code has no
  `unwrap()`/`expect()` (`cargo clippy -p liquers-axum --no-deps -- -D clippy::unwrap_used -D
  clippy::expect_used`).
- **Routes** use axum 0.8 syntax: `{*key}` / `{*query}` for wildcards. The old `/*key` and `/:key`
  forms panic when the router is built.
- **Assets API modules:** `assets/query_handlers.rs` (`q/`), `assets/key_handlers.rs` (`key/`,
  `admin/`), `assets/common.rs` (shared helpers and result types), `assets/value_description.rs`,
  `assets/websocket.rs`, `assets/builder.rs`.
- **Query API settings** (`QueryApiConfig { timeout }`) reach its handlers as an axum `Extension`
  added by the builder; `WebSocketLimits` reach the WebSocket handlers the same way.

---

## Appendix A: Migration from Python liquer

| Python liquer | liquers-axum | Notes |
|---|---|---|
| `/liquer/q/QUERY` | `{query base}{*query}` | same shape with the conventional base `/liquer/q` |
| `/liquer/api/store/data/KEY` | `/liquer/api/store/data/{*key}` | writes are `PUT`, not `POST` |
| `/liquer/api/store/remove/KEY` etc. | `GET remove|removedir|makedir/{*key}` | only with `with_destructive_gets()` |
| `/liquer/api/cache/get/KEY` | `/liquer/api/assets/key/data/{*key}` or `/liquer/api/assets/q/data/{*query}` | the cache is the asset manager |
| `/liquer/submit/QUERY` | `POST|GET /liquer/api/assets/q/submit/{*query}`, then `q/info` | |
| status in the JSON body | HTTP status codes plus the §3 body | |

## Appendix B: Not implemented

Not available at HEAD; listed so that no reader mistakes them for features:

- access control and sessions (§9; `CORE-SESSION-AND-KEY-ACL`);
- a remote store or asset manager speaking this API (`NO-REMOTE-STORE-OR-ASSET-MANAGER`);
- recipe writes (the Recipes API is read-only);
- WebSocket connection-count limits, idle timeouts and server-initiated keep-alive
  (`AXUM-WEBSOCKET-HARDENING`);
- manager-wide or scope subscriptions (`ASSET-EXPIRATION-EVENTS-CANNOT-BE-OBSERVED-EXCEPT-PER-ASSET`);
- streaming of large values (`VALUE-SERIALIZATION-IS-SYNCHRONOUS-AND-WHOLE-VALUE`);
- batch operations, server-sent events, an OpenAPI document, built-in CORS or rate limiting.

## History

| Date | Change | Source |
|---|---|---|
| 2026-10-10 | Store API: `metadata` and `entry` serve legacy metadata as stored (not `{}`); `upload` records each part's `filename` and declares its `Content-Type` only when the filename does not imply it. `AXUM-STORE-UPLOAD-AND-METADATA-DROP-INFORMATION` closed. | `design/axum-store-upload-metadata/` |
| 2026-10-09 | §5.3 `q/cancel`: the best-effort outcomes (a completed command stays `Ready`), the cause in `error_data`, cascade attribution; `ASSET-CANCEL-DURING-PROCESSING-FINISHES-READY` closed. | phase-5 (`design/asset-cancellation-outcome/`) |
| 2026-10-07 | §6: Recipes API `metadata` and `entry` return the recipe's asset info as metadata, and `entry` is negotiated by `?format=` / `Accept`. | phase-5, `design/axum-recipes-metadata-entry/` |
| 2026-10-06 | Added `key/can_make`; `key/contains` is stored-or-listed; deep listing is complete; Store API writes notify the provider. | phase-5 |
| 2026-09-29 | Review fixes of PR #73: §5.8 — each subscription requests its asset in its own task, so the connection stays responsive; with an inline manager `Initial` arrives after the evaluation. §7 — the timeout bounds the whole wait, evaluation included. Linked the new `WEB_API_GUIDE.md`. | `design/axum-assets-endpoints/` |
| 2026-09-28 | Rewritten against the implementation (full audit): the Store API's writes are `PUT` and its results are as served; §3 lists the real `ErrorType` → HTTP mapping, with `StatusConflict` → 409; §5 is the new Assets API — `q/`, `key/` and `admin/` families, access modes, status-aware removal, `removedir`, `expire`, `override`, `description`, the metadata allow-list, GET alternatives and builder switches, and the `ws/q` / `ws/key` WebSocket protocol with its lifecycle and limits; §7 documents `with_timeout` and the `get_binary` read; the nonexistent `FullApiBuilder`, `liquers_web` crate, `Router` trait, `SessionInterface` and `ApiError` are replaced by the `Router::merge` assembly and the real handler pattern; the old version/status header and "Revision History" table are folded into this table. | `design/axum-assets-endpoints/` |
| 2026-08-17 | Added `KeyNotAbsolute` (400) and `KeyNotSupported` (404) to the error-type table: a key containing `.` or `..` is now refused by every store, and the two refusals are deliberately distinct — malformed address versus unrouted key. | `design/store-key-guard/` |
| 2026-03-02 | Present at repository import (version 1.0.0 draft of 2026-01-19); content unchanged since. Not reviewed against the implementation. | migration |
