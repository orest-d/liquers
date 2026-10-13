---
id: STORE-ROUTER-LISTS-AN-UNCREATED-MEMBER-PREFIX
kind: issue
title: AsyncStoreRouter lists an uncreated member prefix that it does not contain
status: closed
priority: P3
complexity: S
area: [core/store]
design: store-router-forwards-asset-info
created: 2026-10-13
github:
---
## Problem

`AsyncStoreRouter::listdir` (`liquers-core/src/store.rs`) names every member's mount point in its
parent's listing, from the prefixes alone. Whether the mount point itself is a directory is left to
the member: `is_dir`, `contains` and `get_metadata` on the prefix key go to it. `AsyncFileStore`
answers `false` / `KeyNotFound` for a prefix directory that has not been created yet
(`tokio::fs::try_exists` fails), so with a file store mounted at `files` and nothing written:

- `router.listdir("")` contains `files`;
- `router.contains("files")` is `false` and `router.get_metadata("files")` is `KeyNotFound`.

`STORE_SEMANTICS.md` §8 expects a listed key to be contained, and §2 expects directory metadata for
it.

## Impact

Low. A UI or `catalog` listing the root shows an entry that cannot be opened until the first write
creates the folder. `C3` mounts exactly this case (`files/` deliberately absent) but asks no rule
about the mount point itself.

## Expected behaviour

Either the router treats a member's prefix as a directory unless the owning member holds data at
that key, or `AsyncFileStore` (and other path-backed stores) reports its own prefix as a directory
whether or not the path exists yet. The second keeps the rule inside the store contract; a single-file
member mounted at a file key is the case the first must not break.

## Discovery

Phase 2 review of `design/store-router-forwards-asset-info/`, 2026-10-13, which rejected making
every mount point a directory for that single-file case.

## Resolution (2026-10-13)

Fixed by design `store-router-forwards-asset-info` (decision 6, option A; commit 91354e7), the first of the two expected behaviours: the router treats a member's own prefix as a directory unless the owning member holds data at that key, so an empty memory store or an uncreated file-store folder is a directory and a member mounted at a single file stays a file. Found to block the design's AC-9 for empty nested mounts. Test `routerfwd11_empty_member_prefix_is_a_directory`; suites `C3` and `C6`.
