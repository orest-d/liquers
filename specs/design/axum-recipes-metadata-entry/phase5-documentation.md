# Phase 5: Documentation - Recipes API metadata and entry

**Status: executed 2026-10-07**, after implementation (Wave 5 step 31 of
`archive/2026-10-06-p2-p3-s-implementation-order-revised.md`). Awaiting approval.

## Completion Preconditions

- [x] Implementation is finished and validated (Phase 4)
- [x] All user comments are answered or incorporated
- [x] All review comments are answered or incorporated (none yet)
- [x] Documentation is consistent with the implemented and tested behavior
- [x] Documentation is included in the implementation PR

## Documentation Plan

### New Reference Documents

None.

### New Guide Documents

None.

### Existing Documents to Update

| Document | Change |
|---|---|
| `reference/WEB_API_SPECIFICATION.md` | §6 Recipes API: the `metadata` and `entry` rows describe the real responses instead of placeholders. History row, `reviewed: 2026-10-07` |

### Candidates Considered and Discarded

`guides/WEB_API_GUIDE.md` has no example of recipe metadata or entry.

### Issues to Close

`AXUM-RECIPES-METADATA-AND-ENTRY-ARE-PLACEHOLDERS`.

## Implementation Summary

`liquers-axum/src/recipes/handlers.rs`:
- New `recipe_metadata`: `get_asset_info` → `Metadata::MetadataRecord(MetadataRecord::from(info))`.
- `get_metadata_handler` keeps its existence check and returns `metadata_json` of that metadata.
- `get_entry_handler` takes `HeaderMap` and `Query<HashMap<_, _>>` and answers through the Assets
  API's `entry_response`, so negotiation is shared rather than copied.

Tests added to `liquers-axum/tests/recipes_api_routes.rs`:
- `recipe_metadata_returns_recipe_asset_info` (E1: title, description, filename)
- `recipe_entry_honours_format_parameter` (E2: JSON; base64 data decoded through `DataEntry`)
- `recipe_entry_honours_accept_header` (E3: `Accept: application/json` gives JSON; with no
  preference the entry is CBOR)
- `recipe_metadata_and_entry_of_an_unknown_key_fail` (T1)

## Documentation Delivered

As planned above.

## Issues Filed

None.

## Important Learning

`DataEntry.data` is base64 in JSON. A client test should decode through `DataEntry`'s own
deserializer rather than read a byte array.

## Conformance and Remaining Work

Conforms to Phases 1–4. No remaining work.

## Validation

- `cargo test -p liquers-axum`: all suites pass (13 in `recipes_api_routes`)
