---
id: AXUM-ASSETS-API-ENDPOINTS-NOT-IMPLEMENTED
kind: issue
title: Six documented assets API endpoints return 501 Not Implemented
status: draft
priority: P0
complexity: M
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

Two of the stubs claim a prerequisite in a comment: deletion "would require
AssetManager.delete() method which needs to be added to the AssetManager trait", and listing
"would require AssetManager.list_assets()". **Both comments are stale.** No method by either name
exists, but the capability does, under other names — every stubbed endpoint has a method waiting
for it on `AssetManager`:

| Stubbed endpoint | Method that already exists |
|---|---|
| `GET /listdir` | `listdir_asset_info(&Key) -> Vec<AssetInfo>` — defaulted, sorts directories first then by filename |
| `POST /data` | `set_binary(&Key, &[u8], MetadataRecord)` — required, so every implementor has it |
| `POST /entry` | `set_binary`, or `set_state(&Key, State<V>)` |
| `POST /metadata` | no direct equivalent; the closest is `set_binary` carrying a record |
| `DELETE /data`, `DELETE /entry` | `remove(&Key)` — required — and `remove_asset(&Query)` |

So this is an `liquers-axum` defect after all: the handlers were never wired to an asset layer that
was ready for them.

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

The six endpoints behave as `WEB_API_SPECIFICATION.md` §5.1 specifies. Most of the work is
wiring, not design:

- **Listing** is a thin wrapper over `listdir_asset_info`. What a directory of assets *is* turns
  out to be already decided in `get_asset_info`, which resolves a key in three steps — a live
  keyed asset, else the store, else the recipe provider — so a listing is the union, and a
  recipe-declared key that has never evaluated is described from its recipe. That behaviour should
  be specified rather than left implicit, but it does not have to be invented.
- **Removal** needs one decision that the trait does not make for us: whether `DELETE` evicts the
  cached asset (`remove_asset`), deletes the stored value (`remove`), or both, and what happens to
  dependents. `CORE-ASSET-GC` and `QUEUED-MANAGER-EVICTION-RACE` are adjacent.
- **Writing** needs the same kind of decision: what `POST /data` means for a query that is not a
  pure key. Refusing it is defensible, but it has to be a specified refusal carrying the documented
  error envelope, not a blanket 501.
- **`POST /metadata`** is the one endpoint with no ready equivalent and needs the most thought.

Until then, a handler that cannot do its job should return the documented error envelope naming
`ErrorType::NotSupported` rather than a bare status with a plain-text body — `api_core/error.rs`
already maps that type to 501.

If any endpoint is decided against, the specification is what changes, in the same commit.

## Discovery

Analysis for `AGENT-MEMORY-SERVICE`, 2026-09-15; corrected 2026-09-16 after reading the
`AssetManager` trait rather than trusting the handlers' comments about it. Read at HEAD in
`liquers-axum/src/assets/handlers.rs` and cross-checked against
`reference/WEB_API_SPECIFICATION.md` §5.1. `AXUM-HANDLER-TEST-COVERAGE` is the reason nothing
caught it: there is no handler test scaffolding, so a stub and an implementation are
indistinguishable to the suite.
