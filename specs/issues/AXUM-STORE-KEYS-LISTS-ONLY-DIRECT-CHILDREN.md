---
id: AXUM-STORE-KEYS-LISTS-ONLY-DIRECT-CHILDREN
kind: issue
title: GET /api/store/keys lists only the direct children of the prefix, not all keys
status: draft
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
