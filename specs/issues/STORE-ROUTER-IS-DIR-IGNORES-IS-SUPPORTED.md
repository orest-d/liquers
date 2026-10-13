---
id: STORE-ROUTER-IS-DIR-IGNORES-IS-SUPPORTED
kind: issue
title: AsyncStoreRouter::is_dir routes by prefix alone while other methods also require is_supported
status: draft
priority: P3
complexity: S
area: [core/store]
design: store-router-forwards-asset-info
created: 2026-10-13
github:
---
## Problem

`AsyncStoreRouter` (`liquers-core/src/store.rs`) picks the owning member in two different ways.
`find_store`, used by `get`, `get_metadata`, `set`, `contains` and the rest, takes the first member
whose prefix is a prefix of the key **and** whose `is_supported(key)` is true. `is_dir` takes the
first member whose prefix is a prefix of the key and asks it, without consulting `is_supported`;
`listdir` likewise asks every member whose prefix matches.

With a narrowing member listed first — the overlay `AsyncStore::is_supported`'s documentation
describes, an empty prefix that supports one file — `router.is_dir("data")` asks the overlay, which
answers `false`, while `router.get_metadata("data/x")` goes to the second member. The router then
reports `data` as not a directory although keys beneath it are readable.

## Impact

None today: no member in tree narrows beyond its prefix. It becomes reachable as soon as an overlay
or a selective adapter (`design/store-adapters/`) is mounted ahead of a broader member. Workaround:
mount narrowing members after the broad ones, or give them a non-empty prefix.

## Expected behaviour

`is_dir` routes to the same member `find_store` would, and falls back to the next matching member
(or to the above-members rule) when a narrowing member does not support the key; a router test with
an overlay listed first checks `is_dir`, `contains` and `listdir` agree.

## Discovery

Codebase review for `design/store-router-forwards-asset-info/` (Phase 1, open question 2),
2026-10-13.
