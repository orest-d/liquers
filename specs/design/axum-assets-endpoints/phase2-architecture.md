# Phase 2: Solution & Architecture - Assets API over the whole AssetManager

## Overview

Two layers change. **`liquers-core`**: `AssetManager::remove` becomes a status-aware default method
(delete a user-supplied value and cascade; drop a recomputed value and keep its version), and two
new default methods are added: `expire(key)` and `set_description(key, …)`. Refusals that depend
on an asset's status get a new `ErrorType::StatusConflict` (HTTP 409). **`liquers-axum`**: the six
stubbed handlers are implemented, eleven routes are added, `AssetsApiBuilder` gains `read_only()` and
`with_admin(bool)`, and the crate gets its first handler tests, which drive the built `Router`
in-process. There are no new commands and no new value types.

## Known-Issue Preflight

| Issue | Status | Priority | Relevance | Blocking? | Action |
|---|---|---|---|---|---|
| `AXUM-ASSETS-API-ENDPOINTS-NOT-IMPLEMENTED` | draft | P0 | the defect this design fixes | — | close on merge |
| `ASSET-REMOVE-FORGETS-DEPENDENTS` | draft | P2 | fixed by the new `remove` | no | close on merge |
| `AXUM-HANDLER-TEST-COVERAGE` | accepted | P2 | this design adds the scaffold for the assets handlers | no | note partial progress; the Store, Query and Recipes APIs remain |
| `TEXT-VALUE-CANNOT-BE-STORED-AS-MARKDOWN` | draft (filed now) | P2 | `POST data` defaults to `Bytes` because of it | no | monitor; the default can become `Text` once it is fixed |
| `CORE-SESSION-AND-KEY-ACL` | accepted | P2 | real access control; the builder switches are a stop-gap until it lands | no | monitor |
| `STORE-NO-READ-ONLY-ADAPTER` | draft | P2 | a corpus mounted from a file store is writable through `POST`/`DELETE` unless the router is `read_only()` | no | document `read_only()` as the mitigation |
| `QUEUED-MANAGER-EVICTION-RACE` | accepted | P2 | `remove` unmaps the live asset under the same lock as today | no | unchanged |
| `WEB-API-SPECIFICATION-DIVERGES-FROM-IMPLEMENTATION` | draft | P1 | §5 is rewritten here; the Store API parts stay with that issue | no | touch only §5 and §3.3 |
| `ASSETS-API-ADMIN-OPERATIONS` | draft | P3 | deferred endpoints | no | none |
| `DESCRIBING-AN-ASSET-CAN-TRIGGER-ITS-EVALUATION` | draft | P1 | the `get_asset_info` change fixes exactly this (both bodies) | no | close on merge, or hand to `store-and-asset-search` if it lands first (final review) |
| `AXUM-ASSETS-WEBSOCKET-ROUTE-PANICS` | draft (filed in final review) | P1 | `build()` panics with the default WebSocket path | **yes, for every router test** | fixed in `builder.rs`; close on merge |

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
#[derive(Serialize)] pub struct VersionResult { pub version: Option<Version> } // hex string or null
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
/// Parse the path as a query and require it to be a pure key.
/// Errors → Err(Response): ParseError (400), or NotSupported (501) naming the query.
fn key_from_path(path: &str, operation: &str) -> Result<Key, Response>;
fn error_response(e: &Error, message: &str) -> Response;          // ApiResponse::error(error_to_detail(e), …)
fn created<T: Serialize>(result: T, message: impl Into<String>) -> Response; // 201
async fn status_after_remove<E: Environment>(am: &E::AssetManager, key: &Key) -> Status; // Recipe | None
```

### Handlers — new or changed

```rust
// Filled-in stubs
pub async fn listdir_handler<E>(State<EnvRef<E>>, Path<String>, AxumQuery<HashMap<String,String>>) -> Response;
pub async fn listdir_root_handler<E>(State<EnvRef<E>>, AxumQuery<HashMap<String,String>>) -> Response;
pub async fn post_data_handler<E>(State<EnvRef<E>>, Path<String>, AxumQuery<HashMap<String,String>>, Bytes) -> Response;
pub async fn post_entry_handler<E>(State<EnvRef<E>>, Path<String>, HeaderMap, AxumQuery<HashMap<String,String>>, Bytes) -> Response;
pub async fn post_metadata_handler<E>(State<EnvRef<E>>, Path<String>) -> Response;   // specified refusal
pub async fn delete_data_handler<E>(State<EnvRef<E>>, Path<String>) -> Response;
pub async fn delete_entry_handler<E>(State<EnvRef<E>>, Path<String>) -> Response;     // delegates to delete_data
// New
pub async fn info_handler<E>(State<EnvRef<E>>, Path<String>) -> Response;
pub async fn contains_handler<E>(State<EnvRef<E>>, Path<String>) -> Response;
pub async fn version_handler<E>(State<EnvRef<E>>, Path<String>) -> Response;
pub async fn recover_handler<E>(State<EnvRef<E>>, Path<String>, HeaderMap, AxumQuery<HashMap<String,String>>) -> Response;
pub async fn override_handler<E>(State<EnvRef<E>>, Path<String>) -> Response;
pub async fn expire_handler<E>(State<EnvRef<E>>, Path<String>) -> Response;
pub async fn makedir_handler<E>(State<EnvRef<E>>, Path<String>) -> Response;
pub async fn description_handler<E>(State<EnvRef<E>>, Path<String>, Json<DescriptionRequest>) -> Response;
pub async fn audit_handler<E>(State<EnvRef<E>>, Path<String>) -> Response;
pub async fn audit_all_handler<E>(State<EnvRef<E>>) -> Response;
pub async fn refresh_command_versions_handler<E>(State<EnvRef<E>>) -> Response;
```

The existing `get_entry_handler` ignores the `Accept` header: it passes an empty `HeaderMap` to
`select_format`. `recover_handler` takes the real headers. `get_entry_handler` is corrected to do
the same, since the change is one line and the spec already promises `Accept` negotiation.

### `liquers-axum/src/assets/builder.rs`

```rust
impl<E: Environment> AssetsApiBuilder<E> {
    /// Omit every state-changing route except `POST cancel`.
    pub fn read_only(mut self) -> Self;
    /// Include (default) or omit the manager-wide routes: `POST audit`, `POST refresh_command_versions`.
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

## Integration Points

| Crate | File | Change |
|---|---|---|
| liquers-core | `src/error.rs` | `ErrorType::StatusConflict`, `Error::status_conflict` |
| liquers-core | `src/assets.rs` | `KeyMutationAccess` plus two impls; default `remove`, delete both per-manager `remove` bodies; `expire`, `set_description`; `get_asset_info` live branch; `AssetRef::set_description_fields`; `mark_expired_status` refusals → `StatusConflict`; `get_asset_info` not-found → `key_not_found` (trait default and the `DefaultAssetManager` override); `dependency_blocks_fast_track` store branch accepts `Recipe`; the `ErrorType` match at ≈2251. Drive-by: the orphaned doc paragraph above `expire_stored_copy` moves back to `DependencyManagerAccess`, whose doc it is |
| liquers-axum | `src/assets/value_description.rs` | new |
| liquers-axum | `src/assets/handlers.rs` | helpers, 7 filled-in handlers, 11 new handlers |
| liquers-axum | `src/assets/builder.rs` | new routes, `read_only`, `with_admin`; WebSocket route `{*query}` (panics today) |
| liquers-axum | `src/assets/mod.rs` | `mod value_description;` |
| liquers-axum | `src/assets/handlers.rs`, `builder.rs` module docs | point at `specs/design/axum-assets-endpoints/` as well as the original `axum-assets-recipes-api` |
| liquers-axum | `src/api_core/error.rs` | 409 mapping, parse, variant lists |
| liquers-axum | `src/assets/tests.rs` | placeholder replaced by handler tests (Phase 3) |
| liquers-axum | `Cargo.toml` | `[dev-dependencies] tower = { version = "0.5.3", features = ["util"] }` for `ServiceExt::oneshot` |
| liquers-py | `src/error.rs` | `ErrorType` variant plus both conversions |
| liquers-web | `src/error.rs`, `tests/objects_OBJECT.rs` | variant string and list |

### Routes (relative to `base_path`)

| Method and path | Handler | Disabled by |
|---|---|---|
| `GET listdir`, `GET listdir/{*query}` (`?deep=true`) | `listdir_root_handler`, `listdir_handler` | — |
| `GET info/{*query}` | `info_handler` | — |
| `GET contains/{*query}` | `contains_handler` | — |
| `GET version/{*query}` | `version_handler` | — |
| `GET recover/{*query}` (`?format=`) | `recover_handler` | — |
| `POST data/{*query}`, `POST entry/{*query}` | `post_data_handler`, `post_entry_handler` | `read_only` |
| `POST metadata/{*query}` | `post_metadata_handler` (always refuses, 501) | — |
| `DELETE data/{*query}`, `DELETE entry/{*query}` | `delete_*_handler` | `read_only` |
| `POST description/{*query}` | `description_handler` | `read_only` |
| `POST expire/{*query}` | `expire_handler` | `read_only` |
| `POST override/{*query}` | `override_handler` | `read_only` |
| `PUT makedir/{*query}` | `makedir_handler` | `read_only` |
| `POST audit`, `POST audit/{*query}` | `audit_all_handler`, `audit_handler` | `read_only`, `with_admin(false)` |
| `POST refresh_command_versions` | `refresh_command_versions_handler` | `read_only`, `with_admin(false)` |
| `GET data|metadata|entry/{*query}`, `POST cancel/{*query}` | unchanged | — |

## Relevant Commands

### New Commands

None. `specs/command_registry.yaml` is unaffected.

### Relevant Existing Namespaces

None is touched. The client of these endpoints is the planned `ns-mem` namespace and the MCP
adapter of `agent-memory-mvp`. That design writes through commands via `Context`, so the only
contract that matters here is the `AssetManager` behaviour above: `remove`, `expire` and
`set_description` are equally reachable from a command through `Context::get_asset_manager`.

## Web Endpoints

Envelope: §3 `ApiResponse` for everything except the byte-returning reads (`GET data`, and
`GET entry` and `GET recover` in their negotiated format). `query` is set on every response.

**`{key}` below means a pure-key query, written `-R/<key>`** (e.g. `POST data/-R/notes/a.txt`).
The path goes through `parse_query`, and a bare `notes/a.txt` parses as the *action* `notes` with
filename `a.txt` (checked with `liquers-validate`), so it is refused with 501 like any other
non-key query. This matches §5's existing `-R/…` examples. Accepting bare keys is an open question
for the user (final review), not part of this design.

| Endpoint | Success | Body `result` | Errors |
|---|---|---|---|
| `GET listdir[/{key}]` | 200 | `{assets: [AssetInfo…]}`; with `deep=true`, `{keys: ["a/b.md", …]}` | non-key 501; store error |
| `GET info/{key}` | 200 | `AssetInfo` | 404 when absent |
| `GET contains/{key}` | 200 | `{contains: bool}` | — |
| `GET version/{key}` | 200 | `{version: "…32 hex…" \| null}` | store read error 500 (not `null`: `Ok(None)` and `Err` stay distinct) |
| `GET recover/{key}` | 200 | `DataEntry` (CBOR by default; `?format=`, or `Accept`) | 404 when there is no data-bearing state |
| `POST data/{key}` | **201** | `AssetInfo` after the write; `message` names ignored parameters | 400 unknown type or bad format; 501 non-key |
| `POST entry/{key}` | **201** | as above; `message` names the dropped metadata fields | 400 undecodable body or non-object metadata |
| `POST metadata/{key}` | — | — | always 501 `NotSupported`: "asset metadata is owned by the asset manager; use POST description for a Source asset's title and description" |
| `DELETE data|entry/{key}` | 200 | `{removed: true, new_status: "Recipe" \| "None"}` | 404; 409 for a directory |
| `POST description/{key}` | 200 | `AssetInfo` after the change | 400 when both fields are absent; 404; 409 when not `Source` |
| `POST expire/{key}` | 200 | `AssetInfo` after the change | 404; 409 when the status cannot expire |
| `POST override/{key}` | 200 | `AssetInfo` after the change | 404 when there is no data (`to_override`'s `key_not_found`) |
| `PUT makedir/{key}` | **201** | `AssetInfo` of the directory | store error |
| `POST audit[/{query}]` | 200 | `{checked: [...], expired: [...]}` | — (a non-key query gives an empty report, as core does) |
| `POST refresh_command_versions` | 200 | `null`, with a message | registry error 500 |

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
| path does not parse | `ParseError` (from `parse_query`) | 400 |
| a key-only operation given a non-key query | `NotSupported`, with the query | 501 |
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
