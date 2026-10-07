---
id: CORE-STORE-ROUTER-DIRECTORY-ABOVE-MEMBERS-HAS-NO-METADATA
kind: issue
title: AsyncStoreRouter reports KeyNotFound for the metadata of a directory above its members' prefixes
status: closed
priority: P3
complexity: S
area: [core/store]
design: store-router-directory-above-members
created: 2026-09-30
github:
---

## Problem

`AsyncStoreRouter::get_metadata` (`liquers-core/src/store.rs`, around line 2092) forwards to
`find_store(key)` and returns `KeyNotFound` when no member owns the key. For a directory *above* the
members' prefixes — the root of a router holding `mem/` and `files/`, say — `find_store` is `None`,
so `get_metadata` fails although `is_dir` answers `true` for the same key (around line 2164).

## Impact

STORE_SEMANTICS §2 requires a directory key's metadata to be directory-shaped and, since
2026-09-29, to list its children. A caller browsing a composed namespace from its root gets an error
instead of a listing. No conformance rule covers it: fixtures stay inside one member's prefix.

## Expected behaviour

For a key no member owns but `is_dir` answers, return `default_metadata(key, true)` with
`children` from `listdir_asset_info`, like the trait default. Consider a router fixture that
requests a key above the members so a rule can check it.

## Discovery

Found 2026-09-29 by the final Phase 4 review of `design/store-conformance-backlog/`, which
had claimed the router used the trait default for such keys.

## Resolution (2026-10-07)

Fixed by design `store-router-directory-above-members`. `AsyncStoreRouter::get_metadata` now
answers a key above its members that `is_dir` reports as a directory with directory metadata whose
`children` list what is mounted below it, as STORE_SEMANTICS §2 requires. A key no member owns and
that is not a directory is still `KeyNotFound`.

Evidence: `router_root_metadata_lists_members`, `router_intermediate_directory_metadata`,
`router_unowned_key_metadata_not_found` (`liquers-core/src/store.rs`).
