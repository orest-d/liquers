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

| Operation | Endpoint | Status |
|---|---|---|
| `get_asset` → value / metadata / both | `GET data`, `GET metadata`, `GET entry` | ✅ |
| `AssetRef::cancel` | `POST cancel` | ✅ |
| `listdir_asset_info` | `GET listdir` (`?deep=true` → `listdir_keys_deep`) | 🟡 |
| `set_binary` | `POST data`, `POST entry` | 🟡 |
| metadata-only write (no method yet, Q3) | `POST metadata` | 🟡 |
| `remove` | `DELETE data`, `DELETE entry` (+ opt-in `GET remove`, spec §5.0.1) | 🟡 |
| `get_asset_info` — describe **without evaluating** | `GET info` | 🆕 |
| `contains` | `GET contains` | 🆕 |
| `version` | `GET version` | 🆕 |
| `get_binary_any_status` — recovery read incl. `Expired` | `GET data|entry?any_status=true` | 🆕 |
| `to_override` — pin the current value | `POST override` | 🆕 |
| expire a key and cascade (only `AssetRef::expire` today, Q4) | `POST expire` | 🆕 |
| `makedir` | `PUT makedir` | 🆕 |
| `trigger_dependency_audit` / `…_all_registered` | `POST audit/{*query}` / `POST audit` | 🆕 |
| `refresh_command_versions_and_expire` | `POST refresh_command_versions` | 🆕 |
| `eval_mode`, `is_started` | `GET manager` | 🆕 |

**Not exposed — internal plumbing:** the `get_dependency_asset*`, `drain_dependencies`,
`wait_for_dependency`, `*_key_asset*`, `next_id_for_asset`, `get_envref`, `get_recipe_provider`,
`create_temporary_asset`, `start`, `track/untrack_expiration`, `remove_expired_from_maps`,
`cascade_expire_dependents`, `expire_dependencies_result`, `register_plan_dependencies`,
`audit_gaps`, the bare `refresh_command_versions`, and the deprecated `apply_immediately`.
**Already covered elsewhere:** `recipe_opt` (Recipes API); `is_volatile` and the expiration time
(fields of `GET info` / `GET metadata`). `set_state` needs a typed value, which HTTP cannot carry;
`POST entry` reaches the same result through `set_binary`.

## Core Interactions

- **Query:** the path is parsed with `parse_query`. Every operation except the reads is key-only.
  A query that is not a pure key is refused with the §3 envelope (`NotSupported`, 501): a specified
  refusal, not a stub.
- **Store / Asset:** no store code. Everything goes through `AssetManager`, which already owns
  locking, status rules (`Source`/`Override`), versioning and cascades. Possibly two defaulted
  trait methods (Q3, Q4) in `liquers-core`.
- **Commands / Value types / UI:** none. `command_registry.yaml` is unaffected.
- **Web:** the handlers and builder change in `liquers-axum/src/assets/`. The first real handler
  tests drive the built `Router` with `tower::ServiceExt::oneshot` against an in-memory
  environment, which covers part of `AXUM-HANDLER-TEST-COVERAGE`.

## Crate Placement

- `liquers-axum`: handlers, builder, tests.
- `liquers-core`: only the defaulted methods from Q3 and Q4.
- Specs: `WEB_API_SPECIFICATION.md` §5 plus a `## History` row, the issue status, and
  `ASSETS.md` if a trait method is added.

## Open Questions (for discussion)

1. **`GET listdir` shape:** return full `AssetInfo` records (`{assets:[AssetInfo…]}`) rather than
   the spec's `{key,status}`? *Lean yes.*
2. **Access control:** writes, deletes and admin operations (audit, refresh, override, expire)
   change shared state. Gate them behind builder switches (`.read_only()`, `.with_admin(bool)`),
   and put the GET-based `remove` behind an explicit opt-in as the Store API does? *Lean yes: the
   default enables everything except `GET remove`.*
3. **`POST metadata`:** add a defaulted `AssetManager::set_metadata(key, record)` that re-reads
   the stored bytes and calls `set_binary`, so status, version and cascade rules all apply? Or
   refuse it and change the spec? *Lean: add the method.*
4. **`POST expire`:** add a defaulted `AssetManager::expire(key)`? It would call `AssetRef::expire`
   for a live asset. For a stored-only `Ready`/`Override` asset it would rewrite the stored status
   to `Expired`, then `cascade_expire_dependents`. `Source` errors, as `AssetRef::expire` already
   does. *Lean yes.*
5. **Setting expiration** (`AssetRef::set_expiration_time`) deliberately skips the monitor, so it
   is not a safe client operation. Leave it out and file a feature issue?
6. **`apply`** (run a query on a posted input state) is a different kind of operation — closer to
   the Query API. Out of scope and filed as a feature?
7. **Recovery read:** a query parameter on `GET data|entry`, or a separate `GET recover` route?
   *Lean: the parameter.*
8. **POST success code:** the spec says 201, but `ApiResponse::ok` always returns 200. Add a 201
   path? **DELETE:** return `new_status` (`Recipe` when a recipe exists, else `None`), at the cost
   of one `recipe_opt` call?
9. `remove` does not expire dependents. Out of scope and filed as an issue?

## References

- `specs/issues/AXUM-ASSETS-API-ENDPOINTS-NOT-IMPLEMENTED.md`, `AXUM-HANDLER-TEST-COVERAGE.md`
- `specs/reference/WEB_API_SPECIFICATION.md` §3, §5; `specs/design/axum-assets-recipes-api/`
- `liquers-core/src/assets.rs`: the `AssetManager` trait, `AssetRef::{expire, to_override}`
