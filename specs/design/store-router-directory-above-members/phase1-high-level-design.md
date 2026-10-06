# Phase 1: High-Level Design

## Design Readiness

- **Readiness:** ready
- **Leading issue:** None
- **Explanation:** STORE_SEMANTICS §2 already requires directory-shaped metadata with children for
  any key `is_dir` answers. The router's own `is_dir` answers `true` above its members, and its
  `get_metadata` must agree. Nothing is left to decide.
- **Open questions:** None

## Problem

`AsyncStoreRouter::get_metadata` (`liquers-core/src/store.rs`) forwards to `find_store(key)` and
returns `KeyNotFound` when no member owns the key. For a key above the members' prefixes (the root
of a router holding `mem/` and `files/`), `find_store` is `None`, but `is_dir` answers `true`
(the "key is a prefix of store prefix" branch, plus the empty-key case). A client browsing from the
root gets an error instead of a listing. `listdir_asset_info` of such a key also calls
`get_asset_info` on intermediate directory children above deeper members, and hits the same error.

## Expected behaviour and acceptance

1. Router with members at `mem` and `files`: `get_metadata(root)` returns a directory record
   (`is_dir: true`) whose `children` list `mem` and `files`.
2. Member mounted at `a/b`: `get_metadata(a)` is a directory with child `b`.
3. A key no member owns and `is_dir` says `false`: still `KeyNotFound`.
4. A conformance rule, or a router-specific test, requests a key above the members (see Phase 2).

## Scope

The router's `get_metadata`. Other router methods are unchanged.

## Design Dependencies

None. (`store-conformance-backlog` is complete. It found the issue.)

## Documentation assessment

- Reference: none. STORE_SEMANTICS already states the rule.
- Guide: `STORE_IMPLEMENTATION_GUIDE.md` §9: drop the note naming this issue from the router row.

## Consolidated Findings

- The trait default `get_metadata` builds directory metadata with `default_metadata(key, true)`
  plus `children = listdir_asset_info(key)`, as `AsyncMemoryStore` does. The router reuses the
  same shape.
- No generic fixture requests a key above the members: the fixtures' subject keys live inside one
  prefix. A router-specific unit test is cheaper than a new generic rule and is the chosen
  validation. A generic rule would need a fixture capability ("has keys above its members"),
  which only the router has.
