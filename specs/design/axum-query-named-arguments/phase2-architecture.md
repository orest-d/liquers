# Phase 2: Solution & Architecture — Argument overrides over HTTP and WebSocket

## Overview

One shared parser in `liquers-axum` turns a request's query string and body into an
`OverrideRequest` (overrides plus options). An empty one takes each route's existing path unchanged
(AC-7). A non-empty one becomes a `Recipe` with `arguments` / `links`, validated by building its
plan before anything runs (naming and conversion errors become HTTP errors, AC-4/6), and is
evaluated with a new **queued** variant of `AssetManager::apply`, which returns the unmanaged asset
without waiting, so `q/submit` can answer and the WebSocket can follow it. Core gains the type
conversion with warnings in `Recipe::to_plan` (AC-12), typed errors there, `apply_queued`, and
`Plan::last_action_command`.

Rejected:
- *Rewrite overrides into a canonical path query* — dropped at Phase 1.
- *Use `apply` as it is* — it runs to completion before returning (`AssetRef::run`,
  `assets.rs:3505`): `q/submit` would block and the WebSocket could not see progress.
- *Convert URL text in core* — core cannot tell text from a typed JSON value, so URL values would
  warn spuriously (AC-12 says they must not).
- *Override validation in axum* — it would duplicate `Recipe::to_plan`; building the plan once in
  the handler is the validation.

**Deviations from Phase 1's crate placement** (recorded in Phase 1, Scope Changes): core also gains
`AssetManager::apply_queued` and `Plan::last_action_command` (Phase 1 named only `recipes.rs`), and
`liquers-axum` gains a direct `uuid` dependency (`v4`; already in the lock file at 1.28) — Phase 1
said "no dependency change".

## Known-Issue Preflight

| Issue | Status | Priority | Impact on this design | Fix first? | Blocking? | Action |
|---|---|---|---|---|---|---|
| `AXUM-WEBSOCKET-HARDENING` | accepted | P3 | Override subscriptions count against `max_subscriptions` and share the bounded queue | No | No | One subscription table, one limit (§Concurrency) |
| `CORE-SESSION-AND-KEY-ACL` | accepted | P2 | No owner to check ids against — why ids are connection-scoped | No | No | Designed around (Q15) |
| `UNMANAGED-ASSET-NOT-FOLLOWABLE-ACROSS-CONNECTIONS` | draft | P3 | The deferred option (b) | No | No | None |
| `INLINE-DROP-REPAIR-STRANDS-EXISTING-WAITERS` | draft | P2 | Aborting a task drops an inline evaluation; unmanaged assets have no other waiter | No | No | Cancel by `AssetRef::cancel` when the asset is known; abort only before |
| `CANCEL-CAN-OVERTAKE-A-RETURNED-COMMAND` | draft | P3 | Cancel by id inherits best-effort cancellation | No | No | Documented as in §5.3 `q/cancel` |
| `DEFAULT-ASSET-MANAGER-RECIPE-OPT-SKIPS-PAYLOAD-CHECK` | draft | P2 | A `payload: required` command with overrides: `apply_queued` passes no payload | No | No | It fails during evaluation, as a `q/data` of it does today |
| `RECIPE-PLAN-ANALYSIS-RUNS-OUTSIDE-PLAN-BUILDING` | draft | P3 | `to_plan` changes nearby | No | No | Touch only the override loop |
| `LINK-IN-VARIADIC-DOES-NOT-EXPAND` | draft | P3 | `_link_<name>` on a variadic yields one element | No | No | Documented |
| `LIBRARY-CODE-USES-UNWRAP-AND-EXPECT` | draft | P2 | New code must not add any | No | No | None |
| `asset-cancellation-outcome` (design) | in_review | — | Defines what cancel reports | No | No | Reuse its outcomes |

## Interfaces

### `liquers-core`

```rust
// assets.rs — trait AssetManager<E> (#[async_trait]): a new provided method
/// As [`Self::apply`], but returns the ad-hoc asset without waiting for it: a queued manager
/// queues it, an inline manager evaluates it and returns it even when it failed (the failure is
/// recorded in the asset). No payload: the job queue runs `run(None)` (`assets.rs:9045`). Like
/// `apply`, not to be called from inside an evaluation (it takes a queue slot, `ASSETS-FIX1` #17).
async fn apply_queued(&self, recipe: Recipe, to: State<E::Value>) -> Result<AssetRef<E>, Error> {
    self.apply(recipe, to, None).await
}
// DefaultAssetManager: AssetData::new_ext(..).to_ref(); self.job_queue.submit(asset.clone()).await?;
//   Ok(asset) — status None is claimable (`try_claim_for_run`, :8913).
// ImmediateAssetManager: AssetData::new_ext(..).to_ref(); let _ = asset.run_inline(None).await;
//   Ok(asset) — the error is already in the asset (`finish_run_with_result`, :3429).

// plan.rs
impl Plan {
    /// Metadata of the command the query named for its last action — the alias for an alias
    /// (`ActionOrigin::Alias { command }`), whose own arguments are the overridable ones: an
    /// alias's head slots carry the target's names and are fixed by the alias definition.
    pub fn last_action_command(&self, cmr: &CommandMetadataRegistry) -> Option<CommandMetadata>;
}
```

`Recipe::to_plan` (`recipes.rs:294`) keeps its signature. It iterates `arguments` and `links` in
**sorted** order (deterministic warnings and errors), converts each value against the last
action's `ArgumentInfo` (private `fn convert_override(arg, value) -> Result<(Value, Option<String>), Error>`),
adds each note with `plan.init_warning`, and reports an unknown, injected or action-less name as
`ErrorType::ParameterError` (today `general_error`, HTTP 500). A name must be among
`last_action_command`'s arguments, which also refuses an alias's head slots (amended at the Phase 3
review, 2026-10-12: Phase 2 first said "the alias target").

| `ArgumentType` | As is | Converted, with a warning | `ConversionError` |
|---|---|---|---|
| `String` | string | number, bool (JSON text) | null, array, object |
| `Integer` | integral number | numeric string; integral float (`as_i64` rejects `3.0`) | null, other strings, fractional, bool |
| `IntegerOption` | as `Integer`, and null | as `Integer` | as `Integer` without null |
| `Float` / `FloatOption` | number (null for the option) | numeric string | null (plain), other strings, bool |
| `Boolean` | bool | string accepted by `ParameterValue::from_string` | other |
| `Enum` | value of the enum | alias string, expanded by `EnumArgument::expand_alias` (a query alias becomes a link override) | — |
| `GlobalEnum`, `Any`, `None` | anything (unchanged) | — | — |

A `multiple` argument converts element-wise (a scalar is one element). Pure CPU, sync.

### `liquers-axum`

```rust
// api_core/overrides.rs (new)
pub enum OverrideInput { Text(Vec<String>), Json(serde_json::Value) }   // URL occurrences | body member
#[derive(Clone, Copy, PartialEq, Eq)] pub enum RouteOptions { None, Entry }
pub struct OverrideRequest {
    pub values: BTreeMap<String, OverrideInput>,
    pub links: BTreeMap<String, String>,          // `_link_<name>`
    pub entry_format: Option<SerializationFormat>, // `_entry_format`, only with RouteOptions::Entry
}
impl OverrideRequest {
    /// AC-9/10/11 without the registry. A zero-length body is "no body" whatever its
    /// Content-Type; `{}` adds nothing. `format` is an ordinary name here (AC-22: on `q/entry` it is
    /// an override; elsewhere the route never calls this parser).
    pub fn parse(pairs: Vec<(String, String)>, body: &[u8], options: RouteOptions) -> Result<Self, Error>;
    /// No override (`values` and `links` empty); options are ignored. AC-7.
    pub fn is_empty(&self) -> bool;
    /// Builds the plan of the bare query (`PlanBuilder`), takes `last_action_command`, converts URL
    /// text with `ParameterValue::from_string(arg, s, &Position::unknown())` — an `EnumLink` result
    /// becomes a link, a `""` defaulting result takes the default — passes JSON through, then calls
    /// `recipe.to_plan(cmr)` once and discards the plan, so AC-4/6 fail before evaluation.
    pub fn to_recipe(&self, query: &Query, cmr: &CommandMetadataRegistry) -> Result<Recipe, Error>;
}

// api_core/format.rs: the option only; each caller keeps its own fallback (Accept for reads,
// Content-Type for request bodies — `key_handlers.rs:293`, `store/handlers.rs:526`).
pub fn entry_format_param(params: &HashMap<String, String>) -> Result<Option<SerializationFormat>, Error>;
// `format` present → ParameterError naming `_entry_format` (routes without overrides);
// an unknown `_entry_format` value → ParameterError (today an unknown `format` falls back silently).

// assets/common.rs
pub(crate) struct Requested<E: Environment> { pub asset: AssetRef<E>, pub overridden: bool }
pub(crate) async fn request_with_overrides<E: Environment>(
    env: &EnvRef<E>, query: &Query, overrides: &OverrideRequest) -> Result<Requested<E>, Error>;
/// `!overridden && lookup returns this asset (by AssetRef::id) && !asset.is_volatile().await`; the lookup
/// is `lookup_key_asset` for a pure-key query, else `lookup_query_asset`. Read when answering.
pub(crate) async fn is_managed<E: Environment>(env: &EnvRef<E>, query: &Query, r: &Requested<E>) -> bool;
pub const MANAGED_HEADER: &str = "x-liquers-managed";
#[derive(Serialize)] pub struct SubmitResult {
    #[serde(flatten)] pub info: AssetInfo,
    pub managed: bool,
    /// Set when unmanaged: "This evaluation is unmanaged: its status and result cannot be read
    /// later. Use q/data, or WebSocket subscribe with arguments, to get a result."
    #[serde(skip_serializing_if = "Option::is_none")] pub note: Option<String>,
}

// assets/websocket.rs
pub enum ClientMessage {
    Subscribe { query: Option<String>, key: Option<String>,
                #[serde(default)] arguments: Option<serde_json::Map<String, serde_json::Value>> },
    Unsubscribe { query: Option<String>, key: Option<String>,
                  #[serde(default)] subscription_id: Option<String> },
    Cancel { subscription_id: String },
    UnsubscribeAll, Ping,
}
pub struct Head { /* existing */ pub managed: bool,
    #[serde(skip_serializing_if = "Option::is_none")] pub subscription_id: Option<String> }
struct SubscriptionHandle<E: Environment> {
    task: JoinHandle<()>,
    asset: Arc<OnceLock<AssetRef<E>>>,        // set by the task once requested
    managed: Arc<OnceLock<bool>>,             // set by the task; an id is usable only when false
    cancel_requested: Arc<AtomicBool>,        // checked by the task right after setting `asset`
}
// Session: `subscriptions: HashMap<String /*uuid*/, SubscriptionHandle<E>>` and
// `by_address: HashMap<String, String>` (address → id) for subscriptions without overrides.
```

Every subscription gets a UUID v4 up front (`Head` shows it only when unmanaged). `arguments`
absent or `{}` is the address path, exactly as today (AC-7).

## Integration Points

- **`liquers-core`**: `assets.rs` — `apply_queued` (trait) and overrides in both managers beside
  `apply` (`:8305`, `:9590`); `plan.rs` — `last_action_command` beside `last_action_index`
  (`:2602`); `recipes.rs` — `Recipe::to_plan`.
- **`liquers-axum`**: `api_core/overrides.rs` (new), `api_core/format.rs`; `query/handlers.rs` —
  `serve_query` takes an `OverrideRequest`; empty → `env.evaluate` as today, else
  `request_with_overrides`; the POST TODO goes; `assets/common.rs` — `request_with_overrides`,
  `is_managed`, `asset_entry` error metadata; `assets/query_handlers.rs` — `data`, `entry`,
  `submit` (+ POST); `assets/builder.rs` — `POST q/data`, `POST q/entry` as read routes, always
  served (not subject to `read_only()`); `key_handlers.rs:293`, `store/handlers.rs:471,526`,
  `recipes/handlers.rs:166`, `assets/common.rs:218` — `_entry_format`; `assets/websocket.rs` —
  `ClientMessage` (and its test at `:633`, which needs `..`), `Head`, `Session`, `request`,
  `subscription_task`; `Cargo.toml` — `uuid = { version = "1", features = ["v4"] }`.

## Web Endpoints

| Route | Change | Answer |
|---|---|---|
| `GET/POST {base}{*query}` (Query API) | overrides; options: none | value + header |
| `GET/POST q/data/{*query}` | `POST` added; overrides | value + header |
| `GET/POST q/entry/{*query}` | `POST` added; overrides (`format` is one); `_entry_format`; failure carries metadata | `DataEntry` + header |
| `GET/POST q/submit/{*query}` | overrides | `SubmitResult` + header |
| `key/entry`, `key/recover`, `POST key/entry`, Store `entry` (GET, PUT), Recipes `entry` | `format` → `_entry_format`; `format` is 400 | unchanged |
| `ws/q` | `arguments` on `subscribe`; `subscription_id` on `unsubscribe`; `cancel` | `Head` gains `managed`, `subscription_id` |
| `ws/key` | `arguments` is a `ParameterError` (non-goal) | `Head` gains `managed` |

`x-liquers-managed: true|false` is on every response of the evaluating routes that concerns an
asset — success, or an evaluation error (AC-14) — and absent on errors raised before any asset
exists (parse, override validation). On `data` / `entry` it is read after the value is obtained;
on `submit` when answering.

## Concurrency

- **Subscribe with overrides**: the session allocates the id and handle, then spawns; the task runs
  `to_recipe` + `apply_queued`, sets `asset` and `managed = false`, then checks
  `cancel_requested` (and cancels if set), then follows. **Without overrides**: as today, plus
  `managed` from `is_managed` after the request; re-subscribing to an address replaces the previous
  subscription through `by_address`.
- **`cancel {id}` / `unsubscribe {id}`**: unknown id, or an id whose `managed` is not `false`, is
  one generic `ParameterError` ("unknown subscription id"), revealing nothing (AC-19). Cancel: if
  `asset` is set, `AssetRef::cancel`; otherwise set `cancel_requested` — the task cancels as soon as
  it has the asset (a queued job may already be running, so aborting would lose the cancel); on an
  inline manager still evaluating inside `apply_queued`, the task is also aborted.
- **Limits**: one table, so `max_subscriptions` counts every subscription (`AXUM-WEBSOCKET-HARDENING`).
- **Disconnect** aborts every task and cancels nothing, as today; on the queued manager an
  unmanaged asset then runs to completion and is dropped. On `ImmediateAssetManager` the evaluation
  runs inside the subscription task, so aborting it stops the evaluation — the inline exception.
- **AC-21**: a keyed recipe requested by key goes through `get` / `get_asset`, never
  `apply_queued`; the key-map lookup returns it, so `managed: true`. The `to_plan` change does not
  alter that path.
- **`managed` race**: an eviction between the request and the lookup reports `false` for an asset
  that was briefly managed — conservative, and noted in §7.1.

## Error Handling

| Case | Constructor | HTTP / WS |
|---|---|---|
| Body not JSON / not an object; name in query string and body; undefined `_` name; value and `_link_` for one name; `arguments` on `ws/key`; `format` or unknown `_entry_format` value on an entry route | `Error::from_error(ErrorType::ParameterError, msg)` | 400 / `Error` |
| Unknown, injected or action-less override name (`to_plan`) | `Error::from_error(ErrorType::ParameterError, msg).with_query(&q)` | 400 |
| Value not convertible | `Error::conversion_error_with_message(value, type, msg)` | 422 |
| `_link_` query does not parse | `parse_query` error (`ParseError`) | 400 |
| Unknown or managed id in `unsubscribe` / `cancel` | `ParameterError` "unknown subscription id" | `Error` |
| Evaluation failure on `q/entry` | the asset's error; `ErrorDetail.metadata = Some(metadata_json(&asset.get_metadata().await?))` | as today |

No `unwrap`/`expect`; matches over `Status`, `ClientMessage`, `ArgumentType`, `RouteOptions`
stay exhaustive.

## Relevant Commands

None added. Commands are reached through `ArgumentInfo` only; examples use `search`
(`ns-search/search`).

## Documentation Architecture

| Document | Kind / audience / area | Change |
|---|---|---|
| `specs/reference/ASSET_LIFECYCLE.md` | reference / internal / core/assets | Terms (argument override, managed / unmanaged asset, causes, recipe edge case); `apply_queued` |
| `specs/reference/WEB_API_SPECIFICATION.md` | reference / internal / axum, web | §2 overrides and options; §5.3 routes; §5.8 `arguments`, ids, `cancel`, `managed`; §7; a table "when an asset is managed" whose last column names the workaround — `q/data` to wait, WebSocket `subscribe` with `arguments` to follow |
| `specs/guides/WEB_API_GUIDE.md` | guide / both / axum, web | §4 overrides; §5 `submit` of an unmanaged asset (no result; side effects); §8 `arguments`; `_entry_format` |
| `specs/guides/ASSET_MANAGER_IMPLEMENTATION_GUIDE.md` | guide / internal / core/assets | `apply_queued`: what an implementation must return |
| `specs/reference/api/DOC_08_RECIPES_PLANS.md` | reference / internal / core/plan | `arguments` conversion, warnings, `ParameterError`, sorted order |
| `liquers-axum/README.md`, `examples/WEBSOCKET_EXAMPLE.md`, `examples/assets_recipes_basic.rs` | other | endpoint list, `_entry_format` |

`affects_docs` (set in `DESIGN.md`): the five `specs/` documents above. Each edit adds a
`## History` row and bumps `reviewed:`. `specs/README.md`: the existing line, "built" at Phase 5.

## Risks

| Assessment | Finding |
|---|---|
| Files likely to change | Integration Points: three in core, eleven in axum |
| Crates and workflows affected | `liquers-core`, `liquers-axum`; `liquers-py` / `liquers-web` neither match `to_plan` error types nor send `format` (checked) |
| Existing tests likely to change | every `?format=` test; `query_api_routes.rs` POST-body case; recipe tests asserting the old `general_error`; `websocket.rs:633` pattern; the Query API's empty path stays `env.evaluate` |
| New validation | Phase 3 suites per AC; `cargo test -p liquers-core --lib --tests`, `-p liquers-axum`, `-p liquers-lib --test registry_export` (no registry change expected) |
| Compatibility / data / concurrency / performance / security | `format` removed (A3); mistyped recipe arguments now convert with a warning; one extra plan build per overridden request; ids per connection — guessing reaches nothing; override values appear only in the asset's own log |
| Recovery | Independent parts: `apply_queued` additive; `to_plan` change local; `_entry_format` one helper |
| Certainty and open questions | High for HTTP; medium for WebSocket bookkeeping — Phase 3 tests cancel-before-asset on both managers. No open design question |
