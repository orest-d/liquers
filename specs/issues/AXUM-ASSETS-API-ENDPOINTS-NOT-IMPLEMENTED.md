---
id: AXUM-ASSETS-API-ENDPOINTS-NOT-IMPLEMENTED
kind: issue
title: Six documented assets API endpoints return 501 Not Implemented
status: draft
priority: P0
complexity: L
area: [axum, core/assets]
design: 
created: 2026-09-15
github:
---
## Problem

`AssetsApiBuilder::build` routes ten handlers. Six of them are stubs that return
`StatusCode::NOT_IMPLEMENTED` without touching the environment:

| Endpoint | Handler | `handlers.rs` |
|---|---|---|
| `GET /listdir/{*query}` | `listdir_handler` | :334 |
| `POST /data/{*query}` | `post_data_handler` | :93 |
| `POST /metadata/{*query}` | `post_metadata_handler` | :187 |
| `POST /entry/{*query}` | `post_entry_handler` | :310 |
| `DELETE /data/{*query}` | `delete_data_handler` | :108 |
| `DELETE /entry/{*query}` | `delete_entry_handler` | :322 |

Every one of them is specified in `reference/WEB_API_SPECIFICATION.md` §5.1 — listdir is §5.1.3,
POST data §5.1.4, POST metadata §5.1.5, DELETE data §5.1.6, DELETE entry §5.1.7, POST entry
§5.1.9, the last with a fully worked CBOR and JSON request format. The specification is a
`reference/` document, so by §2 of `DOCS_STRUCTURE_GUIDE.md` it must be true at HEAD. It is not.

Only `GET /data`, `GET /metadata`, `GET /entry` and `POST /cancel` do anything.

Two of the stubs name their own prerequisite in a comment: deletion "would require
AssetManager.delete() method which needs to be added to the AssetManager trait", and listing
"would require AssetManager.list_assets()". Neither method exists on the trait, so this is not
purely an `liquers-axum` defect — the asset layer has no listing or removal surface to expose.

## Impact

**P0 by §4.4: a documented feature that does not work.** A client written against the
specification gets a 501 with a plain-text body rather than the documented envelope, and the only
way to discover which half of the API is real is to read the handlers.

`GET /listdir` is the worst of the six. `AssetInfo` already carries `title`, `description`,
`type_identifier`, `media_type`, `file_size`, `status`, `updated` and `unicode_icon`, so an asset
listing is the one call that returns a whole directory's worth of descriptive metadata without
reading any data. It is the natural browse primitive of the assets API, and it is the endpoint
that is missing.

The workaround is to use the store API, which implements all of it. That is a real workaround and
it is why this is filed as a defect rather than a crisis — but it is also a downgrade: the store
API answers about *stored bytes*, not about assets, so it cannot report evaluation status,
volatility, expiration or progress, and it cannot address a computed query at all.

Found while designing `AGENT-MEMORY-SERVICE`, which wants exactly the asset-level listing this
endpoint is specified to provide.

## Expected behaviour

The six endpoints behave as `WEB_API_SPECIFICATION.md` §5.1 specifies, which requires first
deciding what they mean at the asset level and adding the missing `AssetManager` surface:

- **Listing.** What a directory of assets *is* — stored keys, recipe-declared keys, cached query
  assets, or the union — and what `AssetInfo` reports for one that has never been evaluated.
- **Removal.** Whether `DELETE` evicts the cached asset, deletes the stored value, or both, and
  what happens to dependents. `CORE-ASSET-GC` and `QUEUED-MANAGER-EVICTION-RACE` are adjacent.
- **Writing.** What `POST /data` means for a computed asset: rejecting it for a query that is not
  a pure key is defensible, but it has to be a specified refusal with the documented error
  envelope, not a blanket 501.

Until then, a handler that cannot do its job should return the documented error envelope naming
`ErrorType::NotSupported` rather than a bare status with a plain-text body — `api_core/error.rs`
already maps that type to 501.

If any endpoint is decided against, the specification is what changes, in the same commit.

## Discovery

Analysis for `AGENT-MEMORY-SERVICE`, 2026-09-15. Read at HEAD in
`liquers-axum/src/assets/handlers.rs` and cross-checked against
`reference/WEB_API_SPECIFICATION.md` §5.1. `AXUM-HANDLER-TEST-COVERAGE` is the reason nothing
caught it: there is no handler test scaffolding, so a stub and an implementation are
indistinguishable to the suite.
