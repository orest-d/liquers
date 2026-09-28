# Phase 2: Solution & Architecture - Assets API over the whole AssetManager

## Overview

> **Revised 2026-09-28 (Phase 4 gate).** The first version addressed every route with a *query*
> and treated "key-only" operations as queries that happen to be pure keys. That hid a whole
> class of assets. The user's review found that **non-keyed assets had no feedback route**: no
> info, status, version or progress for a query such as `-R/notes/a.txt/-/summarize`. This
> revision splits the API into **two route families** that mirror the two `AssetManager` entry
> points, `get_asset(query)` and `get(key)`. See "Addressing", "Learning Points" and "Open
> Questions". Phases 3 and 4 were written against the old routing and are **stale** until this
> document is approved again.

Two layers change.

**`liquers-core`:**
- `AssetManager::remove` becomes a status-aware default method: it deletes a user-supplied value
  and cascades, or drops a recomputed value and keeps its version.
- Two new default methods: `expire(key)` and `set_description(key, …)`.
- `to_override` on a stored `Source` becomes a no-op, matching the in-memory path.
- Refusals that depend on an asset's status get a new `ErrorType::StatusConflict` (HTTP 409).

**`liquers-axum`:** the Assets API is served as two families under the base path:
- **`/q/`**: query-addressed. It goes through `get_asset(query)`, so it covers non-keyed assets and
  `-R/<key>` queries. Reads, info/status/version feedback, cancel and notifications.
- **`/key/`**: key-addressed. Bare keys, parsed with `parse_key`, go to the keyed `AssetManager`
  methods: reads without evaluation, listing, and every mutation.

Two manager-wide admin routes sit at the root. `AssetsApiBuilder` gains `read_only()` and
`with_admin(bool)`, and the crate gets its first handler tests. There are no new commands and no
new value types.

## Addressing: two route families

A **key** (`notes/a.txt`, grammar `parse_key`) names a stored or recipe-declared resource. A
**query** (`-R/notes/a.txt/-/upper`, `make_text`, grammar `parse_query`) names a computation.

The two grammars overlap in text and differ in meaning:
- `parse_query("notes/a.txt")` is the *action* `notes` with filename `a.txt`.
- `parse_key("-R/notes/a.txt")` fails, because a key segment cannot start with `-`
  (`parse.rs` `resource_name`).

So the family must be visible in the URL, and each family has exactly one parser:

| Family | Prefix | Parser | Core entry point | Evaluates? |
|---|---|---|---|---|
| query | `{base}/q/<op>/{*query}` | `parse_query` | `get_asset(query)` to request; `lookup_query_asset(query)` to observe | only the *request* modes (see "Access modes") |
| key | `{base}/key/<op>/{*key}` | `parse_key` | `get(key)` to request; the keyed describe/read methods to observe | only the *request* modes |
| admin | `{base}/<op>` | — | manager-wide methods | no |

- A pure-key query on the query family (`q/data/-R/notes/a.txt`) is still valid: `get_asset`
  delegates to `get(key)`, so it reaches the same keyed asset. The key family is the
  bare-key, no-evaluation view of the same assets.
- Every **mutation** is key-addressed: `set_binary`, `remove`, `expire`, `set_description`,
  `to_override` and `makedir` all take a `Key`. The one query-addressed control is `POST
  q/cancel`, with notifications on the WebSocket.
- Non-keyed assets are never stored and never listed. Their lifecycle is request, observe and
  cancel, and that is exactly what `/q/` offers.

## Access Modes

Decided 2026-09-28 (Q23). Each family offers the same three modes, and **only the first two
may start an evaluation**:

| Mode | Routes (both families) | Core | Starts evaluation? | Returns |
|---|---|---|---|---|
| **a) request and wait for the result** | `data`, `entry` | `get_asset` / `get`, then `AssetRef::get_binary()` (I1) | yes, if not already available | the value (bytes, or a negotiated `DataEntry`) or the evaluation's error |
| **b) submit** | `submit` (`POST`, and `GET`, which is always on because it changes no data) | `get_asset` / `get`, not awaited | yes | **immediately**: `AssetInfo` (e.g. `Submitted`, `Processing`, or `Ready` if cached), or the error of a failed submission (parse 400, unknown command or plan error 400, key not found 404) |
| **c) observe (polling)** | `info`, `metadata`, `version` (plus `contains`, `recover`, `listdir` on `/key/`) | `lookup_query_asset` for `/q/`; for `/key/`, the live asset, then the store, then the recipe provider | **never** | current state. Non-keyed assets are cached, so `q/info` answers "has this query been evaluated, and is it available now?": `status: Ready` means `q/data` returns at once. A query that is not cached (never requested, or evicted) gives 404 (`NotAvailable`, "submit it first"). This is the reliable polling path after `submit`, with or without the WebSocket, since notifications can be coalesced or missed (W6). |

Verified against the code (2026-09-28):
- **`key/info` never evaluates**, once `get_asset_info` stops calling `get`. All three of its
  branches only read: `AssetRef::get_asset_info`, `AsyncStore::get_asset_info`, and the recipe
  provider's `get_asset_info`. The last one builds the recipe's plan through
  `create_plan_with_init_metadata`, whose `has_volatile_dependencies` and
  `has_expirable_dependencies` walk recipe keys with `recipe_opt` and never evaluate. Polling a
  recipe key therefore costs plan building and recipe reads, not command execution.
- **`q/info`, as first designed, did evaluate:** it went through `get_asset`, which creates and
  submits the asset. It now uses a non-creating lookup (O2, now required). A pure-key query
  (`q/info/-R/a.txt`) is answered like `key/info`.
- **`metadata` moves to the observe mode** on both families. Today's `GET metadata` calls
  `get_asset` and so triggers evaluation.
- **Inline managers:** on `ImmediateAssetManager` (`EvalMode::Inline`), `get`/`get_asset` evaluate
  before returning, so `submit` answers with the *final* status. The native default
  (`SimpleEnvironment`, the `Queued` kind) returns at once. The spec documents both.

### Removal of directories (Q24)

`DELETE key/data` on a `Directory` does **not** perform a directory removal: it answers 409
(`StatusConflict`) and names `key/removedir`. Directory removal is its own route, as in the Store
API:

```rust
/// NEW, default method. Remove a directory and everything in it: `remove` (with its
/// status-aware semantics — delete+cascade for Source/Override, drop-and-keep-version for
/// recipe-computed values) for every stored key under it (`listdir_keys_deep`), deepest first,
/// then `store.removedir(key)`. Recipe-declared keys survive (their recipes are not stored data).
/// Holds `key_mutation_lock` per key, not for the whole walk.
async fn removedir(&self, key: &Key) -> Result<(), Error>;
```

It is not atomic: a failure part-way leaves the keys already removed removed, as
`AsyncStore::removedir` is documented to behave (`STORE_SEMANTICS.md` §5). The error names the
first key that failed.

### GET alternatives for every operation (Q25)

The Store API offers opt-in GET routes for its destructive operations
(`StoreApiBuilder::with_destructive_gets()`, default off). The Assets API follows the same
approach, and applies it to **every** operation that does not need a request body:

| Operation | Primary | GET alternative (with `with_destructive_gets()`) |
|---|---|---|
| remove | `DELETE key/data`, `DELETE key/entry` | `GET key/remove/{*key}` |
| removedir | `DELETE key/removedir/{*key}` | `GET key/removedir/{*key}` |
| makedir | `PUT key/makedir/{*key}` | `GET key/makedir/{*key}` |
| expire | `POST key/expire/{*key}` | `GET key/expire/{*key}` |
| override | `POST key/override/{*key}` | `GET key/override/{*key}` |
| description | `POST key/description/{*key}` (JSON) | `GET key/description/{*key}?title=…&description=…` |
| cancel | `POST q/cancel`, `POST key/cancel` | `GET q/cancel/{*query}`, `GET key/cancel/{*key}` |
| audit | `POST admin/audit/{*key}`, `POST admin/audit` | `GET admin/audit/{*key}`, `GET admin/audit` |
| refresh | `POST admin/refresh_command_versions` | `GET admin/refresh_command_versions` |
| submit | `POST q/submit`, `POST key/submit` | `GET q/submit`, `GET key/submit`, **always on** (no data changes) |

- **Excluded:** `POST data` and `POST entry`, because they carry a request body, and
  `POST metadata`, which always refuses.
- `read_only()` removes the GET alternatives of everything it disables, and `with_admin(false)`
  removes them for the admin operations.
- This brings `GET remove` into scope, so the row in `ASSETS-API-ADMIN-OPERATIONS` is removed.

## Known-Issue Preflight

| Issue | Status | Priority | Relevance | Blocking? | Action |
|---|---|---|---|---|---|
| `AXUM-ASSETS-API-ENDPOINTS-NOT-IMPLEMENTED` | draft | P0 | the defect this design fixes | — | close on merge |
| `ASSET-REMOVE-FORGETS-DEPENDENTS` | draft | P2 | fixed by the new `remove` | no | close on merge |
| `AXUM-HANDLER-TEST-COVERAGE` | accepted | P2 | **in scope (I4):** route tests for all four builders | no | close on merge |
| `TEXT-VALUE-CANNOT-BE-STORED-AS-MARKDOWN` | draft (filed now) | P2 | `POST data` defaults to `Bytes` because of it | no | monitor; the default can become `Text` once it is fixed |
| `CORE-SESSION-AND-KEY-ACL` | accepted | P2 | real access control; the builder switches are a stop-gap until it lands | no | monitor |
| `STORE-NO-READ-ONLY-ADAPTER` | draft | P2 | a corpus mounted from a file store is writable through `POST`/`DELETE` unless the router is `read_only()` | no | document `read_only()` as the mitigation |
| `QUEUED-MANAGER-EVICTION-RACE` | accepted | P2 | `remove` unmaps the live asset under the same lock as today | no | unchanged |
| `WEB-API-SPECIFICATION-DIVERGES-FROM-IMPLEMENTATION` | draft | P1 | **in scope (Q21):** the whole of `WEB_API_SPECIFICATION.md` is made true at HEAD. That covers the store entry write (`PUT`), the nonexistent `FullApiBuilder` and crate names, the WebSocket path, and every other drift the audit finds. | no | close on merge |
| `ASSETS-API-ADMIN-OPERATIONS` | draft | P3 | deferred endpoints | no | none |
| `AXUM-ASSETS-API-SERVES-ONLY-BYTES-AND-TEXT` | draft | P2 | **in scope (I1)** | no | close on merge |
| `EXPIRATION-RECOVERY-WEB-API` | accepted | P2 | **in scope (I2);** delivered by `key/recover` and `key/override` | no | close on merge |
| `AXUM-QUERY-TIMEOUT-HARDCODED` | draft | P2 | **in scope (I5)** | no | close on merge |
| `AXUM-WEBSOCKET-HARDENING` | accepted | P3 | **partly in scope (I6)** | no | progress note; stays `accepted` |
| `LIBRARY-CODE-USES-UNWRAP-AND-EXPECT` | draft | P2 | **the `liquers-axum` part in scope (I7)** | no | progress note; stays open for other crates |
| `MEDIA-TYPES-MISSING-FOR-TABULAR-FORMATS` | draft | P3 | **in scope (I9)** | no | close on merge |
| out of scope: `ERROR-WITH-KEY-SETS-QUERY-FIELD`, `CORE-SESSION-AND-KEY-ACL`, `NO-REMOTE-STORE-OR-ASSET-MANAGER`, `VALUE-SERIALIZATION-IS-SYNCHRONOUS-AND-WHOLE-VALUE`, `TYPE-REGISTRY-NOT-REALM-AWARE`, `WORKSPACE-SERDE-DERIVE-UNDECLARED`, `HTTP-STORE-METADATA-DROPS-THE-EXTENSION-MEDIA-TYPE` | — | — | reviewed 2026-09-28: cross-crate core redesigns, or not the server's web API | no | none |
| `DESCRIBING-AN-ASSET-CAN-TRIGGER-ITS-EVALUATION` | draft | P1 | the `get_asset_info` change fixes exactly this (both bodies) | no | close on merge, or hand to `store-and-asset-search` if it lands first (final review) |
| `ASSET-TO-OVERRIDE-SOURCE-INCONSISTENT` | draft | P3 | decided: `to_override` on a `Source` does nothing in both paths | no | fixed here; close on merge |
| `AXUM-ASSETS-CANCEL-STARTS-EVALUATION` | draft | P3 | **in scope (I3):** `POST cancel` via `get_asset` starts the evaluation it cancels | no | `key/cancel` fixes it for keys; for queries it needs O2; close on merge if O2 is accepted |
| `AXUM-ASSETS-WEBSOCKET-ROUTE-PANICS` | draft (filed in final review); **in scope (Q21)** | P1 | `build()` panics with the default WebSocket path | **yes, for every router test** | fixed in `builder.rs`; close on merge |

None is blocking.

## Data Structures

### New Structs

#### `liquers-axum/src/assets/value_description.rs` — `ValueDescription`

```rust
/// The client-settable part of a value's metadata (Phase 1, "Metadata ownership").
/// Everything else in `MetadataRecord` is owned by the asset manager.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ValueDescription {
    pub type_identifier: Option<String>,
    pub data_format: Option<String>,
    pub media_type: Option<String>,
    pub title: Option<String>,
    pub description: Option<String>,
}

pub const VALUE_DESCRIPTION_FIELDS: [&str; 5] =
    ["type_identifier", "data_format", "media_type", "title", "description"];
```

- Owned `String`s: parsed from a request, then moved into a `MetadataRecord`.
- It is an allow-list by construction. The record handed to `set_binary` is built **fresh** by
  `into_metadata_record`, never cleaned from a client document, so a field added to
  `MetadataRecord` later is not client-settable by accident.

#### `liquers-axum/src/assets/handlers.rs` — response DTOs

```rust
#[derive(Serialize)] pub struct AssetListing { pub assets: Vec<AssetInfo> }   // GET listdir
#[derive(Serialize)] pub struct KeyListing  { pub keys: Vec<String> }        // GET listdir?deep=true
#[derive(Serialize)] pub struct RemoveResult { pub removed: bool, pub new_status: Status }
#[derive(Serialize)] pub struct ContainsResult { pub contains: bool }
#[derive(Serialize)] pub struct VersionResult { pub version: Version } // 32 hex digits; all zeros = unknown (O3)
#[derive(Serialize)] pub struct AuditResult { pub checked: Vec<String>, pub expired: Vec<String> }
#[derive(Deserialize)] pub struct DescriptionRequest { pub title: Option<String>, pub description: Option<String> }
```

- `AuditResult` is an axum-side projection of `liquers_core::assets::AuditReport`, which has no
  `Serialize`. The projection keeps core free of a serde contract for an internal report;
  `DependencyKey::as_str()` gives the strings.
- `AssetInfo`, `Status` and `Version` already serialize (`Version` as a 32-digit hex string).

#### `AssetsApiBuilder` — two new fields

```rust
pub struct AssetsApiBuilder<E: Environment> {
    base_path: String,
    websocket_path: Option<String>,
    read_only: bool, // new, default false
    admin: bool,     // new, default true
    _phantom: PhantomData<E>,
}
```

### New Enums

None. One variant is added to an existing enum.

#### `ErrorType::StatusConflict` (`liquers-core/src/error.rs`)

```rust
/// The operation is valid, but not for an asset in its current status: removing a
/// directory, expiring a `Source`, describing a computed value. Distinct from `NotSupported`,
/// which means the operation is never available. Maps to HTTP 409.
StatusConflict,
```

`ErrorType` is matched exhaustively, and no `_ =>` arms are allowed, so each of these sites gains
an arm:

| File | Site |
|---|---|
| `liquers-core/src/error.rs` | the variant and its constructor only — core has no exhaustive `ErrorType` list (the `ErrorType::…` hits there are inside constructors) |
| `liquers-core/src/assets.rs` | the `PersistenceStatus` classification (≈ line 2251) → `NotPersisted` |
| `liquers-axum/src/api_core/error.rs` | `error_to_status_code` → `CONFLICT` (≈ line 32); `parse_error_type` (≈ 83); the variant list (≈ 110–123), the exhaustive match (≈ 136–149) and the list (≈ 322–326) in its tests |
| `liquers-py/src/error.rs` | both conversion directions; the Python-side `ErrorType` enum gains the variant |
| `liquers-web/src/error.rs` | both directions, string `"status_conflict"`; the list in `liquers-web/tests/objects_OBJECT.rs` |

The new constructor follows the typed-constructor rule:

```rust
impl Error {
    pub fn status_conflict(key: &Key, status: Status, operation: &str) -> Self;
    // message: "Cannot {operation} '{key}': asset status is {status:?}"; sets key and query.
}
```

### ExtValue Extensions (if applicable)

None.

## Trait Implementations

### `AssetManager<E>` (`liquers-core/src/assets.rs`)

```rust
pub trait AssetManager<E: Environment>: MaybeSend + MaybeSync
    + DependencyManagerAccess<E>
    + KeyMutationAccess            // new internal supertrait, see below
{
    /// Status-aware removal. Now a DEFAULT method; the two per-manager bodies are deleted.
    async fn remove(&self, key: &Key) -> Result<(), Error> { /* see "remove" below */ }

    /// NEW. Expire a keyed asset and cascade to its dependents, whether it is live or only stored.
    async fn expire(&self, key: &Key) -> Result<(), Error> { /* default */ }

    /// NEW. Set `title` and/or `description` of a `Source` asset. Data and version are unchanged.
    async fn set_description(
        &self,
        key: &Key,
        title: Option<String>,
        description: Option<String>,
    ) -> Result<(), Error> { /* default */ }

    // changed store-only branch: a stored `Source` is left alone (no-op), matching
    // `AssetRef::to_override`, instead of being rewritten to an `Override` without a recipe
    async fn to_override(&self, key: &Key) -> Result<(), Error> { … }

    /// NEW, required (both managers hold a query map). The live asset for a query, without
    /// creating or submitting one. A pure-key query delegates to `lookup_key_asset`.
    fn lookup_query_asset(&self, query: &Query) -> Option<AssetRef<E>>;

    /// NEW, default. See "Removal of directories".
    async fn removedir(&self, key: &Key) -> Result<(), Error> { /* default */ }

    // changed body: uses the live asset directly instead of `self.get(key)`
    async fn get_asset_info(&self, key: &Key) -> Result<AssetInfo, Error> { … }
    // …all other methods unchanged
}
```

**Why defaults behind a lock accessor.** Both managers serialize keyed mutations with a
manager-wide `key_mutation_lock: tokio::sync::Mutex<()>`, and a default trait method cannot reach
it. Today every locking method (`remove`, `set_binary`, `set_state`, `to_override`) is therefore
written out twice. A small internal supertrait, modelled on the existing `DependencyManagerAccess`,
lets the new logic live once:

```rust
/// Internal: the lock that serializes keyed mutations (remove, set_*, to_override, expire,
/// set_description). Not part of the supported API.
pub(crate) trait KeyMutationAccess {
    fn key_mutation_lock(&self) -> &tokio::sync::Mutex<()>;
}
impl<E: Environment> KeyMutationAccess for DefaultAssetManager<E>   { … &self.key_mutation_lock }
impl<E: Environment> KeyMutationAccess for ImmediateAssetManager<E> { … &self.key_mutation_lock }
```

Adding a supertrait to a public trait only breaks out-of-tree implementors. None exists: the only
implementors are `DefaultAssetManager` and `ImmediateAssetManager`, and the `pub(crate)`
`DependencyManagerAccess` supertrait already seals the trait in practice.

**Lock discipline.** A default method holding the lock may call only methods that do not take it
themselves: `lookup_key_asset`, `remove_key_asset`, `untrack_expiration`, `recipe_opt`,
`cascade_expire_dependents`, `expire_dependencies_result`, store calls, and `AssetRef` methods.
`cascade_expire_dependents` is already called under the lock by `set_binary`. It must **not** call
`get`, `owned_key_asset`, `to_override`, `set_binary`, `set_state` or `remove_expired_from_maps`,
all of which lock; `tokio::sync::Mutex` is not reentrant, so doing so would deadlock.

#### `remove(key)` — the decision table

The status is read from the live asset if there is one, otherwise from the stored metadata.
`has_recipe` is `recipe_opt(key)?.is_some()`.

| Status | `has_recipe` | Action | Dependents | Stored afterwards |
|---|---|---|---|---|
| `Directory` | any | `Err(status_conflict(key, Directory, "remove"))` | — | unchanged |
| any other | `false` | **delete**: cancel and unmap the live asset, `untrack_expiration`, `dm.remove`, `store.remove` | `cascade_expire_dependents` **before** `dm.remove` | nothing |
| `Source`, `Override` | `true` | **delete the user value**, as above; the next read is `Recipe` | cascaded | nothing |
| `Ready`, `Expired`, `Error`, `Cancelled`, `Volatile`, `Partial`, in-flight (`Submitted`, `Dependencies`, `Processing`, `Storing`) | `true` | **drop the computed value**: cancel and unmap the live asset, `untrack_expiration`; the key stays in the dependency graph | **not** cascaded | if the store holds the key: `store.set(key, &[], md)`, where `md` is the stored record with `status = Recipe`, data-bearing fields cleared, and `version` kept |
| `None` or `Recipe` (nothing live and nothing stored, or a stored entry already dropped to `Recipe`) | `true` | nothing to do, `Ok(())` (idempotent) | — | unchanged |
| nothing live, nothing stored | `false` | `Err(Error::key_not_found(key))` | — | — |

- **Which status is decisive:** a live asset whose status is `None` or `Recipe` has not produced
  anything yet (it was just created by a `get`), so the stored status decides instead; otherwise
  the live status wins. A stored `Metadata::LegacyMetadata` in the drop-computed row cannot carry
  the kept version faithfully, so that case is handled as a delete (`store.remove`) — same
  outcome as today.

- **Why cascade before `dm.remove`:** the cascade walks the graph edges that `dm.remove` deletes.
  This is the order `ASSET-REMOVE-FORGETS-DEPENDENTS` asks for.
- **Why the metadata record is kept:** `AssetManager::version` reads stored metadata, so
  `trigger_dependency_audit` keeps treating dependents as valid. `Status::Recipe.has_data()` is
  false, so `try_fast_track` skips the entry and `get_binary_any_status` returns `None`: nothing
  reads the empty bytes as a value.
- **A dropped dependency must not block its dependents' fast track.**
  `AssetData::dependency_blocks_fast_track` (`assets.rs` ≈1076) reads a dependency's *stored*
  status when no live asset holds it, and today refuses anything but `Ready | Source | Override`.
  A stored `Recipe` would therefore force every dependent of a dropped intermediate to re-evaluate
  after a restart (or whenever the dependent is loaded from the store), which defeats the point of
  keeping the version. The store branch of that function treats `Status::Recipe` as **not
  blocking** — the same answer it already gives for a dependency the store does not hold at all.
  The live-asset branch is unchanged. Tested by AMR24.
- **Why the recomputed case does not cascade now:** if recomputation produces a different
  content hash, `register_version` cascades at that point, and only then.
- **Readers of the kept entry, checked:** `try_fast_track` refuses a stored `Recipe`, so the next
  `get` evaluates and the evaluation's store write replaces the entry; `get_any_status` and
  `get_binary_any_status` return `None` (`has_data()` is false); `expire_stored_copy` leaves
  `Recipe` alone; `contains` stays `true`; `get_asset_info` and `listdir` report `Recipe` with the
  kept version. `AsyncStore::set` implementations rewrite only `Status::None` in
  `finalize_metadata`, so `Recipe` is stored as given (memory, file and OpenDAL stores; the file and
  OpenDAL stores leave a zero-byte data object beside the sidecar). The Store API's `GET data`
  serves the empty body with `Recipe` metadata, which is accurate.
- **The kept dependency-graph entry is harmless.** A later cascade that reaches the key finds no
  live asset and calls `expire_stored_copy`, which ignores `Recipe`; the cascade still continues
  through the key's edges to its dependents, which is correct because they were derived from the
  old value. The weak-reference `dependent_assets` of the key are taken (and dead ones filtered) by
  the next cascade, as for any key.
- **Behaviour change** for callers of today's `remove` on a recipe key: the stored metadata
  survives, and `contains(key)` stays `true`. Core tests that assert otherwise are updated (Phase 4).

#### `expire(key)`

Holds the lock. The error for any status that cannot be expired is `status_conflict(…, "expire")`.

1. **Live asset** (unless its status is `None`/`Recipe`, which falls through to the store as in
   `remove`): `Ready`, `Override` or `Expired` → `asset.expire()`, which already cascades; any
   other status → `status_conflict(key, status, "expire")` returned by the manager itself, so the
   error carries `key` uniformly. `mark_expired_status`'s own refusals also change from
   `general_error` to `ErrorType::StatusConflict` (it may run on a non-keyed asset, so it cannot
   use the keyed constructor; note `Error::with_key` sets the `query` field, not `key` —
   `ERROR-WITH-KEY-SETS-QUERY-FIELD`).
2. **Stored only:**
   - `Ready` or `Override`: reuse `expire_stored_copy(store, key)`, then `cascade_expire_dependents`.
   - `Expired`: `Ok(())`, idempotent, matching `AssetRef::expire`.
   - `Source`: `status_conflict`, because there is no recipe to recover from.
   - any other stored status: `status_conflict`.
3. **Neither live nor stored:** `status_conflict(…, Recipe, "expire")` if a recipe exists,
   otherwise `key_not_found`.

#### `set_description(key, title, description)`

Holds the lock. If both arguments are `None`, it returns
`Err(Error::from_error(ErrorType::ParameterError, msg))` — there is no dedicated constructor, and
`from_error` with a message is the existing idiom (`validate_metadata_hard` uses it).

- The status must be `Source`, whether live or stored. Any other status returns `status_conflict`;
  a key that is neither live nor stored returns `key_not_found`.
- **Live asset:** the new `AssetRef::set_description_fields(title, description)` (`pub(crate)`)
  updates the in-memory `MetadataRecord`.
- **Stored:** `store.get_metadata` → set the fields → `store.set_metadata`.
- **Data and version are untouched,** so nothing cascades: `Version` hashes content, not metadata.

#### `get_asset_info(key)` — no longer evaluates

```rust
if let Some(asset) = self.lookup_key_asset(key) { return asset.get_asset_info().await; }
// …store, then recipe provider, as today
```

Today the live-asset branch calls `self.get(key)`, which treats a cached `Expired`, `Error` or
`Cancelled` entry as a miss and **re-evaluates** it. `GET info` and `GET listdir` promise a
description without evaluation, so they report that entry's current status instead.

The body exists **twice**: the trait default (`assets.rs` ≈4067, used by `ImmediateAssetManager`)
and `DefaultAssetManager`'s own override (≈5518). Both change; the simplest form is to delete the
override, whose only difference is diagnostic `eprintln!`s. In the same edit the final
"not found" branch changes from `general_error` (HTTP 500) to `Error::key_not_found(key)`, which
`GET info`'s 404 and AMR01/AAE22 rely on. The only other caller, `listdir_asset_info` (and through
it the interpreter's directory listing, `interpreter.rs` ≈784), only gains from not evaluating.

### `IntoResponse` — unchanged

A 201 is `(StatusCode::CREATED, ApiResponse::ok(…)).into_response()`. axum lets the tuple's
status override the response's own status. `ApiResponse` gains no field.

## Generic Parameters & Bounds

The handlers are generic over `E: Environment`, exactly like the existing ones: `State<EnvRef<E>>`.
No new generic types. `KeyMutationAccess` has no parameters.

## Sync vs Async Decisions

| Function | Async? | Rationale |
|---|---|---|
| all handlers | yes | axum handlers; they call async `AssetManager` methods |
| `AssetManager::{remove, expire, set_description, get_asset_info}` | yes | store I/O and a tokio lock |
| `AssetRef::set_description_fields` | yes | takes the asset's `RwLock` |
| `KeyMutationAccess::key_mutation_lock` | no | returns a reference; locking happens at the call site |
| `ValueDescription::{from_json, from_params, into_metadata_record}` | no | pure transformation; the registry is passed in |
| `AssetsApiBuilder::{read_only, with_admin, build}` | no | router construction |

## Function Signatures

### `liquers-axum/src/assets/value_description.rs`

```rust
impl ValueDescription {
    /// From a `DataEntry.metadata` object. Returns the description and the names of the
    /// top-level fields that were not taken (sorted, reported in `message`).
    /// A non-object value → `Err(ParameterError)`; a present-but-non-string field → `Err(ParameterError)`.
    pub fn from_json(value: &serde_json::Value) -> Result<(Self, Vec<String>), Error>;

    /// From `POST data` query parameters; unknown parameter names are returned as ignored.
    pub fn from_params(params: &HashMap<String, String>) -> (Self, Vec<String>);

    /// Fill absent fields from the key's current value (`AssetInfo`). Never `media_type`:
    /// `AssetInfo.media_type` is the *effective* type, and copying it would turn a derived type
    /// into an override. Rules (final review):
    /// - `type_identifier` and `data_format` are filled **as a pair**, only when the client gave
    ///   neither, and only from a previous whose `status.has_data()` and whose `type_identifier`
    ///   is non-empty. A never-evaluated recipe key reports `type_identifier: ""` and the
    ///   recipe's `data_format` (e.g. `txt`); copying those would give a 400 (unknown `""`) or a
    ///   422 (`Bytes` cannot be `txt`) for a plain `POST data` onto a recipe key (Q11). A client
    ///   that names only a type must not inherit a format that type may not support.
    /// - `title` and `description` are filled when absent and the previous value is non-empty.
    pub fn or_previous(self, previous: Option<&AssetInfo>) -> Self;

    /// Build a fresh record. `type_identifier` defaults to `"Bytes"`; `type_name` comes from the
    /// registry's `TypeInfo`. An identifier the registry does not contain → `Err(ParameterError)`
    /// (checked here so the client gets a 400, not the 500 of `validate_metadata_hard`).
    pub fn into_metadata_record(self, registry: &TypeRegistry) -> Result<MetadataRecord, Error>;
}
```

### `liquers-axum/src/assets/handlers.rs` — helpers

```rust
/// Key family: `parse_key` on the path. A `ParseError` (400) whose message adds a hint when the
/// path starts with `-` ("key routes take a bare key; use /q/ for a query").
fn key_from_path(path: &str) -> Result<Key, Response>;
/// Query family: `parse_query` on the path; ParseError → 400.
fn query_from_path(path: &str) -> Result<Query, Response>;
fn error_response(e: &Error, message: &str) -> Response;          // ApiResponse::error(error_to_detail(e), …)
fn created<T: Serialize>(result: T, message: impl Into<String>) -> Response; // 201
async fn status_after_remove<E: Environment>(am: &E::AssetManager, key: &Key) -> Status; // Recipe | None
```

The earlier `key_from_path(path, operation)` parsed a query and refused a non-key query with 501.
It is gone. With a key family there is no "non-key query on a key route": the path simply fails
to parse as a key.

### Handlers — new or changed

Handlers live in two modules so the family is visible in the code as well as in the URL:
`assets/query_handlers.rs` (renamed from today's `handlers.rs`, whose four real handlers are
already query-addressed) and `assets/key_handlers.rs` (new). Shared helpers go in
`assets/common.rs`. Mode (a) is request-and-wait, (b) submit, (c) observe.

```rust
// ---- assets/query_handlers.rs — parse_query
pub async fn q_get_data_handler<E>(State<EnvRef<E>>, Path<String>) -> Response;                // (a) get_asset + get_binary
pub async fn q_get_entry_handler<E>(State<EnvRef<E>>, Path<String>, HeaderMap, AxumQuery<HashMap<String,String>>) -> Response; // (a)
pub async fn q_submit_handler<E>(State<EnvRef<E>>, Path<String>) -> Response;                  // (b) get_asset, not awaited → AssetInfo
pub async fn q_info_handler<E>(State<EnvRef<E>>, Path<String>) -> Response;                    // (c) lookup_query_asset → AssetInfo | 404
pub async fn q_get_metadata_handler<E>(State<EnvRef<E>>, Path<String>) -> Response;            // (c) lookup_query_asset → metadata | 404
pub async fn q_version_handler<E>(State<EnvRef<E>>, Path<String>) -> Response;                 // (c) lookup → version (zeros = unknown) | 404
pub async fn q_cancel_handler<E>(State<EnvRef<E>>, Path<String>) -> Response;                  // lookup → cancel | 404; never creates

// ---- assets/key_handlers.rs — parse_key
pub async fn key_get_data_handler<E>(State<EnvRef<E>>, Path<String>) -> Response;              // (a) get(key) + get_binary
pub async fn key_get_entry_handler<E>(State<EnvRef<E>>, Path<String>, HeaderMap, AxumQuery<HashMap<String,String>>) -> Response; // (a)
pub async fn key_submit_handler<E>(State<EnvRef<E>>, Path<String>) -> Response;                // (b) get(key), not awaited
pub async fn key_info_handler<E>(State<EnvRef<E>>, Path<String>) -> Response;                  // (c) get_asset_info
pub async fn key_get_metadata_handler<E>(State<EnvRef<E>>, Path<String>) -> Response;          // (c) live asset → store → recipe; never get()
pub async fn key_version_handler<E>(State<EnvRef<E>>, Path<String>) -> Response;               // (c) AssetManager::version
pub async fn key_contains_handler<E>(State<EnvRef<E>>, Path<String>) -> Response;              // (c)
pub async fn key_recover_handler<E>(State<EnvRef<E>>, Path<String>, HeaderMap, AxumQuery<HashMap<String,String>>) -> Response; // (c)
pub async fn key_listdir_handler<E>(State<EnvRef<E>>, Path<String>, AxumQuery<HashMap<String,String>>) -> Response;          // (c)
pub async fn key_listdir_root_handler<E>(State<EnvRef<E>>, AxumQuery<HashMap<String,String>>) -> Response;                   // (c)
pub async fn key_post_data_handler<E>(State<EnvRef<E>>, Path<String>, AxumQuery<HashMap<String,String>>, Bytes) -> Response;
pub async fn key_post_entry_handler<E>(State<EnvRef<E>>, Path<String>, HeaderMap, AxumQuery<HashMap<String,String>>, Bytes) -> Response;
pub async fn key_post_metadata_handler<E>(State<EnvRef<E>>, Path<String>) -> Response;        // specified refusal, 501
pub async fn key_remove_handler<E>(State<EnvRef<E>>, Path<String>) -> Response;               // DELETE data|entry, GET remove
pub async fn key_removedir_handler<E>(State<EnvRef<E>>, Path<String>) -> Response;            // DELETE|GET removedir
pub async fn key_makedir_handler<E>(State<EnvRef<E>>, Path<String>) -> Response;              // PUT|GET makedir
pub async fn key_description_handler<E>(State<EnvRef<E>>, Path<String>, Option<Json<DescriptionRequest>>, AxumQuery<DescriptionRequest>) -> Response; // POST body or GET params
pub async fn key_expire_handler<E>(State<EnvRef<E>>, Path<String>) -> Response;
pub async fn key_override_handler<E>(State<EnvRef<E>>, Path<String>) -> Response;
pub async fn key_cancel_handler<E>(State<EnvRef<E>>, Path<String>) -> Response;               // lookup_key_asset → cancel | 404
pub async fn key_audit_handler<E>(State<EnvRef<E>>, Path<String>) -> Response;   // served at admin/audit/{*key}

// ---- admin (`admin/` prefix)
pub async fn audit_all_handler<E>(State<EnvRef<E>>) -> Response;
pub async fn refresh_command_versions_handler<E>(State<EnvRef<E>>) -> Response;
```

- One handler serves a POST/PUT/DELETE route and its GET alternative. The route table decides
  which methods exist.
- **`submit`** returns as soon as `get_asset`/`get` returns. On the queued manager that is right
  after scheduling. The handler never awaits the value.
- **Observe handlers never call `get_asset` or `get`.** That is the invariant AAE tests assert: an
  observe call on an unevaluated recipe key leaves it in `Recipe`.
- **`key_cancel`/`q_cancel`** use the lookups, so they never start an evaluation. This closes
  `AXUM-ASSETS-CANCEL-STARTS-EVALUATION` (I3).

Today's `get_entry_handler` ignores the `Accept` header: it passes an empty `HeaderMap` to
`select_format`. Both entry handlers and `key_recover_handler` take the real headers. The fix is
one line, and the spec already promises `Accept` negotiation.

### `liquers-axum/src/assets/builder.rs`

```rust
impl<E: Environment> AssetsApiBuilder<E> {
    /// Omit every state-changing key route (POST/PUT/DELETE except `key/cancel`) and the admin
    /// routes. `q/cancel` and `key/cancel` stay: cancelling is not a data change.
    pub fn read_only(mut self) -> Self;
    /// Include (default) or omit every `admin/` route: `audit`, `audit/{*key}`, `refresh_command_versions`.
    pub fn with_admin(mut self, enabled: bool) -> Self;
}
```

**Pre-existing blocker fixed here:** `build()` registers the WebSocket route as
`format!("{}/*query", ws_path)`. axum 0.8 rejects a segment starting with `*` and **panics** in
`Router::route`, so every `AssetsApiBuilder::new(…).build()` with the default WebSocket path
panics today (the `assets_recipes_basic` example included). The route becomes
`format!("{}/{{*query}}", ws_path)`. Filed as `AXUM-ASSETS-WEBSOCKET-ROUTE-PANICS`; this design
closes it.

An omitted route answers axum's own 405, where the path serves other methods, or 404. Pretending
a disabled route exists, with a §3 envelope, would hide the server's configuration from the client.

## WebSocket Notifications

In scope at the user's request (2026-09-28): the WebSocket must be consistent with `/api/assets`,
and anything substantial it fails to cover is part of this design.

### What exists today (`liquers-axum/src/assets/websocket.rs`, spec §5.2)

| # | Finding | Severity |
|---|---|---|
| W1 | **Nothing is forwarded.** `handle_subscribe` resolves the asset, stores it and sends one `Initial` with `metadata: None`. The comment says forwarding "would require splitting the socket", and `convert_notification` is dead code. The notification channel the query family relies on for non-keyed feedback does not work. | blocking |
| W2 | The route `{ws}/*query` panics under axum 0.8 (`AXUM-ASSETS-WEBSOCKET-ROUTE-PANICS`). The `{*query}` in the URL is **ignored**, although the spec subscribes to it on connect. The default path is `{base}/ws`; the spec says `/ws/assets/{*query}`. | blocking / spec drift |
| W3 | Client messages are `#[serde(tag = "action")]` with Rust variant names, so the server expects `"Subscribe"` and `"UnsubscribeAll"` while the spec documents `"subscribe"` and `"unsubscribe_all"`. A client that follows the spec has **every message silently dropped**: the parse error is only printed with `eprintln!`, and no reply is sent. | blocking |
| W4 | Server messages diverge from the spec. `Submitted`, `Processing` and `Ready` stand where the spec has `JobSubmitted`, `JobStarted` and `JobFinished`. `LogMessage` is faked as `StatusChanged("Logging")`. Progress is flattened to a ratio, losing message, done, total and eta. `Expired` becomes a `StatusChanged` string. Errors are strings instead of the §3 `ErrorDetail`. `RecipeDetected` and `Cancelled` are never produced. The spec's status list names statuses that do not exist (`External`, `Unavailable`) and omits real ones. | spec drift |
| W5 | **Subscriptions are query-only.** A bare key parses as an action, and subscribing calls `get_asset`. Nothing addresses a key, and nothing survives the asset being replaced: `remove`, `set_binary` and `set_state` unmap the live asset, and a subscriber holding the old one simply goes quiet. `ASSETS.md` Scenario 5 promises a `Removed` notification that `AssetNotificationMessage` does not have. | substantial gap |
| W6 | The core channel is a `tokio::sync::watch`. It keeps the latest message only, and its documentation says to "re-read authoritative asset state after each change". The WebSocket never re-reads, so even with forwarding a client could miss the final state. | design constraint |

Out of scope, with existing issues:
- Connection limits, message-size caps, timeouts and slow clients: `AXUM-WEBSOCKET-HARDENING`.
- Manager-level or scope subscriptions ("tell me when anything under `mem/` changes"), and events
  from assets that are not live: `ASSET-EXPIRATION-EVENTS-CANNOT-BE-OBSERVED-EXCEPT-PER-ASSET`.

### Design

**Routes (O5).** One endpoint per family, mirroring REST. The paths are configurable through
`with_websocket_path(base)`, which replaces `{base}/ws`, and `without_websocket()` disables both:
- `GET ws/q` and `GET ws/q/{*query}`: subscriptions are **queries**, parsed with `parse_query`
  and requested through `get_asset`. The URL path, if present, is subscribed on connect, as §5.2
  promises.
- `GET ws/key` and `GET ws/key/{*key}`: subscriptions are **keys**, parsed with `parse_key` and
  requested through `get`. The URL path is subscribed on connect.

**Client messages** (`#[serde(tag = "action", rename_all = "snake_case")]`, matching §5.2). The
address field names the object the endpoint parses (O6): `query` on `ws/q`, `key` on `ws/key`.
Using the other field is an `Error` reply.

```rust
pub enum ClientMessage {
    Subscribe { query: Option<String>, key: Option<String> },   // requests the asset (O9)
    Unsubscribe { query: Option<String>, key: Option<String> },
    UnsubscribeAll,
    Ping,
}
```

A message that does not parse, names the wrong address field, or exceeds a limit (I6) gets an
`Error` reply carrying an `ErrorDetail`, instead of being dropped.

**Server messages.** Every notification carries the identity of what was subscribed plus a
**snapshot re-read after the change**. That answers W6: whatever the watch coalesced, the client
always ends up holding current state.

```rust
#[serde(tag = "type")]
pub enum NotificationMessage {
    // one per AssetNotificationMessage variant, spec names (§5.2):
    Initial               { #[serde(flatten)] head: Head },
    JobSubmitted          { #[serde(flatten)] head: Head },
    JobStarted            { #[serde(flatten)] head: Head },
    StatusChanged         { #[serde(flatten)] head: Head, status: Status },
    ValueProduced         { #[serde(flatten)] head: Head },
    ErrorOccurred         { #[serde(flatten)] head: Head, error: ErrorDetail },
    LogMessage            { #[serde(flatten)] head: Head },        // text is in info.message
    PrimaryProgressUpdated   { #[serde(flatten)] head: Head, progress: ProgressEntry },
    SecondaryProgressUpdated { #[serde(flatten)] head: Head, progress: ProgressEntry },
    JobFinished           { #[serde(flatten)] head: Head },
    Expired               { #[serde(flatten)] head: Head },
    Removed               { #[serde(flatten)] head: Head },        // new, also added to AssetNotificationMessage (O8)
    // protocol replies — not asset notifications, so they have no core counterpart
    // control replies
    Pong { timestamp: String },
    UnsubscribedAll { timestamp: String },
    Error { timestamp: String, error: ErrorDetail },
}

pub struct Head {
    pub asset_id: u64,
    pub query: Option<String>,   // as subscribed (query subscriptions)
    pub key: Option<String>,     // as subscribed (key subscriptions), or the asset's key if keyed
    pub timestamp: String,
    pub info: Option<AssetInfo>, // AssetRef::get_asset_info() re-read after the change
}
```

`convert_notification` becomes an exhaustive `match` on `AssetNotificationMessage`, with no
invented statuses. `RecipeDetected` and `Cancelled` are deleted, because the core never produces
them: a cancel arrives as `StatusChanged(Cancelled)`.

**Forwarding (W1).** The socket is split once.
- One **writer task** owns the sink and drains a `tokio::sync::mpsc::Sender<NotificationMessage>`.
- Each subscription is a task holding the `AssetRef` and its `watch::Receiver`. On every
  `changed()` it reads the message, re-reads `get_asset_info()`, and sends one
  `NotificationMessage` to the writer.
- The subscription map holds the tasks' `JoinHandle`s, keyed by `Address::Query(String)` or
  `Address::Key(Key)`. `Unsubscribe` and `UnsubscribeAll` abort the handles, and a disconnect
  aborts them all.
- The type-erased `Arc<dyn Any>` map disappears.

**A subscription is tied to one asset (O10, W5).** Today a keyed asset that has not been
requested cannot be managed, so there is nothing to wait for. A subscription therefore follows
one `AssetRef` (one `AssetData`) for its whole life, and **ends** when that asset's lifecycle
ends. The terminal notifications are:
- `StatusChanged` to `Error` or `Cancelled`;
- `Expired`, after which the manager replaces the asset on the next request;
- `Removed`.

The server forwards the terminal notification, with the `info` snapshot, then drops the
subscription. To continue, the client subscribes again, which requests a fresh asset. For a key
subscription that terminal `info` is read with `get_asset_info(key)`, which does not evaluate.

To make `Removed` observable, the core gains one notification (O8: kept consistent with the
original notification set, and meaningful for any in-process observer as well):

```rust
pub enum AssetNotificationMessage { …, Removed }  // liquers-core/src/assets.rs
```

- It is sent by `remove`, `set_binary` and `set_state` just before they unmap a live asset. It is
  **not** sent by `remove_expired_from_maps`, since expiry already announced `Expired`.
- `AssetNotificationMessage` is matched exhaustively in the WebSocket and in
  `liquers-lib/src/ui/element.rs` (≈516; `Removed` joins the arm that ignores status-only
  messages). Core's own matches gain the arm too; Phase 4 lists every site.
- Observing without requesting is not offered (O9). It would change how assets are managed, and
  it can be added later if it becomes important.

**Consistency with the REST families.**

| REST | WebSocket |
|---|---|
| `/q/…` (`get_asset(query)`) | `ws/q`: `{"action":"subscribe","query":"…"}`, or the URL path `ws/q/{*query}` |
| `/key/…` (`get(key)`) | `ws/key`: `{"action":"subscribe","key":"…"}`, or the URL path `ws/key/{*key}` |
| `GET q/info`, `GET key/info` polling | the reliable fallback: notifications can be coalesced or missed (W6), polling cannot |
| `GET q/info`, `GET key/info` → `AssetInfo` | every notification carries `info: AssetInfo` |
| `POST key/cancel`, `POST q/cancel` | cancellation arrives as `StatusChanged` with `status: "Cancelled"` |
| `DELETE key/…`, `POST key/data` | `Removed`, then the subscription ends; subscribe again for the new asset |
| `read_only()` | no effect: subscribing changes no data |

### Functions

```rust
// assets/websocket.rs
pub async fn ws_query_handler<E>(WebSocketUpgrade, Option<Path<String>>, State<EnvRef<E>>) -> Response; // ws/q
pub async fn ws_key_handler<E>(WebSocketUpgrade, Option<Path<String>>, State<EnvRef<E>>) -> Response;   // ws/key
async fn handle_socket<E>(socket: WebSocket, env: EnvRef<E>, family: Family, initial: Option<String>);
async fn writer_task(sink: SplitSink<WebSocket, Message>, rx: mpsc::Receiver<NotificationMessage>);
async fn subscription_task<E>(env: EnvRef<E>, address: Address, asset: AssetRef<E>,
                              tx: mpsc::Sender<NotificationMessage>);
fn convert_notification(head: Head, n: AssetNotificationMessage) -> NotificationMessage; // exhaustive
```

**Tests** (Phase 3 to add). An in-process server on `127.0.0.1:0` with a `tokio-tungstenite`
client (already a dev-dependency). The tests cover:
- subscribing by query and receiving `JobFinished` with `info.status == Ready`;
- subscribing by key, then `DELETE` → `Removed`, and the subscription ends (no further messages);
  subscribing again follows the new asset;
- `Error` and `Cancelled` end a subscription;
- a `key` field on `ws/q` (and a `query` field on `ws/key`) → `Error` reply;
- the spec's snake_case client messages;
- a malformed message → an `Error` reply;
- `unsubscribe` stops messages;
- the URL-path subscription on connect.

## Web API Known Issues in Scope

Decided 2026-09-28 (Q21, Q22): the goal is a working web and WebSocket interface, delivered as
**one design**. Every open issue of the web API that the user accepted is handled here. Each
subsection names the issue, the change, and the test that proves it.

### I1 `AXUM-ASSETS-API-SERVES-ONLY-BYTES-AND-TEXT` (P2)

**Problem.** Assets `GET data` and `GET entry` turn the value into bytes with `try_into_bytes`,
which is a conversion, not serialization. Core accepts only `Bytes` and `Text`, and
`CombinedValue` refuses every extended value. So a number, a JSON object, a DataFrame or an image
fails, while `GET /q` serves the same query correctly through `AssetRef::get_binary()`, which
applies the effective data format, reuses the cached encoding and refuses an expired read.

**Change.** One shared function in `assets/common.rs`, used by every byte-returning route:
`q/data`, `q/entry`, `key/data` and `key/entry`. `key/recover` uses `get_binary_any_status`,
which is already format-aware.

```rust
/// The asset's serialized form, exactly as `GET /q` serves it.
async fn asset_bytes<E: Environment>(asset: &AssetRef<E>) -> Result<(Arc<Vec<u8>>, Arc<Metadata>), Error> {
    asset.get_binary().await
}
```

**Test.** An integer, a JSON object and an array (core `Value`) are served by
`q/data` and `key/data` with the same bytes and `Content-Type` as `GET /q`.

### I2 `EXPIRATION-RECOVERY-WEB-API` (P2)

**Already delivered by this design.**
- The recovery read is `GET key/recover`, through `get_binary_any_status`. It never evaluates, and
  the metadata in the response says `Expired`.
- Promotion is `POST key/override`, through `to_override`.
- Both are key-only, as the issue requires.
- The WebSocket reports `Expired` as its own message (O8).

Close the issue on merge with those tests as evidence.

### I3 `AXUM-ASSETS-CANCEL-STARTS-EVALUATION` (P3)

`key/cancel` uses `lookup_key_asset` and never creates an asset. `q/cancel` needs the
non-creating query lookup from **O2**. With O2 accepted, the issue closes. Without it, it stays
open for queries only.

### I4 `AXUM-HANDLER-TEST-COVERAGE` (P2)

The assets API gets the suite from Phase 3. In addition, **every route of every builder** gets at
least one in-process test (`tower::ServiceExt::oneshot`), which is also how the specification
audit (I8) is proven:

| Builder | Routes to cover |
|---|---|
| `StoreApiBuilder` | `data` (GET/PUT/DELETE), `metadata` (GET/PUT), `entry` (GET/PUT/DELETE), `listdir`, `is_dir`, `contains`, `keys`, `makedir`, `removedir`, `upload` (multipart), and the opt-in destructive GETs `remove`, `removedir` and `makedir` with and without `with_destructive_gets()` |
| `QueryApiBuilder` | `GET` and `POST {base}/{*query}`: success, a parse error, an evaluation error, and the timeout (I5) |
| `RecipesApiBuilder` | `listdir`, `data`, `metadata`, `entry`, `resolve` |
| `AssetsApiBuilder` | Phase 3's suite, plus a `build()` test for every switch combination (the WebSocket route panic is a `build()` failure) |

Each builder also gets a test that `build()` succeeds with default options. The issue closes on
merge.

### I5 `AXUM-QUERY-TIMEOUT-HARDCODED` (P2)

`get_query_handler` (`query/handlers.rs` ≈41) polls with an inline `Duration::from_secs(30)`.

**Change:**

```rust
impl<E: Environment> QueryApiBuilder<E> {
    /// How long `GET/POST {base}/{*query}` waits for a value (default 30 s, unchanged).
    pub fn with_timeout(mut self, timeout: std::time::Duration) -> Self;
}
```

The timeout reaches the handler through a small `QueryApiConfig` layered with `Extension`, so the
handler signature stays generic over `E` only. On timeout the error message names the duration and
points to the long-running path: "use {assets}/q/submit, then poll GET {assets}/q/info or subscribe on
{assets}/ws/q, to follow a long evaluation". The `ErrorType` stays `ExecutionError`; see O11.

**Test:** a command that sleeps longer than a 100 ms timeout gives the documented error.

### I6 `AXUM-WEBSOCKET-HARDENING` (P3), partial

Taken now, because the socket is rewritten anyway:
- **Message-size cap:** `WebSocketUpgrade::max_message_size`, default 64 KiB. A client message is
  a small JSON control message.
- **Per-socket subscription cap:** default 256. A `subscribe` beyond it gets an `Error` reply.
- **Disconnect cleanup:** dropping the socket aborts every subscription task. It does **not**
  cancel the subscribed evaluations, because other clients may share them.
- **Tests:** oversized message, subscription cap, and disconnect mid-evaluation (the evaluation
  still completes, and no task is left behind).

```rust
pub struct WebSocketLimits { pub max_message_size: usize, pub max_subscriptions: usize }
impl Default for WebSocketLimits { /* 64 * 1024, 256 */ }
impl<E: Environment> AssetsApiBuilder<E> { pub fn with_websocket_limits(mut self, limits: WebSocketLimits) -> Self; }
```

Left in the issue, which stays `accepted` with a progress note: connection-count limits, idle
timeouts and server-initiated keep-alive, and slow-consumer back-pressure policy. The writer
channel is bounded, and a full channel drops that subscription's message; the next change carries
a fresh `AssetInfo` snapshot anyway.

### I7 `LIBRARY-CODE-USES-UNWRAP-AND-EXPECT`, the `liquers-axum` part

The nine library sites, verified 2026-09-28:

| Site | Replacement |
|---|---|
| `axum_integration.rs` 37, 45, 70, 86, 96 (`Response::builder()…unwrap()`) | a `fn build_or_500(builder, body) -> Response` helper that falls back to a plain 500 response; a builder only fails on an invalid header |
| `recipes/handlers.rs` 102 (`"text/plain".parse().unwrap()`) | `HeaderValue::from_static("text/plain")` |
| `recipes/handlers.rs` 190, `assets/handlers.rs` 290 (`format.mime_type().parse().unwrap()`) | `HeaderValue::from_static(format.mime_type())` |
| `store/handlers.rs` 479 | the same `build_or_500` helper |

After this, the crate-wide `cargo clippy -p liquers-axum -- -D clippy::unwrap_used -D
clippy::expect_used` passes (tests are exempt, since clippy lints only non-test code by default).
It replaces Phase 4's file-scoped `awk` check. The issue stays open for the other crates, with a
note that `liquers-axum` is clean.

### I8 `WEB-API-SPECIFICATION-DIVERGES-FROM-IMPLEMENTATION` (P1), and the whole specification

`WEB_API_SPECIFICATION.md` is a `reference/` document and must be true at HEAD. The issue lists
three drifts: the store entry write is `PUT`, `FullApiBuilder` does not exist (nor do the crate
names `liquers_web`/`liquers_web_axum` used with it), and the WebSocket path. This design found
more:
- §5 as a whole (routes, 501 stubs, envelopes);
- §5.2 message names, casing and payloads (W3, W4);
- the non-existent statuses `External`/`Unavailable` in §5.2;
- `GET /api/assets/remove`.

**Change.** Rewrite §5 for the `/q/`, `/key/` and admin families and the WebSocket. Then audit
§2–§4 and §6–§10 against the four builders, **route by route, using the I4 tests as the
evidence**: every documented route either has a passing test or is corrected. Replace
`FullApiBuilder` with the real assembly (`Router::merge` of the four builders), and document the
new builder options (`read_only`, `with_admin`, `with_websocket_limits`, `with_timeout`). Add a
`## History` row and bump `reviewed:`. The issue closes on merge.

### I9 `MEDIA-TYPES-MISSING-FOR-TABULAR-FORMATS` (P3)

In `liquers-core/src/media_type.rs`: `ndjson` → `application/x-ndjson`; `jsonl` →
`application/jsonl`; `arrow`/`feather`/`ipc` → `application/vnd.apache.arrow.file`; `parquet` →
`application/vnd.apache.parquet`. Each value is **checked against the IANA registry during
implementation**, as the issue asks, rather than taken from this document. Add a unit test per
extension. The issue closes on merge.

### I10 `AXUM-ASSETS-WEBSOCKET-ROUTE-PANICS` (P1)

Covered by "WebSocket Notifications" (the route becomes `{{*query}}`) and by I4's `build()` tests.
The issue closes on merge.

## Integration Points

| Crate | File | Change |
|---|---|---|
| liquers-core | `src/error.rs` | `ErrorType::StatusConflict`, `Error::status_conflict` |
| liquers-core | `src/assets.rs` | `KeyMutationAccess` plus two impls; `to_override` store-only branch skips `Source`; default `remove`, delete both per-manager `remove` bodies; `expire`, `set_description`; `get_asset_info` live branch; `AssetRef::set_description_fields`; `mark_expired_status` refusals → `StatusConflict`; `get_asset_info` not-found → `key_not_found` (trait default and the `DefaultAssetManager` override); `dependency_blocks_fast_track` store branch accepts `Recipe`; the `ErrorType` match at ≈2251. Drive-by: the orphaned doc paragraph above `expire_stored_copy` moves back to `DependencyManagerAccess`, whose doc it is |
| liquers-axum | `src/assets/value_description.rs` | new |
| liquers-axum | `src/assets/query_handlers.rs` (renamed from `handlers.rs`), `key_handlers.rs` (new), `common.rs` (new) | the query family (4 existing plus `q_info` and `q_version`), the key family (22 handlers), the shared helpers |
| liquers-axum | `src/assets/builder.rs` | `/q/`, `/key/` and admin routes; unprefixed routes removed; `read_only`, `with_admin`; WebSocket at `{base}/ws/q` and `{base}/ws/key` with `{*query}`/`{*key}` routes (panics today) |
| liquers-axum | `src/assets/websocket.rs` | rewritten per "WebSocket Notifications": writer task and subscription tasks, snake_case client messages with `query`/`key`, spec-named server messages with an `AssetInfo` snapshot, `Error` replies, subscription on the URL path |
| liquers-core | `src/assets.rs` | `AssetNotificationMessage::Removed`, sent by `remove`, `set_binary` and `set_state` before they unmap a live asset; exhaustive matches on `AssetNotificationMessage` gain the arm |
| liquers-axum | `src/assets/common.rs` | `asset_bytes` (I1) and the shared helpers |
| liquers-axum | `src/query/builder.rs`, `query/handlers.rs` | `with_timeout`, `QueryApiConfig` and the timeout message (I5) |
| liquers-axum | `src/axum_integration.rs`, `recipes/handlers.rs`, `store/handlers.rs` | the nine `unwrap()` sites (I7) |
| liquers-axum | `tests/store_api_routes.rs`, `tests/query_api_routes.rs`, `tests/recipes_api_routes.rs` (new) | route tests for every builder (I4) |
| liquers-core | `src/media_type.rs` | tabular media types (I9) |
| specs | `reference/WEB_API_SPECIFICATION.md` | full audit and §5 rewrite (I8) |
| liquers-axum | `examples/assets_recipes_basic.rs` | printed and documented URLs move to `/q/` |
| liquers-axum | `src/assets/mod.rs` | `mod value_description;` |
| liquers-axum | `src/assets/handlers.rs`, `builder.rs` module docs | point at `specs/design/axum-assets-endpoints/` as well as the original `axum-assets-recipes-api` |
| liquers-axum | `src/api_core/error.rs` | 409 mapping, parse, variant lists |
| liquers-axum | `src/assets/tests.rs` | placeholder replaced by handler tests (Phase 3) |
| liquers-axum | `Cargo.toml` | `[dev-dependencies] tower = { version = "0.5.3", features = ["util"] }` for `ServiceExt::oneshot` |
| liquers-py | `src/error.rs` | `ErrorType` variant plus both conversions |
| liquers-web | `src/error.rs`, `tests/objects_OBJECT.rs` | variant string and list |

### Routes (relative to `base_path`)

`(G)` marks a GET alternative, present only with `with_destructive_gets()` (Q25).

| Method and path | Handler | Mode | Disabled by |
|---|---|---|---|
| **query family** | | | |
| `GET q/data/{*query}`, `GET q/entry/{*query}` | `q_get_data_handler`, `q_get_entry_handler` | a | — |
| `POST q/submit/{*query}`, `GET q/submit/{*query}` | `q_submit_handler` | b | — |
| `GET q/info/{*query}`, `GET q/metadata/{*query}`, `GET q/version/{*query}` | `q_info_handler`, `q_get_metadata_handler`, `q_version_handler` | c | — |
| `POST q/cancel/{*query}`, (G) `GET q/cancel/{*query}` | `q_cancel_handler` | — | — |
| `GET ws/q`, `GET ws/q/{*query}` (WebSocket, queries) | `ws_query_handler` | b + c | `without_websocket()` |
| `GET ws/key`, `GET ws/key/{*key}` (WebSocket, keys) | `ws_key_handler` | b + c | `without_websocket()` |
| **key family** | | | |
| `GET key/data/{*key}`, `GET key/entry/{*key}` | `key_get_data_handler`, `key_get_entry_handler` | a | — |
| `POST key/submit/{*key}`, `GET key/submit/{*key}` | `key_submit_handler` | b | — |
| `GET key/info`, `metadata`, `version`, `contains`, `recover` `/{*key}` | `key_*_handler` | c | — |
| `GET key/listdir`, `GET key/listdir/{*key}` (`?deep=true`) | `key_listdir_root_handler`, `key_listdir_handler` | c | — |
| `POST key/data/{*key}`, `POST key/entry/{*key}` | `key_post_data_handler`, `key_post_entry_handler` | — | `read_only` |
| `POST key/metadata/{*key}` | `key_post_metadata_handler` (always 501) | — | — |
| `DELETE key/data/{*key}`, `DELETE key/entry/{*key}`, (G) `GET key/remove/{*key}` | `key_remove_handler` | — | `read_only` |
| `DELETE key/removedir/{*key}`, (G) `GET key/removedir/{*key}` | `key_removedir_handler` | — | `read_only` |
| `PUT key/makedir/{*key}`, (G) `GET key/makedir/{*key}` | `key_makedir_handler` | — | `read_only` |
| `POST key/description/{*key}`, (G) `GET key/description/{*key}?title=&description=` | `key_description_handler` | — | `read_only` |
| `POST key/expire/{*key}`, (G) `GET key/expire/{*key}` | `key_expire_handler` | — | `read_only` |
| `POST key/override/{*key}`, (G) `GET key/override/{*key}` | `key_override_handler` | — | `read_only` |
| `POST key/cancel/{*key}`, (G) `GET key/cancel/{*key}` | `key_cancel_handler` | — | — |
| **admin** (`admin/` prefix, O7) | | | |
| `POST admin/audit/{*key}` (parse_key), (G) `GET admin/audit/{*key}` | `key_audit_handler` | — | `read_only`, `with_admin(false)` |
| `POST admin/audit`, (G) `GET admin/audit` | `audit_all_handler` | — | `read_only`, `with_admin(false)` |
| `POST admin/refresh_command_versions`, (G) `GET admin/refresh_command_versions` | `refresh_command_versions_handler` | — | `read_only`, `with_admin(false)` |

**Unprefixed routes.** The current `GET data|metadata|entry/{*query}` and `POST cancel/{*query}`
are removed, and their handlers move to `/q/` (Open Question O1). The only in-repo client is the
`assets_recipes_basic` example, which is updated.

## Relevant Commands

### New Commands

None. `specs/command_registry.yaml` is unaffected.

### Relevant Existing Namespaces

None is touched. The client of these endpoints is the planned `ns-mem` namespace and the MCP
adapter of `agent-memory-mvp`. That design writes through commands via `Context`, so the only
contract that matters here is the `AssetManager` behaviour above: `remove`, `expire` and
`set_description` are equally reachable from a command through `Context::get_asset_manager`.

## Web Endpoints

Envelope (verified per route, 2026-09-28): **every status output is an `ApiResponse`**. That covers
info, metadata, version, contains, listdir, submit, every mutation's result, every error, and the
admin routes. The exceptions are the value transfers only: `GET data` (raw bytes plus metadata
headers), and `GET entry` and `GET key/recover` (a negotiated `DataEntry`, which already contains
its metadata). The WebSocket has its own protocol. The I8 audit applies the same rule to the Store,
Query and Recipes APIs. `query` is set on every response.

`{key}` below is a **bare key** (`notes/a.txt`), and `{query}` is any query, keyed or not.

| Endpoint | Success | Body `result` | Errors |
|---|---|---|---|
| `GET q/data|entry/{query}` (a) | 200 | bytes via `get_binary` (I1); negotiated `DataEntry` | 400 parse; evaluation errors |
| `POST|GET q/submit/{query}` (b) | 200 | `AssetInfo` right after submission (`Submitted`, `Processing`, or `Ready` if cached) | 400 parse or plan error; 404 missing key |
| `GET q/metadata/{query}` (c) | 200 | the live asset's metadata record | 404 `NotAvailable` when nobody requested it |
| `POST|GET key/submit/{key}` (b) | 200 | as `q/submit` | 400 parse; 404 when neither stored nor declared by a recipe |
| `DELETE key/removedir/{key}` | 200 | `{removed: true}` | 404; the first failing key's error |
| `GET q/info/{query}` (c) | 200 | `AssetInfo` of the live asset's current state (with progress while `Processing`); never starts evaluation | 400 parse; 404 `NotAvailable` "submit it first" |
| `GET q/version/{query}` (c) | 200 | `{version: "…"}`, all zeros (unknown) until the asset has finished | as `q/info` |
| `POST q/cancel/{query}` | 200 | as today | as today |
| `GET key/data|entry/{key}` (a) | 200 | as the `q/` reads, via `get(key)` | 400 parse (a `-R/` path gets a hint); 404 |
| `GET key/metadata/{key}` (c) | 200 | the live asset's, else the stored, else the recipe's metadata; never evaluates | 404 |
| `POST key/cancel/{key}` | 200 | `AssetInfo` after the cancel | 404 when no live asset holds the key |
| `POST admin/audit/{key}` | 200 | `{checked, expired}` | — |
| `GET key/listdir[/{key}]` | 200 | `{assets: [AssetInfo…]}`; with `deep=true`, `{keys: ["a/b.md", …]}` | store error |
| `GET key/info/{key}` | 200 | `AssetInfo` | 404 when absent |
| `GET key/contains/{key}` | 200 | `{contains: bool}` | — |
| `GET key/version/{key}` | 200 | `{version: "…32 hex…"}, `"000…0"` (`Version::unknown()`) when there is no version (O3)` | store read error 500 (an error stays an error; only `Ok(None)` maps to zeros) |
| `GET key/recover/{key}` | 200 | `DataEntry` (CBOR by default; `?format=`, or `Accept`) | 404 when there is no data-bearing state |
| `POST key/data/{key}` | **201** | `AssetInfo` after the write; `message` names ignored parameters | 400 unparsable key, unknown type or bad format |
| `POST key/entry/{key}` | **201** | as above; `message` names the dropped metadata fields | 400 undecodable body or non-object metadata |
| `POST key/metadata/{key}` | — | — | always 501 `NotSupported`: "asset metadata is owned by the asset manager; use POST description for a Source asset's title and description" |
| `DELETE key/data|entry/{key}` | 200 | `{removed: true, new_status: "Recipe" \| "None"}` | 404; 409 for a directory (use `key/removedir`) |
| `POST key/description/{key}` | 200 | `AssetInfo` after the change | 400 when both fields are absent; 404; 409 when not `Source` |
| `POST key/expire/{key}` | 200 | `AssetInfo` after the change | 404; 409 when the status cannot expire |
| `POST key/override/{key}` | 200 | `AssetInfo` after the change; a `Source` is left unchanged | 404 when there is no data (`to_override`'s `key_not_found`) |
| `PUT key/makedir/{key}` | **201** | `AssetInfo` of the directory | store error |
| `POST admin/audit` | 200 | `{checked: [...], expired: [...]}` over every registered gap | — |
| `POST admin/refresh_command_versions` | 200 | `null`, with a message | registry error 500 |

`POST data` parameters: `type_identifier`, `data_format`, `media_type`, `title`, `description`, all
optional. The `Content-Type` header is **not** used as `media_type`: clients send
`application/x-www-form-urlencoded` or `application/octet-stream` by default, and treating that as
a deliberate override would mislabel every note. The default `type_identifier` is `Bytes`, with an
undeclared `data_format`. Such an entry is decoded as `bin` (`MetadataRecord::get_data_format`
substitutes `bin`, not the extension), which yields `Value::Bytes` of exactly the posted bytes;
`GET data` serves them unchanged and `try_into_string` on `Value::Bytes` is a lossy UTF-8 decode,
so a text command downstream still works. `Text` would be the natural
default for text, but it cannot declare `md` (`TEXT-VALUE-CANNOT-BE-STORED-AS-MARKDOWN`).

## Error Handling

| Scenario | ErrorType | HTTP |
|---|---|---|
| path does not parse (`parse_query` on `/q/`, `parse_key` on `/key/`, including a `-R/…` path on a key route) | `ParseError` | 400 |
| `POST metadata` | `NotSupported` | 501 |
| unknown `type_identifier`, bad `DataEntry`, non-string description field, empty `POST description` | `ParameterError` | 400 |
| `data_format` not supported by the type | `SerializationError` (from `validate_metadata_hard`) | 422 |
| key absent | `KeyNotFound` (`Error::key_not_found`) | 404 |
| remove a directory; expire a `Source`, `Recipe` or in-flight asset; describe a non-`Source` | `StatusConflict` | **409** |
| store I/O | as returned by the store (`KeyReadError`, `KeyWriteError`, …) | 500 |

All errors are built with typed constructors (`key_not_found`, `not_supported`, the new
`status_conflict`, and `from_error(ErrorType::ParameterError, msg)` for 400s); `Error::new` is not
used. Handlers never `unwrap()`: the existing
`headers.insert(CONTENT_TYPE, format.mime_type().parse().unwrap())` in `get_entry_handler` is
replaced by `HeaderValue::from_static`, since `mime_type()` returns `&'static str`.

## Serialization Strategy

- Requests: `DataEntry` (CBOR, bincode or JSON, existing codec); `DescriptionRequest` (JSON);
  `POST data` raw bytes plus query parameters.
- Responses: `ApiResponse<T>` JSON. `AssetInfo`, `Status` and `Version` use their existing serde
  forms. No new serde attributes on core types.
- `ValueDescription` derives `Serialize, Deserialize` for tests and possible reuse; the handlers
  parse through `from_json`, which reports the dropped fields that a derive would silently ignore.

## Concurrency Considerations

- **Keyed mutations** (`remove`, `expire`, `set_description`, and the existing `set_binary`,
  `set_state` and `to_override`) are serialized by the manager-wide `key_mutation_lock`. That makes
  `remove`'s status check and its action atomic with respect to a concurrent `POST data`: the race
  Phase 1 set out to avoid.
- **Holding the lock during a cascade** is existing practice (`set_binary`). The cascade does not
  lock, and the lock-discipline list above is what keeps that true.
- **Holding the lock across `cancel()`** (`remove` of an in-flight asset) is existing practice
  too (today's `remove`, `set_binary`). It is not a deadlock but can stall: an evaluation blocked
  on `key_mutation_lock` (e.g. inside `get` → `get_nonvolatile_resource_asset` for one of its
  dependencies) cannot observe the cancel, so `cancel()` waits out its 5-second timeout and
  `remove` then proceeds; the cancelled flag still stops that evaluation's store write. Accepted,
  unchanged by this design.
- **Reads** (`info`, `listdir`, `contains`, `version`, `recover`) take no manager lock. They may
  observe the state just before or just after a concurrent mutation, never a torn record: store
  writes are per-key.
- `listdir_asset_info` is O(n) `get_asset_info` calls, each at most one store metadata read. That
  is acceptable at the MVP's ~300 documents; `STORE-NO-CONTENT-OR-METADATA-SEARCH` covers scale.

## Compilation Validation

- `KeyMutationAccess` as a `pub(crate)` supertrait of a `pub` trait follows the existing
  `DependencyManagerAccess` precedent, so it compiles, possibly with the same `private_bounds` lint
  allowance.
- Default async methods in an `#[async_trait]` trait may take `self.key_mutation_lock().lock().await`;
  the guard is `Send` (`tokio::sync::MutexGuard`), which satisfies `MaybeSend` on native targets.
  On wasm `MaybeSend` is empty.
- A `(StatusCode, Response)` tuple implements `IntoResponse` in axum 0.8.
- `tower::ServiceExt` needs tower's `util` feature, so it is added to dev-dependencies only.
- Adding `ErrorType::StatusConflict` fails compilation at every exhaustive match until the arm is
  added. That is the intended safety net, and the table above lists the known sites.

## References to liquers-patterns.md

- Async by default, with `#[async_trait]`. Errors come only from `liquers_core::error`, through
  typed constructors.
- No `_ =>` on `ErrorType` or `Status`: the `remove` decision table is written as an exhaustive
  `match` on `Status`.
- Extending a trait with default methods follows "prefer extending traits over modifying them".
  `remove` keeps its signature; only its (now shared) body changes.
- `liquers-core` stays minimal. HTTP concerns (`ValueDescription`, DTOs, formats, switches) live in
  `liquers-axum`.
- **Documentation touched in the implementation commit:**
  - `WEB_API_SPECIFICATION.md` §3.3 (409) and §5.0.1 and §5.1 (every endpoint above; `POST
    metadata` as a refusal; `GET /api/assets/remove` removed), with a `## History` row and a
    `reviewed:` bump.
  - `ASSETS.md` "Remove Semantics": rewritten with the decision table, plus a History row.
  - Status and resolution notes for `AXUM-ASSETS-API-ENDPOINTS-NOT-IMPLEMENTED` and
    `ASSET-REMOVE-FORGETS-DEPENDENTS`.
  - A progress note on `AXUM-HANDLER-TEST-COVERAGE`.

## Learning Points

Collected over Phases 1–4. Each one changed the design; together they are the reason for the
current shape.

1. **Keys and queries are two address spaces, and the API has to say which one each route
   takes.** `notes/a.txt` is a key to `parse_key` and an *action* to `parse_query`; `-R/…` is not
   key syntax at all. Routing every path through `parse_query` hid non-keyed assets entirely
   (found by the user at the Phase 4 gate) and made bare keys fail (found by the final review).
   → Two families, one parser each.
2. **The reference specification and code comments were not evidence.**
   `WEB_API_SPECIFICATION.md` §5 disagreed with the builders, and handler comments named
   `AssetManager` methods as missing that existed under other names. Every shape in this design
   was re-checked against code.
3. **`set_binary` trusts client metadata.** A client could store empty bytes (`status: Error`),
   make a `Source` unreadable (`Expired`), skip the write (`stored: false`), inject dependency
   edges, set an expiry, or relax validation (`is_error`). → The client sets a value plus five
   descriptive fields, the record is built fresh (allow-list), and metadata is manager-owned.
4. **Removal has two meanings.** Deleting a user value must cascade; dropping a recomputable value
   must keep the version, or the next dependency audit, or a restart through
   `dependency_blocks_fast_track`, cascades anyway. Today's `remove` did neither and forgot the
   dependents (`ASSET-REMOVE-FORGETS-DEPENDENTS`).
5. **Use the codebase's vocabulary.** "Evict" means in-memory map removal here, and the operation
   is `remove` in every layer. Proposed `delete`/`evict`/`remove_cached` names were dropped.
6. **"Describe" must not evaluate.** `get_asset_info` evaluated through `get`, existed in two
   bodies, and turned "not found" into a 500.
7. **Default trait methods cannot reach the manager's lock.** Hence the internal
   `KeyMutationAccess` supertrait. The lock is a non-reentrant `tokio::sync::Mutex`, so a
   default method holding it must never call a method that takes it (the lock-discipline list).
8. **An untested builder can be broken outright.** `build()` panics under axum 0.8 (the old
   `/*query` syntax), and no test existed to notice (`AXUM-HANDLER-TEST-COVERAGE`).
9. **Value types constrain the API's defaults.** `Text` cannot be stored as `md`, so `POST data`
   defaults to `Bytes`. An undeclared `data_format` decodes as `bin`, not by extension.
10. **Two code paths for one operation drift apart.** `to_override` ignored a live `Source` but
    promoted a stored one to `Override`.
11. **`get_asset` is not a lookup:** it creates and submits. `POST cancel` therefore started the
    evaluation it cancelled (`AXUM-ASSETS-CANCEL-STARTS-EVALUATION`), and any query-family
    "observe" operation inherits this unless a non-creating lookup exists (O2).
12. **Tests drafted without the code in view guessed wrong** (`String` vs `Option<String>`,
    by-value arguments, missing trait imports, a JSON body without `Content-Type`), and so did a
    crate-wide lint check (existing `unwrap()` calls). Verify drafts against the code before
    trusting them.
13. **A feature can be specified, routed and shipped without ever working.** The WebSocket
    subscribed but never forwarded, used the wrong client-message casing, and could not even be
    registered under axum 0.8. It had no test. Every notification path in this design gets an
    end-to-end test.
14. **Observing must never trigger.** A polling client that starts the computation it is
    polling for (`q/info` through `get_asset`, `GET metadata` today) cannot be used to watch
    without side effects. Request, submit and observe are distinct modes, and only the first two
    may evaluate.

## Open Questions

Decided at the Phase 4 gate (2026-09-28):
- **Q18:** two families, `/api/assets/q/` (queries, non-keyed) and `/api/assets/key/` (bare
  keys).
- **Q19:** `to_override` on a `Source` does nothing, in both paths.

- **Q23:** two ways to get an asset on both families: (a) request and wait (`data`, `entry`)
  and (b) `submit`, which returns `AssetInfo` at once. `info`, `metadata` and `version` observe
  without triggering.
- **Q24:** `DELETE` on a directory does not remove it; `key/removedir` does.
- **Q25:** GET alternatives for every operation without a body, as in the Store API
  (`with_destructive_gets()`).

Answered by the user, 2026-09-28:

| # | Decision |
|---|---|
| O1 | The unprefixed routes are **removed**; their handlers move to `/q/`. |
| O2 | Neither `key/info` nor `q/info` triggers evaluation. **`lookup_query_asset` is added.** `q/info` tells whether a (cached) non-keyed query has been evaluated and is available now. It is the polling path after `submit`, and it matters even with WebSockets, because notifications can be coalesced or missed. |
| O3 | Version is **never null**: `Version::unknown()` (all zeros) means unknown. Every status output, i.e. everything except `data`, `entry` and `recover`, is an `ApiResponse` (verified; see "Web Endpoints"). |
| O4 | **No** `q/expire`: expiring non-keyed assets is not supported for now. |
| O5 | WebSocket endpoints **`ws/q`** and **`ws/key`**. |
| O6 | Two parsers, two objects: `parse_query` → `Query` on `/q/` and `ws/q`; `parse_key` → `Key` on `/key/`, `ws/key` and `admin/audit/{*key}`. A `-R/…` path on a key route is a parse error. |
| O7 | Admin routes under the **`admin/`** prefix. |
| O8 | WebSocket messages stay consistent with the original notification names. The extra `Removed` is also added to `AssetNotificationMessage`, where it is meaningful; `Pong`, `UnsubscribedAll` and `Error` are protocol replies, not asset notifications. |
| O9 | Subscribing requests the asset and then observes it. Observing without requesting may be added later. |
| O10 | A subscription is tied to one asset. It ends with the asset's lifecycle (`Error`, `Cancelled`, `Expired`, `Removed`), and the client must request again. |
| O11 | A query timeout stays 500 (`ExecutionError`) with a message pointing to the assets API. |
| O12 | WebSocket limits: 64 KiB per client message, 256 subscriptions per socket, both configurable. |
| O13 | `removedir` is recursive, with `remove`'s per-status semantics for each key. |
| O14 | GET alternatives are off by default (`with_destructive_gets()`). |

No questions are open.

## Review Log

Multi-agent review, 2026-09-27.

- **Reviewer A (Phase 1 conformity):** no findings. Every in-scope endpoint has a route, a handler
  and a response spec; the deferred items are absent; Q1–Q17 are reflected; the removal table
  matches Phase 1.
- **Reviewer B (codebase alignment):** reported the not-yet-written implementation as "blocking",
  which is expected for a design, and an `unwrap` in `get_metadata_handler` that is actually
  `unwrap_or`. One valid advisory was taken: the module docs point at the old design folder
  (added to Integration Points).
- **Verified by hand**, because Reviewer B did not cover them:
  - *Exhaustive `ErrorType` sites.* A grep for `ErrorType::Cancelled` and
    `ErrorType::CacheNotSupported` outside constructors finds exactly: `assets.rs` ≈2239–2252;
    `api_core/error.rs` 28/79/110/136/322; `liquers-py/src/error.rs` both directions;
    `liquers-web/src/error.rs` both directions and `tests/objects_OBJECT.rs`. Nothing outside
    `.rs` files. The table above was corrected: core `error.rs` has no variant list.
  - *Lock discipline.*
    - `AssetRef::cancel` does not reach the manager lock; today's `remove` already calls it
      under the lock.
    - `remove_key_asset` is a plain map removal in both managers (`scc` / `std::sync::Mutex`).
    - `untrack_expiration` is a channel send.
    - `cascade_expire_dependents` → `expire_dependencies_result` → `expire_without_cascade` /
      `expire_stored_copy` takes no manager lock; `set_binary` already runs it under the lock.
    - The expiration monitor's `remove_expired_from_maps` does take the lock, but on its own
      task: it waits rather than deadlocks.

  No fixer pass was needed beyond these edits.

**Final cross-phase review, 2026-09-27** (changes made in this document):
- `{key}` in the endpoint table is `-R/<key>`: a bare `notes/a.txt` parses as action `notes`.
- WebSocket route `{}/*query` panics in axum 0.8; fixed in `build()` (`AXUM-ASSETS-WEBSOCKET-ROUTE-PANICS`).
- `get_asset_info` exists twice (trait default and `DefaultAssetManager` override); both change,
  and "not found" becomes `key_not_found` (was `general_error` → 500, contradicting the 404).
- `dependency_blocks_fast_track` accepts a stored `Recipe`, or the kept version is useless after a restart.
- `remove`: live `None`/`Recipe` defers to the stored status; stored `Recipe` is an idempotent no-op;
  legacy metadata is deleted rather than dropped. `expire`: the manager refuses non-expirable live
  statuses itself, so the error carries `key`.
- `or_previous` fills `type_identifier`/`data_format` only as a pair from a data-bearing previous.
- Corrected: an undeclared `data_format` decodes as `bin`, not the key's extension.
- Recorded the readers of the kept `Recipe` entry and the `cancel()`-under-lock stall.

**Revision after the Phase 4 gate, 2026-09-28** (user review): the addressing was split into
`/q/` and `/key/` (Q18), `to_override` on a `Source` became a no-op (Q19), and the Learning
Points and Open Questions sections were added. Routes, handlers, the endpoint table and the error
table were rewritten, and `key_cancel` and `q_info`/`q_version` were added. Phases 3 and 4 are
stale until this revision is approved.

**WebSocket review, 2026-09-28** (user request): findings W1–W6 were recorded and the "WebSocket
Notifications" section was added: forwarding, the route, spec-conformant messages, key
subscriptions, and `AssetNotificationMessage::Removed`. Open Questions O8–O10 were added. Hardening
and scope subscriptions stay with their existing issues.

**Scope extension, 2026-09-28** (Q22, the user accepted every recommended issue in one design):
added "Web API Known Issues in Scope" (I1–I10), extended the preflight, integration points and
open questions (O11, O12).

**Access modes, 2026-09-28** (user review):
- Q23 (request / submit / observe), Q24 (`removedir`), Q25 (GET alternatives).
- `q/info`, `q/metadata` and `q/version` now use `lookup_query_asset`, so observing never
  evaluates. I verified that `key/info` never evaluates, including the recipe-provider path.
- `metadata` moved to observe mode.
- Added `submit`, `removedir` (core and route) and the full route table.

**Answers to O1–O14, 2026-09-28:**
- The WebSocket moves to `ws/q` and `ws/key`, a subscription is tied to one asset and ends with
  its lifecycle, and `Removed` is added to the core notifications.
- The version is never null (zeros mean unknown), and every status output is an `ApiResponse`.
- Admin routes live under `admin/`, and `q/expire` is dropped.
- No questions remain open.

