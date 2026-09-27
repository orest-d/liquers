# Phase 1: High-Level Design - Complete the Assets API Endpoints

## Feature Name

Assets API endpoint completion (`AXUM-ASSETS-API-ENDPOINTS-NOT-IMPLEMENTED`)

## Purpose

Six of the ten routes `AssetsApiBuilder` serves (`GET listdir`, `POST data|metadata|entry`,
`DELETE data|entry`) are stubs returning a bare `501` with a plain-text body, although
`reference/WEB_API_SPECIFICATION.md` §5.1 documents all of them. Wire each one to the
`AssetManager` method that already exists, return the §3 envelope on success and failure, and
make the specification true at HEAD — changing it in the same commit wherever a decision below
departs from what it says.

## Core Interactions

### Query System
Path is parsed with `parse_query`, as the working handlers do. Write/delete/listdir act on
*keys*: a query that is not a pure key (`query.key()` is `None`) is refused with the documented
envelope (`ErrorType::NotSupported`, 501) — a specified refusal, not a blanket stub.

### Store System
No new store code. Reached only through `AssetManager` (`set_binary`, `remove`,
`listdir_asset_info`), which already owns store writes, locking and status rules.

### Command System
None. No new commands; `specs/command_registry.yaml` is unaffected.

### Asset System
- `GET listdir` → `listdir_asset_info(&key)`: union of live assets, store and recipes.
- `POST data` → `set_binary(key, body, metadata)`; metadata starts from the stored record
  (or empty) — `set_binary` decides `Source` vs `Override` from recipe existence.
- `POST entry` → same, with the `DataEntry` body (CBOR/bincode/JSON) supplying the metadata.
- `POST metadata` → the only one without a ready method (see Q2).
- `DELETE data|entry` → `remove(&key)`: evicts the cached asset, cancels it if running, deletes
  the stored value; a recipe survives, so the next read recomputes (spec §5.1.6's `new_status`).

### Value Types
None.

### Web/API
Only `liquers-axum/src/assets/handlers.rs` changes; routes and builder stay as they are.
First real handler tests: drive the built `Router` with `tower::ServiceExt::oneshot` against an
in-memory environment (addresses part of `AXUM-HANDLER-TEST-COVERAGE`).

### UI
N/A.

## Crate Placement

`liquers-axum` (handlers, tests). `liquers-core` only if Q2 needs a trait method with a default.
Spec edits: `WEB_API_SPECIFICATION.md` §5.1.3–5.1.9 (+ `## History`), issue status.

## Open Questions

1. Listing shape: spec §5.1.3 shows `{assets:[{key,status}]}`; return full `AssetInfo` records
   (richer, what the issue wants), wrapped as `{assets:[AssetInfo…]}`?
2. `POST metadata`: (a) a defaulted `AssetManager::set_metadata(key, record)` that re-reads stored
   bytes and calls `set_binary`; (b) do it in the handler via store + `set_binary`; or (c) refuse
   with `NotSupported` and change the spec. Leaning (a).
3. Success status: spec says 201 for POSTs; `ApiResponse::ok` always sends 200. Add a created
   variant, or document 200?
4. DELETE response `new_status`: compute it after removal (`Recipe` if a recipe exists else
   `None`) — needs one `recipe_opt` lookup; acceptable?
5. Dependents of a deleted key are not expired by `remove` — record as an issue, out of scope?

## References

- `specs/issues/AXUM-ASSETS-API-ENDPOINTS-NOT-IMPLEMENTED.md`, `AXUM-HANDLER-TEST-COVERAGE.md`
- `specs/reference/WEB_API_SPECIFICATION.md` §3, §5.1; `specs/design/axum-assets-recipes-api/`
- `liquers-core/src/assets.rs` `AssetManager` (`remove`, `set_binary`, `listdir_asset_info`)
