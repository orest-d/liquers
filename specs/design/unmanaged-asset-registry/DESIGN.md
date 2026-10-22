---
id: UNMANAGED-ASSET-REGISTRY
kind: design
title: A server-wide registry that lets an unmanaged evaluation be followed by id
form: compact
status: in_review
phase: implementation
readiness: needs-decision
autofix: not-eligible
area: [axum, docs]
issues: [UNMANAGED-ASSET-NOT-FOLLOWABLE-ACROSS-CONNECTIONS]
affects_docs: [specs/reference/WEB_API_SPECIFICATION.md, specs/guides/WEB_API_GUIDE.md]
created: 2026-10-20
---
# A server-wide registry that lets an unmanaged evaluation be followed by id

Bulk design (`specs/guides/autonomous_bulk_design.md`): Phases 1-4 written, not approved, nothing
implemented. It builds the option (b) that `axum-query-named-arguments` Phase 1 Q10 analysed and
Q15 deferred "once sessions are supported"; Q1 below decides whether it may ship before them.

## Phase 1: High-Level Design

### Purpose

An evaluation with argument overrides (or a volatile, `cached: false`, non-retained one) is
**unmanaged**: nothing but the request that started it can see it. A client that must submit over
HTTP and poll, or that reconnects its WebSocket, has no way back to it. Give such evaluations a
server-issued id that any request can follow, with bounded retention, so long parameterised
queries (searches, reports) work over plain HTTP — VISION "Server / data service" and the agent
workbench, whose tool calls are short HTTP requests.

### Problem Example

At HEAD (`liquers-axum/src/assets/query_handlers.rs` `q_submit_handler`):

```text
POST /liquer/api/assets/q/submit/ns-search/search?query=liquers&limit=20
→ 200 {"result":{"status":"Submitted","managed":false,"note":"This evaluation is unmanaged: its
       status and result cannot be read later. Use q/data, or WebSocket subscribe with arguments, …"}}
GET  /liquer/api/assets/q/info/ns-search/search                 → 404 NotAvailable
```

The evaluation runs, but its status and result are unreachable; a WebSocket subscriber that drops
its connection loses its `subscription_id`. Should be (registry enabled): the submit answers
`"id": "5f0c…"`, `GET id/info/5f0c…` reports `Processing` then `Ready`, `GET id/data/5f0c…` waits
for and returns the result, and a new WebSocket connection can `{"action":"subscribe","id":"5f0c…"}`.

### Scope and Acceptance Criteria

- **AC-1** Submit issues an id
  WHEN the registry is enabled and `q/submit` (or `key/submit`) produces an unmanaged asset
  THEN the result carries `managed: false`, an `id` (UUID v4) and a note naming the `id/` routes
- **AC-2** Follow over HTTP
  WHEN `GET id/info/{id}`, `GET id/data/{id}` or `GET id/entry/{id}` is requested for a live id
  THEN `info` answers the current `AssetInfo` without evaluating, `data`/`entry` wait for the result and answer it as `q/data`/`q/entry` do (with `x-liquers-managed: false`), and the same evaluation is answered every time (a volatile result is not recomputed)
- **AC-3** Follow from any connection
  WHEN a WebSocket `subscribe` names `id` on any connection, or `cancel` names a registered id the connection does not hold
  THEN the subscription follows that asset (`Initial`, notifications, terminal end), and the cancel cancels it; `POST id/cancel/{id}` does the same over HTTP
- **AC-4** Retention
  WHEN an entry's asset has finished (`Status::is_finished`) and `retain_after_finish` has passed
  THEN the id is forgotten and every route answers it as unknown
- **AC-5** Capacity
  WHEN `capacity` entries are registered
  THEN the oldest finished entry is evicted for a new one; if none has finished, the new evaluation still runs and its answer has no `id` and a note "registry full"
- **AC-6** Unknown ids
  WHEN an id was never issued, has expired, or belongs to another owner (AC-8)
  THEN the answer is 404 `NotAvailable` "unknown or expired id" — the same in all three cases
- **AC-7** Disabled by default
  WHEN the builder did not enable the registry
  THEN every answer is as at HEAD and the `id/` routes do not exist
- **AC-8** Owner binding
  WHEN a `User` request extension was present when the id was issued
  THEN only a request carrying an equal `User` can use the id

Non-goals: ids for managed assets (their address names them — Q10 of the origin design);
persistence across restarts; listing ids; ids for `q/data` / Query API requests (the requester
already waits); authentication (an embedding layer inserts `User`, see `access-policy-at-entry-points`).

### Design Readiness

- **Readiness:** needs-decision
- **Automatic fixing:** not-eligible — new HTTP routes, a new builder option and a new WebSocket
  message field (rule 4).
- **Leading issue:** Q1, whether ids may be bearer capabilities before sessions exist.
- **Explanation:** every mechanism is inside `liquers-axum` and reuses the `q/` handlers' value
  paths; the open choices are security posture and defaults, which the plan carries as assumptions.
- **Open questions:**
  1. **Open design - ship before sessions:** opt-in now, ids as bearer capabilities with the owner
     binding of AC-8 active whenever an auth layer supplies `User` (recommended: the issue's own
     analysis says UUID v4 defeats guessing; opt-in keeps today's default), or wait for
     `access-policy-at-entry-points` (Q15's literal wording).
  2. **Proposed resolution - defaults:** `capacity = 1024`, `retain_after_finish = 10 min`.
  3. **Proposed resolution - routes:** an `id/` family at the Assets API base (`id/info`,
     `id/data`, `id/entry`, `id/cancel`), not query parameters on `q/` routes (an id names an
     evaluation, not a query).

### Design Dependencies

- `axum-query-named-arguments` (complete) — origin of the analysis (Q10 b, Q15); its connection-scoped
  ids keep working and become registry ids when the registry is on.
- `access-policy-at-entry-points` — **requires** (soft): AC-8 compares `User` values, which needs
  the derives of its Step 2; its `Principal` extractor replaces the bare extension read.
- `axum-websocket-connection-limits` — **overlaps**: same file (`websocket.rs`), disjoint functions.
- `assets-api-admin-operations` — **overlaps**: same builder; no order.

### Consolidated Findings

- Persists at HEAD: `SubmitResult` has no id (`common.rs`), `Session::cancel` / `unsubscribe_id`
  look only in the connection's own table (`websocket.rs`), and nothing retains an unmanaged
  `AssetRef` after the request.
- `User` has no derives at 18732dd; until `access-policy-at-entry-points` Step 2 lands, AC-8 is
  implemented as "owner must be absent" (ids issued under an auth layer are refused to everyone —
  safe) or Step 4 waits for it. The plan orders Step 4 last for this reason.
- Holding an `AssetRef` keeps its value in memory; capacity and retention are the only bound, so
  both are mandatory settings with defaults, not options.
- A watcher task per entry (await finish, sleep, remove) gives exact retention without a sweeper.

## Phase 2: Architecture

### Solution

A `UnmanagedRegistry<E>` (`Arc`, inside an `Extension` added by `AssetsApiBuilder::build` when
enabled) maps id → `{asset: AssetRef<E>, owner: Option<User>, finished_at: Option<Instant>,
inserted: Instant}` under a `std::sync::Mutex<HashMap<String, Entry<E>>>` (short, non-async critical sections).
`register` evicts per AC-5 and spawns a watcher: subscribe to the asset's notifications until
`status().is_finished()`, record `finished_at`, sleep `retain_after_finish`, remove the entry if it
is still the same asset. `q_submit_handler` / `key_submit_handler` register an unmanaged result.
The `id/` handlers look up (owner-checked) and reuse the `q/` value paths (`asset_bytes`,
`asset_entry`, `get_asset_info`, `cancel`). In `websocket.rs`, override and unmanaged
subscriptions register their asset and use the registry id as `subscription_id`; `subscribe` with
`id` builds a `Subscribed` from the registered asset; `cancel` falls back to the registry.

Rejected: a periodic sweeper (retention granularity, an extra task per router); keeping ids in
`AssetManager` (core has no notion of a request's caller, and managed assets need none); storing
results in the store (persistence is a non-goal and would make unmanaged results managed).

Known-issue preflight: none blocks; `CORE-SESSION-AND-KEY-ACL` is the soft requirement above.

### Changes

`liquers-axum/src/assets/registry.rs` (new):

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UnmanagedRegistryConfig { pub capacity: usize, pub retain_after_finish: Duration }
impl Default for UnmanagedRegistryConfig { … }          // 1024, 10 min (Q2)
pub(crate) struct UnmanagedRegistry<E: Environment> { … }
impl<E: Environment> UnmanagedRegistry<E> {
    pub(crate) fn register(self: &Arc<Self>, asset: AssetRef<E>, owner: Option<User>) -> Option<String>;
    pub(crate) fn get(&self, id: &str, owner: Option<&User>) -> Option<AssetRef<E>>;
}
```

`builder.rs`: `pub fn with_unmanaged_registry(self, config: UnmanagedRegistryConfig) -> Self`;
routes `GET {b}/id/info/{id}`, `GET {b}/id/data/{id}`, `GET {b}/id/entry/{id}`,
`POST {b}/id/cancel/{id}` (GET alternative with `with_destructive_gets()`), only when enabled.
`common.rs`: `SubmitResult` gains `#[serde(skip_serializing_if = "Option::is_none")] pub id:
Option<String>`; a second note text. `query_handlers.rs`, `key_handlers.rs`: submit handlers take
`Option<Extension<Arc<UnmanagedRegistry<E>>>>` and `Option<Extension<User>>`; new `id_handlers.rs`.
`websocket.rs`: `ClientMessage::Subscribe` gains `#[serde(default)] id: Option<String>` (exclusive
with `query`/`key`/`arguments`, else `ParameterError`); `Session` holds the optional registry.
Errors: unknown id → `Error::from_error(ErrorType::NotAvailable, "unknown or expired id")` (404,
as `common.rs` builds its `NotAvailable` answers); `id` with an
address → `ParameterError` (400 / WebSocket `Error`). Async handlers; the mutex is never held
across an `.await`.

Documents: `WEB_API_SPECIFICATION.md` §2.5 (the table's "workaround" column and the `q/submit`
paragraph), §5.3 `submit` row, new §5.9 "Following unmanaged evaluations", §5.6 builder table,
§5.8 `subscribe` with `id`; `WEB_API_GUIDE.md` §5 "Submitting what cannot be polled". History rows,
`reviewed:`.

### Risks

| Risk | Containment | Certainty |
|---|---|---|
| Memory held by retained results | capacity + retention defaults; opt-in | high |
| Leaked id read by another party | UUID v4; never logged; owner binding when `User` exists; Q1 | medium |
| Id cancels a shared evaluation | unmanaged assets are never shared, so only that evaluation stops | high |
| Watcher outlives the router | the watcher holds a `Weak` to the registry and ends when it is gone | high |
| WebSocket and HTTP ids diverge | one registry; WebSocket ids are registry ids when enabled | high |

Files: `registry.rs`, `id_handlers.rs` (new), `builder.rs`, `common.rs`, the two submit handlers,
`websocket.rs`, `mod.rs`. Existing tests unchanged (AC-7). Recovery: drop
`with_unmanaged_registry` from a deployment, or revert per step.

## Phase 3: Examples and Tests

### Examples

```rust
AssetsApiBuilder::<SimpleEnvironment<Value>>::new("/liquer/api/assets")
    .with_unmanaged_registry(UnmanagedRegistryConfig::default())
    .build()
```

```text
POST q/submit/ns-search/search?query=liquers&limit=20  → {"result":{…,"managed":false,"id":"5f0c…"}}
GET  id/info/5f0c…                                      → {"result":{"status":"Processing",…}}
GET  id/data/5f0c…                                      → 200 (waits) x-liquers-managed: false
ws/q: {"action":"subscribe","id":"5f0c…"}               → Initial … JobFinished (on any connection)
```

The query `ns-search/search` with overrides `query`, `limit` is the origin design's example; it
validates as `Action{ns-search/search}`.

### Tests

New `liquers-axum/tests/unmanaged_registry.rs` (fixture: `echo_args` and a slow `wait` command as
in `argument_overrides_http.rs`):

- `umr01_submit_with_overrides_returns_id` (AC-1): `id` present and a UUID; `key/submit` of a
  `cached: false` recipe key likewise.
- `umr02_id_info_and_data_follow_the_evaluation` (AC-2): poll `id/info` to `Ready`; `id/data`
  twice returns identical bytes; a volatile command's counter increments once.
- `umr03_ws_subscribe_by_id_from_new_connection` (AC-3): submit over HTTP, subscribe by id on a
  fresh connection, receive `Initial` and the terminal notification.
- `umr04_cancel_by_id_over_http_and_ws` (AC-3): `POST id/cancel` and a WebSocket `cancel` from a
  connection that does not hold it both end `Cancelled`.
- `umr05_retention_forgets_finished_ids` (AC-4): `retain_after_finish: 100 ms`; 404 after 500 ms.
- `umr06_capacity_evicts_oldest_finished_else_no_id` (AC-5): capacity 2.
- `umr07_unknown_expired_and_foreign_ids_answer_alike` (AC-6, AC-8): three 404 bodies equal; a
  test layer inserting `User` from a header makes the foreign case.
- `umr08_disabled_registry_changes_nothing` (AC-7): no `id`, `id/info/x` → axum 404.

Regression: `cargo test -p liquers-axum --lib --tests` (all `argument_overrides_*` tests unchanged).

## Phase 4: Implementation Plan

### Steps

- [ ] 1. `registry.rs` config, registry, watcher; unit tests for eviction and retention —
  `cargo test -p liquers-axum --lib registry`
- [ ] 2. `common.rs` `SubmitResult.id`; submit handlers register; `builder.rs` option and `id/`
  routes; `id_handlers.rs` — `umr01`, `umr02`, `umr05`, `umr06`, `umr08`
- [ ] 3. `websocket.rs`: registry ids as `subscription_id`, `subscribe` by `id`, registry
  fallback for `cancel`; `id/cancel` — `umr03`, `umr04`
- [ ] 4. Owner binding with `User` (after `access-policy-at-entry-points` Step 2, or "owner must be
  absent" until then) — `umr07`
- [ ] 5. Documents (§2.5, §5.3, §5.6, §5.8, new §5.9; guide §5), History rows, `reviewed:`; issue
  closed — `python3 scripts/docs_index.py --check`

Rollback: one commit per step; a deployment without `with_unmanaged_registry` is unaffected.

### Validation

`cargo test -p liquers-axum --lib --tests`; `cargo check -p liquers-axum --examples`;
`cargo clippy -p liquers-axum --lib`. No `cfg(feature)` or command change; `specs/index.csv`
regenerated.
