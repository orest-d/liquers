# Phase 2: Solution & Architecture — Argument overrides over HTTP and WebSocket

## Overview

A request's overrides are collected once, by one shared parser in `liquers-axum`, into an
`OverrideRequest` (overrides plus options). An empty one takes each route's existing path
unchanged (AC-7). A non-empty one becomes a `Recipe` with `arguments` / `links`, validated by
building its plan before anything runs (so naming and conversion errors are HTTP errors, AC-4/6),
and evaluated with a new **queued** variant of `AssetManager::apply`, which returns the unmanaged
asset at once so that `q/submit` can answer and the WebSocket can follow it. Core gains the
type conversion with warnings in `Recipe::to_plan` (AC-12), typed errors there, and the queued
apply.

Rejected:
- *Rewrite overrides into a canonical path query* — dropped by the user at Phase 1 (identity would
  make the asset managed, but values without a text form could not travel).
- *Use `apply` as it is* — it runs the evaluation to completion before returning
  (`AssetRef::run`, `assets.rs:3505`), so `q/submit` would block and the WebSocket could not see
  progress.
- *Convert URL text in core* — core cannot tell text from a typed JSON value once both are
  `serde_json::Value`, so URL values would warn spuriously (AC-12 says they must not).
- *Override-specific validation in axum* — it would duplicate `Recipe::to_plan`; building the plan
  once in the handler is the validation, and the same function runs again inside the asset.

## Known-Issue Preflight

| Issue | Status | Priority | Impact on this design | Fix first? | Blocking? | Action |
|---|---|---|---|---|---|---|
| `AXUM-WEBSOCKET-HARDENING` | accepted | P3 | `max_subscriptions` and the bounded queue also bound override subscriptions | No | No | Count override subscriptions against the existing limit |
| `CORE-SESSION-AND-KEY-ACL` | accepted | P2 | No owner to check ids against — why ids are connection-scoped (Q15) | No | No | Designed around |
| `UNMANAGED-ASSET-NOT-FOLLOWABLE-ACROSS-CONNECTIONS` | draft | P3 | The deferred option (b) | No | No | None |
| `LINK-IN-VARIADIC-DOES-NOT-EXPAND` | draft | P3 | `_link_<name>` on a variadic yields one element, as a recipe link does | No | No | Documented in §7.1 |
| `ERROR-WITH-KEY-SETS-QUERY-FIELD` | draft | P2 | Error `query` field semantics unchanged here | No | No | None |
| `ASSETS-FIX1` | accepted | P2 | TODOs in `assets.rs`; none on the `apply` path | No | No | None |

## Interfaces

### `liquers-core`

```rust
// assets.rs — trait AssetManager<E>: a new provided method (extends, does not change, the trait)
/// As [`Self::apply`], but returns as soon as the ad-hoc asset is queued, so its progress can be
/// followed. The default evaluates before returning (correct for inline managers, whose
/// `get_asset` does the same).
async fn apply_queued(
    &self,
    recipe: Recipe,
    to: State<E::Value>,
    payload: Option<E::Payload>,
) -> Result<AssetRef<E>, Error> {
    self.apply(recipe, to, payload).await
}
// DefaultAssetManager overrides it: `AssetData::new_ext(..).to_ref()`, then
// `self.job_queue.submit(asset_ref.clone())`, then `Ok(asset_ref)` — the same queueing `get` uses.
// ImmediateAssetManager keeps the default.

// plan.rs
impl Plan {
    /// The metadata of the command the last action runs (the alias target for an alias).
    pub fn last_action_command(&self, cmr: &CommandMetadataRegistry) -> Option<CommandMetadata>;
}
```

`Recipe::to_plan` (`recipes.rs:294`) keeps its signature and changes behaviour:
- before each `override_value`, the value is converted against the last action's `ArgumentInfo`
  (private `fn convert_override(arg: &ArgumentInfo, value: &Value) -> Result<(Value, Option<String>), Error>`);
  a returned note becomes `plan.init_warning(..)`;
- an unknown, injected or action-less name is `ErrorType::ParameterError` (today
  `general_error`, which maps to HTTP 500).

| `ArgumentType` | Accepted as is | Converted, with a warning | `ConversionError` |
|---|---|---|---|
| `String` | string | number, bool (their JSON text) | null, array, object |
| `Integer` / `IntegerOption` | integral number (and null for the option) | numeric string; integral float | other strings, fractional, bool |
| `Float` / `FloatOption` | number (and null for the option) | numeric string | other strings, bool |
| `Boolean` | bool | string accepted by `ParameterValue::from_string` (`true`, `yes`, `1`, …) | other |
| `Enum`, `GlobalEnum`, `Any`, `None` | anything (unchanged, as today) | — | — |

A `multiple` argument converts element-wise (a scalar is one element). Sync: pure CPU.

### `liquers-axum`

```rust
// api_core/overrides.rs (new): one parser for every route
pub enum OverrideInput { Text(Vec<String>), Json(serde_json::Value) }   // URL occurrences | body member
pub struct OverrideRequest {
    pub values: BTreeMap<String, OverrideInput>,   // argument overrides
    pub links: BTreeMap<String, String>,           // from `_link_<name>`
    pub entry_format: Option<String>,              // `_entry_format`, where allowed
}
#[derive(Clone, Copy)] pub enum RouteOptions { None, Entry }   // which options a route accepts
impl OverrideRequest {
    /// AC-9, AC-10, AC-11 (duplicate, reserved, value-and-link) — no registry needed.
    pub fn parse(pairs: Vec<(String, String)>, body: Option<serde_json::Value>,
                 options: RouteOptions) -> Result<Self, Error>;
    pub fn is_empty(&self) -> bool;                // no override at all: AC-7
    /// URL text converted by `ParameterValue::from_string` (no warning); JSON passed through;
    /// then `recipe.to_plan(cmr)` is called and discarded, so AC-4/6 fail before evaluation.
    pub fn to_recipe(&self, query: &Query, cmr: &CommandMetadataRegistry) -> Result<Recipe, Error>;
}

// assets/common.rs
pub(crate) struct Requested<E: Environment> { pub asset: AssetRef<E>, pub managed: bool }
/// Empty overrides: the route's existing request (`get_asset`, or `can_make` + `get` for a key);
/// `managed` = the manager's lookup returns this same asset (`lookup_query_asset` /
/// `lookup_key_asset`, compared by `AssetRef::id`). Otherwise `apply_queued(recipe,
/// State::new(), None)`, `managed: false`.
pub(crate) async fn request_with_overrides<E: Environment>(
    env: &EnvRef<E>, query: &Query, overrides: &OverrideRequest) -> Result<Requested<E>, Error>;
/// `_entry_format`, then `Accept`, then CBOR; `format` present is a `ParameterError` (AC-22).
pub(crate) fn entry_format(params: &[(String, String)], headers: &HeaderMap)
    -> Result<SerializationFormat, Error>;
pub const MANAGED_HEADER: &str = "x-liquers-managed";

#[derive(Serialize)] pub struct SubmitResult { #[serde(flatten)] pub info: AssetInfo, pub managed: bool }

// assets/websocket.rs
pub enum ClientMessage {
    Subscribe { query: Option<String>, key: Option<String>,
                #[serde(default)] arguments: Option<serde_json::Map<String, serde_json::Value>> },
    Unsubscribe { query: Option<String>, key: Option<String>, #[serde(default)] id: Option<String> },
    Cancel { id: String },                                   // new
    UnsubscribeAll, Ping,
}
pub struct Head { /* existing fields */
    #[serde(skip_serializing_if = "Option::is_none")] pub id: Option<String>,   // unmanaged only
    pub managed: bool }
struct SubscriptionHandle<E: Environment> {
    task: JoinHandle<()>,
    asset: Arc<std::sync::OnceLock<AssetRef<E>>>,          // set by the task once requested
}
// Session: `subscriptions: HashMap<String, SubscriptionHandle<E>>` (by address, as today) and
// `by_id: HashMap<String, SubscriptionHandle<E>>` (override subscriptions); ids are UUID v4.
```

The Query API handlers keep their routes; both read `Query<Vec<(String, String)>>` (repeated keys
kept, in order) and, for POST, the body.

## Integration Points

- **`liquers-core`**: `assets.rs` — `AssetManager::apply_queued` (trait, provided) and its
  `DefaultAssetManager` override beside `apply` (`assets.rs:8305`); `plan.rs` —
  `Plan::last_action_command` beside `last_action_index` (`plan.rs:2602`); `recipes.rs` —
  `Recipe::to_plan` (`:294`) conversion, warnings and error types.
- **`liquers-axum`**: `api_core/overrides.rs` (new, re-exported from `api_core/mod.rs`);
  `api_core/format.rs` `select_format` callers move to `entry_format`; `query/handlers.rs` —
  `serve_query` takes an `OverrideRequest`, the POST TODO goes; `assets/common.rs` —
  `request_with_overrides`, `entry_format`, `asset_entry` error metadata; `assets/query_handlers.rs`
  — `q_get_data_handler`, `q_get_entry_handler`, `q_submit_handler` (+ POST forms);
  `assets/builder.rs` — `post(..)` on `q/data`, `q/entry`; `assets/key_handlers.rs:293`,
  `store/handlers.rs:471,526`, `recipes/handlers.rs:166` — `_entry_format`; `assets/websocket.rs`
  — `ClientMessage`, `Head`, `Session`, `request`, `subscription_task`; `Cargo.toml` — `uuid` (v4,
  already in the lock file).

## Web Endpoints

| Route | Change | Answer |
|---|---|---|
| `GET/POST {base}{*query}` (Query API) | overrides from query string and (POST) body; options: none | value (§2.3) + `x-liquers-managed` |
| `GET/POST q/data/{*query}` | `POST` added; overrides | value + header |
| `GET/POST q/entry/{*query}` | `POST` added; overrides; `_entry_format`; failure carries metadata (AC-14) | `DataEntry` + header |
| `GET/POST q/submit/{*query}` | overrides (POST body too) | `SubmitResult` (`AssetInfo` + `managed`) |
| `key/entry`, `key/recover`, `POST key/entry`, Store `entry` (GET, PUT), Recipes `entry` | `format` → `_entry_format`; `format` is 400 | unchanged |
| `ws/q`, `ws/key` | `arguments` on `subscribe` (`ws/q` only; on `ws/key` it is a `ParameterError`); `id` on `unsubscribe`; `cancel` | `Head` gains `managed`, and `id` when unmanaged |

## Concurrency

- An override subscription is registered under a fresh UUID before its task is spawned; the task
  sets the `OnceLock` after `apply_queued` returns (at once on the default manager). `cancel {id}`
  calls `AssetRef::cancel` on the set asset; if it is not yet set (an inline manager still
  evaluating inside `apply_queued`), the task is aborted, which drops the inline evaluation.
- A subscription without overrides is unchanged: keyed by address, replaced on re-subscribe; it
  receives an id (and an entry in `by_id`) only if its asset turns out unmanaged (volatile,
  `cached: false`, not retained) — reported in `Head`, so it can be cancelled by id too.
- Disconnecting aborts every task and cancels nothing, as today; an unmanaged asset then runs to
  completion and is dropped (no holder).
- `managed` is decided after the request; an eviction racing between `get_asset` and the lookup
  reports `false` for an asset that was briefly managed — harmless, and noted in §7.1.

## Error Handling

| Case | Constructor | HTTP / WS |
|---|---|---|
| Body not JSON / not an object; name in query string and body; `_` name not an option; value and `_link_` for one name; `arguments` on `ws/key` | `Error::from_error(ErrorType::ParameterError, msg)` | 400 / `Error` |
| Unknown, injected, or action-less override name (`Recipe::to_plan`) | `Error::from_error(ErrorType::ParameterError, msg).with_query(&q)` | 400 |
| Value not convertible | `Error::conversion_error_with_message(value, type, msg)` | 422 |
| `_link_` query does not parse | `parse_query` error (`ParseError`) | 400 |
| `format` on an entry route | `ParameterError` naming `_entry_format` | 400 |
| Unknown id in `unsubscribe` / `cancel` | `ParameterError` "unknown subscription id" (no hint of other connections) | `Error` |
| Evaluation failure on `q/entry` | the asset's error, `ErrorDetail.metadata` = `metadata_json(asset.get_metadata())` | as today |

No `unwrap`/`expect`; match statements over `Status`, `ClientMessage` and `ArgumentType` stay
exhaustive.

## Relevant Commands

None added. Commands are reached only through their argument metadata (`ArgumentInfo`); the
motivating namespace is `search` (`ns-search/search`), used in examples.

## Documentation Architecture

| Document | Kind / audience / area | Change |
|---|---|---|
| `specs/reference/ASSET_LIFECYCLE.md` | reference / internal / core/assets | Terms: argument override, managed / unmanaged asset (four causes, recipe edge case); `apply_queued` |
| `specs/reference/WEB_API_SPECIFICATION.md` | reference / internal / axum, web | §2: overrides and options (`_link_`, `_entry_format`, reserved `_`); §5.3 routes above; §5.8 `arguments`, ids, `cancel`, `managed`; §7 Query API; new table "when an asset is managed" with the workaround |
| `specs/guides/WEB_API_GUIDE.md` | guide / both / axum, web | §4 overrides; §5 `submit` of an unmanaged asset (no result, side effects); §8 subscribe with `arguments`; `_entry_format` |
| `specs/reference/api/DOC_08_RECIPES_PLANS.md` | reference / internal / core/plan | `arguments` conversion and warnings; `ParameterError` |
| `liquers-axum/README.md`, `examples/WEBSOCKET_EXAMPLE.md`, `examples/assets_recipes_basic.rs` | other | endpoint list, `_entry_format` |

Proposed `affects_docs`: the four `specs/` documents above. `specs/README.md`: the existing line,
moved to "built" at Phase 5. Each reference/guide edit adds a `## History` row and bumps `reviewed:`.

## Risks

| Assessment | Finding |
|---|---|
| Files likely to change | As in Integration Points: three in core, ten in axum |
| Crates and workflows affected | `liquers-core`, `liquers-axum`; `liquers-py` and `liquers-web` do not call `to_plan` errors by type nor send `format` (checked) |
| Existing tests likely to change | every `?format=` test (Store, Assets, Recipes routes); `query_api_routes.rs` POST-body case; recipe tests asserting the `general_error` text |
| New validation | Phase 3 suites for each AC; `cargo test -p liquers-core`, `-p liquers-axum`, `-p liquers-lib --test registry_export` (no registry change expected) |
| Compatibility / data / concurrency / performance / security | `format` removed (accepted, A3); recipes with mistyped arguments now succeed with a warning; one extra plan build per overridden request; ids are per connection — guessing reaches nothing; override values never logged by the server beyond the asset's own log |
| Recovery | Each part reverts independently: `apply_queued` is additive; the `to_plan` change is local; the `format` rename is one helper |
| Certainty and open questions | High for HTTP; medium for the WebSocket id bookkeeping (Phase 3 tests the cancel-before-set path). No open design question |
