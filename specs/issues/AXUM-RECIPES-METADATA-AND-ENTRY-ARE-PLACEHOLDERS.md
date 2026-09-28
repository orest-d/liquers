---
id: AXUM-RECIPES-METADATA-AND-ENTRY-ARE-PLACEHOLDERS
kind: issue
title: The Recipes API's metadata and entry endpoints return placeholder metadata and ignore the format
status: draft
priority: P3
complexity: S
area: [axum]
design:
created: 2026-09-28
github:
---
## Problem

In `liquers-axum/src/recipes/handlers.rs`:

- `get_metadata_handler` (`GET {recipes}/metadata/{*key}`) checks that the recipe exists and then
  answers `result: {}` — a placeholder comment says so. The recipe's title, description, filename
  and planning diagnostics, which `AsyncRecipeProvider::get_asset_info` already computes, are not
  returned.
- `get_entry_handler` (`GET {recipes}/entry/{*key}`) always serializes CBOR, ignoring `?format=`
  and `Accept` (the Store and Assets entry routes honour both), and sends `metadata: {}`.

## Expected behaviour

`metadata` returns the recipe's `AssetInfo` (or the `MetadataRecord` built from it, as the Assets
API's `key/metadata` does for a recipe key), and `entry` negotiates its format like the other entry
routes and carries the same metadata.

## Discovery

Found 2026-09-28 during the `WEB_API_SPECIFICATION.md` audit of `specs/design/axum-assets-endpoints/`
(Step 13); the specification now documents the current behaviour.
