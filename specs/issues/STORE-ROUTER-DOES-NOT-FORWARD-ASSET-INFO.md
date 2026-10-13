---
id: STORE-ROUTER-DOES-NOT-FORWARD-ASSET-INFO
kind: issue
title: AsyncStoreRouter does not forward get_asset_info to its member stores
status: draft
priority: P2
complexity: S
area: [core/store]
design: store-router-forwards-asset-info
created: 2026-10-11
github:
---
## Problem

`AsyncStoreRouter`'s `impl AsyncStore` (`liquers-core/src/store.rs`, from line 1147) forwards
`get`, `get_metadata`, `set`, `set_metadata`, `listdir`, `is_dir` and the rest to the member store
that owns the key, but it does **not** override `get_asset_info` or `listdir_asset_info`. Both fall
back to the trait defaults (`store.rs:174`, `:275`), which run on the router itself:

- for a directory, the default builds the info from `self.default_metadata(key, true)` — the
  router's, not the member's — without consulting the member at all;
- for a file, it goes through `router.get_metadata`, so only what the member exposes through
  `get_metadata` survives.

A member store that overrides `get_asset_info` or `listdir_asset_info` (to describe its
directories, or to list more cheaply) is therefore ignored as soon as it is mounted in a router,
which is how every environment with more than one store is assembled.

## Impact

Silent: the member's override is simply not called. No current store overrides these methods, so
nothing is wrong today; the first one that does — the front-matter store proposed by
`design/agent-memory-mvp/` (`tiers-and-storage.md` §2 C), which describes a folder from its
`README.md` — will not work behind a router. Workaround: wrap the whole router instead of a member.

## Expected behaviour

The router forwards `get_asset_info` and `listdir_asset_info` to the owning member, as it does
`get_metadata`, with the same prefix handling; a conformance test checks that a member's override
is observed through the router.

## Discovery

Codebase review of `agent-memory-mvp` Phase 1 revision 2, 2026-10-11, checking whether a store
adapter mounted in `AsyncStoreRouter` could project folder tiers.
