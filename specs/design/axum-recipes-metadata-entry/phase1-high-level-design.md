# Phase 1: High-Level Design

## Design Readiness

- **Readiness:** ready
- **Automatic fixing:** eligible — bug fix in `liquers-axum` replacing placeholder answers with the
  documented ones; no route or type changes (already implemented)
- **Leading issue:** None
- **Explanation:** Both behaviours already exist elsewhere in the same crate. The Assets API builds
  a `MetadataRecord` from `AsyncRecipeProvider::get_asset_info` for a recipe key (`key_metadata`),
  and negotiates entries with `entry_response`. The Recipes API reuses them, so there is nothing
  new to decide.
- **Open questions:** None

## Problem

`liquers-axum/src/recipes/handlers.rs`:

- `get_metadata_handler` (`GET {recipes}/metadata/{*key}`) checks that the recipe exists and
  answers `{}` (a placeholder).
- `get_entry_handler` (`GET {recipes}/entry/{*key}`) always serializes CBOR, ignoring `?format=`
  and `Accept`, and sends `metadata: {}`.

`specs/reference/WEB_API_SPECIFICATION.md` documents this current behaviour.

## Expected behaviour and acceptance

1. `GET {recipes}/metadata/{key}` returns `ApiResponse::ok(MetadataRecord::from(info) as JSON)`,
   where `info = provider.get_asset_info(&key, env)`. It includes title, description, filename and
   planning diagnostics.
2. `GET {recipes}/entry/{key}` returns a `DataEntry { data: recipe YAML bytes, metadata: same
   JSON }` negotiated by `?format=` → `Accept` → CBOR, with the matching `Content-Type`.
3. An unknown recipe key: the same error response as today.

## Scope

The two handlers. Other Recipes routes are unchanged.

## Design Dependencies

- `axum-assets-endpoints` — **overlaps** (its audit found this; it owns `entry_response` and
  `key_metadata`).

## Documentation assessment

- Reference: `specs/reference/WEB_API_SPECIFICATION.md`, Recipes API rows: replace the
  placeholder descriptions.
- Guide: `WEB_API_GUIDE.md`, if it shows a recipe metadata example.

## Consolidated Findings

- `entry_response(data, metadata: &Metadata, headers, params)` takes a `Metadata`. Build
  `Metadata::MetadataRecord(MetadataRecord::from(info))` once and use it for both routes.
- The handlers need `HeaderMap` and `AxumQuery<HashMap<String,String>>` extractors added to the
  entry handler signature. The router in `recipes/builder.rs` (or equivalent) needs no change,
  because axum infers extractors.
- `entry_response` is `pub(crate)` in `assets/common.rs`, so it is reachable from `recipes`.
