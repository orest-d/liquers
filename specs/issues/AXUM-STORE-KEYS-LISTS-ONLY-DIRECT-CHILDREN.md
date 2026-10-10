---
id: AXUM-STORE-KEYS-LISTS-ONLY-DIRECT-CHILDREN
kind: issue
title: GET /api/store/keys lists only the direct children of the prefix, not all keys
status: closed
priority: P3
complexity: S
area: [axum]
design: axum-store-keys-deep
created: 2026-09-28
github:
---
## Problem

`keys_handler` (`liquers-axum/src/store/handlers.rs`, `GET {store}/keys?prefix=…`) calls
`AsyncStore::listdir_keys(prefix)` — the keys directly inside the prefix directory — so it answers
exactly what `GET {store}/listdir/{prefix}` does. Its name, its doc comment ("List all keys,
optionally filtered by prefix") and the earlier specification promised every key under the prefix,
which is `AsyncStore::listdir_keys_deep`.

## Expected behaviour

`keys` returns every key under the prefix (`listdir_keys_deep`), so a client can enumerate a
subtree in one call; or the route is documented as a synonym of `listdir` and its doc comment
corrected.

## Discovery

Found 2026-09-28 during the `WEB_API_SPECIFICATION.md` audit of `specs/design/axum-assets-endpoints/`
(Step 13); the specification now documents the current behaviour.

## Resolution (2026-10-10)

Maintainer decision (2026-10-08, D2): deep. `keys_handler` (`liquers-axum/src/store/handlers.rs`)
now returns `listdir_keys_deep(prefix)`, root by default. Tests
`store_keys_lists_nested_keys_under_prefix`, `store_keys_without_prefix_lists_whole_store` and
`store_listdir_still_lists_direct_children_only` in `tests/store_api_routes.rs`;
`WEB_API_SPECIFICATION.md` updated. Design: `design/axum-store-keys-deep/`.
