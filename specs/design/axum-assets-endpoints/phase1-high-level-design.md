# Phase 1: High-Level Design - Assets API over the whole AssetManager

## Feature Name

Assets API endpoint completion (`AXUM-ASSETS-API-ENDPOINTS-NOT-IMPLEMENTED`, widened)

## Purpose

Six routes `AssetsApiBuilder` serves (`GET listdir`, `POST data|metadata|entry`,
`DELETE data|entry`) are stubs returning a bare `501`, although `WEB_API_SPECIFICATION.md` §5.1
documents them. Since that API was specified, `AssetManager` gained client-meaningful operations
the HTTP API cannot reach at all — override, expire, recovery reads, versions, dependency audit.
Give **every client-meaningful `AssetManager` operation** an endpoint, return the §3 envelope on
success and failure, and make §5 true at HEAD in the same change.

## Endpoint map

All paths are relative to the assets base path (`/api/assets`). ✅ exists · 🟡 stub today · 🆕 new.

**Scope principle.** This design exists to unblock the agent memory MVP
(`specs/design/agent-memory-mvp/`: a document corpus served through the assets API, `title` = L0,
`description` = L1, data = L2, derived entries kept fresh by recipes and versions, an
agent-writable notes area, an MCP adapter on top). The six **documented** endpoints are fixed
regardless — that is the P0 defect. Every **new or non-standard** endpoint must be either
*required* by the MVP or *useful* to it; anything else is deferred.

| Operation | Endpoint | Today | Agent memory MVP |
|---|---|---|---|
| `get_asset` → value / metadata / both | `GET data`, `GET metadata`, `GET entry` | ✅ | required — L2 read |
| `AssetRef::cancel` | `POST cancel` | ✅ | — (exists) |
| `listdir_asset_info` | `GET listdir` | 🟡 | **required** — the browse primitive: L0/L1 of a whole directory without reading data |
| `set_binary` | `POST data`, `POST entry` | 🟡 | useful — writing agent notes over HTTP (the MVP's own write path is an `ns-mem` command) |
| metadata-only write — **refused** (see *Metadata ownership*) | `POST metadata` | 🟡 | — |
| `remove(key)` — status-aware (see *Removal*) | `DELETE data`, `DELETE entry` | 🟡 | useful — deleting a note (`Source`) |
| `get_asset_info` — describe one key **without evaluating** | `GET info` | 🆕 | **useful, near-required** — L0/L1 of one entry; `GET metadata` goes through `get_asset` and can trigger evaluation (e.g. an LLM summary) just to read a title |
| `listdir_keys_deep` | `GET listdir?deep=true` | 🆕 | useful — one call for the key set of a subtree (corpus index, search over a subtree) |
| new defaulted `AssetManager::set_description(key, title, description)`, `Source` only | `POST description` | 🆕 | useful — L0/L1 of agent notes, edited after writing |
| new defaulted `AssetManager::expire(key)`, then cascade | `POST expire` | 🆕 | useful — force regeneration of a derived entry (a non-deterministic summary) without touching its source |
| `contains` | `GET contains` | 🆕 | useful — cheap existence check before writing a note (no evaluation, no body) |
| `version` | `GET version` | 🆕 | useful — an agent caching an L2 read can tell whether it changed, without re-reading it |
| `get_binary_any_status` — recovery read incl. `Expired` | `GET recover` (entry format) | 🆕 | useful — read the last known value of an expired derived entry while it regenerates |
| `to_override` — pin the current value | `POST override` | 🆕 | useful — keep a good generated summary instead of letting it regenerate |
| `makedir` | `PUT makedir` | 🆕 | useful — create a folder in the agent-writable area |
| `trigger_dependency_audit` / `…_all_registered` | `POST audit/{*query}` / `POST audit` | 🆕 | useful — after the corpus changed outside the server (git pull), expire what was derived from it |
| `refresh_command_versions_and_expire` | `POST refresh_command_versions` | 🆕 | useful — after `ns-mem` commands change, expire what they produced |

**Deferred** (recorded in `ASSETS-API-ADMIN-OPERATIONS`): `GET manager` (`eval_mode`,
`is_started` — a deployment detail, not an agent's concern), the guarded `remove_cached`, and the
Store API-style opt-in `GET remove` (dropped from spec §5.0.1 for assets).

**Not exposed — internal plumbing:** the `get_dependency_asset*`, `drain_dependencies`,
`wait_for_dependency`, `*_key_asset*`, `next_id_for_asset`, `get_envref`, `get_recipe_provider`,
`create_temporary_asset`, `start`, `track/untrack_expiration`, `remove_expired_from_maps`,
`cascade_expire_dependents`, `expire_dependencies_result`, `register_plan_dependencies`,
`audit_gaps`, the bare `refresh_command_versions`, the deprecated `apply_immediately`, `AssetRef::set_expiration_time` (bypasses the expiration
monitor — not a safe client operation) and `apply` (a query over a posted input belongs with the
Query API; out of scope).
**Already covered elsewhere:** `recipe_opt` (Recipes API); `is_volatile` and the expiration time
(fields of `GET info` / `GET metadata`). `set_state` needs a typed value, which HTTP cannot carry;
`POST entry` reaches the same result through `set_binary`.

## Metadata ownership (decided)

Metadata of an asset is **generated by the asset manager**; the API does not let a client set it.
`POST metadata` stays routed but answers with the §3 envelope, `NotSupported` (501), and
§5.1.5 is rewritten to say so.

`POST data|entry` sets a *value* (`Source` without a recipe, `Override` with one — decided by
`set_binary`, as today). The metadata sent with it is a **value description**, not a metadata
write: only the fields needed to interpret the bytes and describe a user-supplied value are
taken — `type_identifier`, `data_format`, `media_type`, `title`, `description`. Everything the
manager owns (`status`, `version`, `log`, `dependencies`, `expires`/`expiration_time`,
`is_volatile`, `stored`/`cached`, `error_data`, `progress`, `key`/`query`, `updated`,
`file_size`, …) is not taken from the client (Q10).

There are two legitimate reasons to write metadata; they want different mechanisms:

1. **In scope — describing a user-supplied asset.** `POST description` sets `title` and/or
   `description` of an asset whose status is `Source`; any other status is refused with
   `NotSupported`. Data and version are untouched. Backed by a new defaulted
   `AssetManager::set_description`, so a live cached asset and the stored record stay in step.
2. **Out of scope — asset managers talking to each other** (server ↔ browser). That is a remote
   store / remote asset manager over a separate, *trusted* system API carrying whole entries,
   versions, dependencies and expiration, cleanly split from the safe user API specified here.
   Filed as `NO-REMOTE-STORE-OR-ASSET-MANAGER`.

## Removal

**Naming.** The operation is `remove` everywhere — `AsyncStore::remove`, `AssetManager::remove` /
`remove_asset`, the Python `Store.remove` / `Cache.remove`, the JS store wrapper, and the Store
API's opt-in `GET remove` route (HTTP `DELETE` is the verb, not a name). This design keeps it.
"Evict" is not used: in code and in `ASSETS.md` it means dropping an asset from the manager's
*in-memory* map only, which is not what is meant here.

**What `remove` does today** (identical in `DefaultAssetManager` and the queued manager), for
every status: cancel and unmap the live asset → `DependencyManager::remove(key)`, which forgets
the key's version *and its dependents* → delete the stored value and metadata. The recipe lives
in the recipe provider and survives, so the next read is `Recipe` → recompute, or not-found
without a recipe. Dependents are never expired, and the forgotten edges mean a later recompute
cannot reach them (`ASSET-REMOVE-FORGETS-DEPENDENTS`).

**What it should do** — the right outcome depends on whether the value's identity changes:

| Status at the key | `remove` | Stored afterwards | Dependents |
|---|---|---|---|
| `Source` | value gone → `None` | nothing | cascade-expired |
| `Override` | override gone → `Recipe` | nothing | cascade-expired (the recipe will produce a different value) |
| recipe-computed (`Ready`, `Expired`, `Error`, `Cancelled`, `Volatile`) | value dropped → `Recipe` | **metadata with version kept**, data removed | not expired; if recomputation yields a different version, `register_version` cascades then |
| in-flight (`Submitted`, `Dependencies`, `Processing`, …) | cancelled, then as above | | |
| `Directory` | refused (409) — use the Store API's `removedir` | | |

Keeping the version is what makes "no cascade" true rather than deferred: `AssetManager::version`
reads stored *metadata*, so `trigger_dependency_audit` keeps dependents valid — the property
`version`'s documentation already promises ("delete large intermediates and keep the results
derived from them"). The decision is taken inside core under the key's mutation lock, so it cannot
race a concurrent `POST data`.

A guarded variant that refuses user-supplied values (`remove_cached`) is deferred: not needed by
the MVP.

## Core Interactions

- **Query:** the path is parsed with `parse_query`. Every operation except the reads is key-only.
  A query that is not a pure key is refused with the §3 envelope (`NotSupported`, 501): a specified
  refusal, not a stub. A key is therefore addressed as `-R/<key>` (`POST data/-R/notes/a.txt`): a bare
  `notes/a.txt` parses as the action `notes` with a filename, not as a key.
- **Store / Asset:** no store code. Everything goes through `AssetManager`, which already owns
  locking, status rules (`Source`/`Override`), versioning and cascades. `liquers-core`: new
  `expire`, `set_description`; `remove` semantics fixed.
- **Commands / Value types / UI:** none. `command_registry.yaml` is unaffected.
- **Web:** the handlers and builder change in `liquers-axum/src/assets/`. The first real handler
  tests drive the built `Router` with `tower::ServiceExt::oneshot` against an in-memory
  environment, which covers part of `AXUM-HANDLER-TEST-COVERAGE`.

## Crate Placement

- `liquers-axum`: handlers, builder, tests.
- `liquers-core`: `AssetManager::{expire, set_description}`, the fixed `remove`,
  their tests; `ASSETS.md` "Remove Semantics" rewritten.
- Specs: `WEB_API_SPECIFICATION.md` §5 plus a `## History` row, the issue status, and
  `ASSETS.md` if a trait method is added.

## Decisions and Open Questions

Decided (2026-09-27): **Q2** builder switches (`.read_only()` turning off every
state-changing route except `cancel`; `.with_admin(bool)` for `audit` and
`refresh_command_versions`, which act on the whole manager rather than one key) as a stop-gap until `CORE-SESSION-AND-KEY-ACL` delivers real access control.
**Q3** no metadata writes (above). **Q4** add `AssetManager::expire(key)`. **Q5**
`set_expiration_time` stays out, no issue. **Q6** `apply` out of scope, no feature.
**Q11** `POST data|entry` onto a key with a recipe is allowed and makes it `Override`.
**Q12** `POST description` (Source only) is in scope; remote/trusted API filed as
`NO-REMOTE-STORE-OR-ASSET-MANAGER`.
**Q1** `GET listdir` returns full `AssetInfo` records. **Q7** recovery read is a separate route,
`GET recover`. **Q8** POSTs answer 201; removals report `new_status`. **Q9/Q16** `remove` keeps its name and gets
status-aware semantics (above); no `remove_cached` for now; fixes
`ASSET-REMOVE-FORGETS-DEPENDENTS`. **Q13** removing a recipe-computed value drops memory and
stored data, keeping the metadata/version. **Q14** state-conflict refusals (`remove` on a
`Directory`, `POST description` on a non-`Source`) answer 409 (new `ErrorType`). **Q15**
superseded by the scope principle: no `GET remove`. **Q17** scope = documented endpoints +
what the agent memory MVP requires or can use (table above); `contains`, `version`, `recover`,
`override`, `makedir`, `audit`, `refresh_command_versions` kept in scope at the user's request.
**Q10** a POSTed entry contributes only the five descriptive fields; the handler builds a fresh
`MetadataRecord` from them and drops everything else, naming the dropped fields in `message`.
Why the split matters — trusted by `set_binary` / `try_fast_track` today: `status: Error` stores
*empty bytes* and still reports success; `status: Expired` is kept, so a `Source` can never be
read again; `stored: false` skips the store write; `dependencies` are loaded into the dependency
manager on the next read (fake edges); `expiration_time` is adopted by the live asset;
`is_error: true` relaxes type validation.

Still open: none — Phase 1 is ready for approval.

## References

- `specs/issues/CORE-SESSION-AND-KEY-ACL.md` (real access control),
  `specs/issues/NO-REMOTE-STORE-OR-ASSET-MANAGER.md` (trusted system API)

- `specs/issues/AXUM-ASSETS-API-ENDPOINTS-NOT-IMPLEMENTED.md`, `AXUM-HANDLER-TEST-COVERAGE.md`
- `specs/reference/WEB_API_SPECIFICATION.md` §3, §5; `specs/design/axum-assets-recipes-api/`
- `liquers-core/src/assets.rs`: the `AssetManager` trait, `AssetRef::{expire, to_override}`

## Review Log

- **Final cross-phase review, 2026-09-27:** added the `-R/<key>` addressing note under *Core
  Interactions*; the rest of the review's changes are in Phases 2–4.
