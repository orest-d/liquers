# Phase 1: High-Level Design — Store router forwards asset info

## Purpose

`AsyncStoreRouter` (`liquers-core/src/store.rs`) is how every multi-store environment is assembled,
so a member store must answer through it exactly as it answers alone. Today it does for `get`,
`get_metadata`, `set`, `listdir`, `is_dir` and the rest, but not for `get_asset_info` and
`listdir_asset_info`: those fall to the `AsyncStore` trait defaults running *on the router*. A member
that overrides them — first of all the front-matter adapter of `store-adapters`, which describes a
folder from its `README.md` (`agent-memory-mvp/tiers-and-storage.md` §2 C) — is silently ignored
once mounted. This design makes the router forward both, so folder tiers work with the adapter
mounted as a member instead of wrapping the whole router.

## Problem Example

A member mounted at `memory` overrides `get_asset_info` to give a directory a title:

```rust
// Illustrative: TitledDirStore = AsyncMemoryStore at prefix "memory" whose get_asset_info
// returns title "Decisions" for the directory memory/decisions.
let mut router = AsyncStoreRouter::new();
router.add_store(Box::new(TitledDirStore::new(&parse_key("memory")?)));

member.get_asset_info(&parse_key("memory/decisions")?).await?.title   // "Decisions"
router.get_asset_info(&parse_key("memory/decisions")?).await?.title   // today: "" — wanted: "Decisions"
```

Today the router's call runs the trait default: `router.is_dir` is true, so it returns
`router.default_metadata(key, true).get_asset_info()`. `default_metadata` does reach the member, but
only its *default* record — the member's `get_asset_info` is never called. For a file key the
default goes through `router.get_metadata`, so only what the member also puts in `get_metadata`
survives.

Two further paths lose the same information:

- **Listing a directory through the router.** `router.listdir_asset_info(&memory)` is the trait
  default, which calls `router.get_asset_info` per child — so it inherits the defect above, and a
  member's own (possibly cheaper) `listdir_asset_info` is never used.
- **`AssetManager::get_asset_info`** (`liquers-core/src/assets.rs`, the path every client takes)
  first asks `store.contains(key)`. The router's `contains` returns `Err(KeyNotSupported)` for any
  key no member owns, including a directory *above* the members (`memory` when members are mounted
  at `memory/a` and `memory/b`), where `STORE_SEMANTICS.md` §4 and the trait default (`contains` →
  `is_dir`) require `Ok(true)`, and `Ok(false)` for an absent key. The conformance fixture `C3`
  keeps every key inside one member, so nothing has caught it.

## Scope and Acceptance Criteria

In scope: the router's `get_asset_info`, `listdir_asset_info`, `contains`, `get_metadata` (AC-9) and `is_dir` (AC-10), plus tests. Out of
scope: changing any member store, the trait defaults, or `AssetManager`.

- **AC-1** Member asset info for a file is observed through the router
  - WHEN a member overrides `get_asset_info` and a caller asks the router for a data key the member owns
  - THEN the router returns exactly the member's `AssetInfo`
- **AC-2** Member asset info for a directory is observed through the router
  - WHEN the member owns a directory key and describes it in its `get_asset_info` override
  - THEN the router returns the member's description, not a default directory record
- **AC-3** Directory above the members
  - WHEN the router is asked for asset info of a key that is a directory only because members are mounted beneath it
  - THEN it returns a directory-shaped `AssetInfo` with that key, `is_dir = true`, built by the router as `get_metadata` already does (STORE_SEMANTICS §2)
- **AC-4** Unowned key
  - WHEN the router is asked for asset info of a key no member owns and that is not a directory above them
  - THEN it returns `KeyNotFound`, as `get_metadata` does
- **AC-5** Listing uses each child's owner
  - WHEN the router lists asset info of a directory whose children belong to different members, or to a member and a mount point
  - THEN each child's entry is what its owning member's `get_asset_info` returns (or AC-3 for a mount point), each name once, sorted as the trait default sorts
- **AC-6** Sole owner lists itself
  - WHEN exactly one member can contribute to a directory (no other member's prefix is a prefix of the key or lies beneath it)
  - THEN the router forwards `listdir_asset_info` to that member, so a member override (or a cheaper listing) is used
- **AC-7** `contains` follows §4 above and outside the members
  - WHEN the router is asked `contains` for a directory above its members, or for an absolute key no member owns that is not such a directory
  - THEN it answers `Ok(true)` and `Ok(false)` respectively, and the asset manager's `get_asset_info` on the directory above members succeeds
- **AC-8** Conformance
  - WHEN the store conformance suite runs against a router whose member overrides `get_asset_info`
  - THEN the router passes with no allowed failure, and the existing `C3` suite still passes

## Core Interactions and Crate Placement

`liquers-core/src/store.rs` `impl AsyncStore for AsyncStoreRouter` only — no new type, trait method,
crate or command. Tests in the same file and in `liquers-core/tests/store_conformance_CONF.rs`.

## Documentation Intent

- Reference: extend `specs/reference/STORE_SEMANTICS.md` §6 — how the router forwards, composes and
  answers `is_dir` (History row, `reviewed:` bump).
- Guide: extend `specs/guides/STORE_IMPLEMENTATION_GUIDE.md` §Prefix and its §9 router row, which
  repeat the old `is_dir` selection.
- Other documents: none. `agent-memory-mvp/tiers-and-storage.md` keeps its note; the issues close
  with a resolution pointing here.

## Design Readiness

Pre-approved after Phase 2 on 2026-10-13 (`proceed all`); this section is the decision log.

- **Readiness:** ready
- **Automatic fixing:** not-eligible — `is_dir` changes from the selection `STORE_SEMANTICS.md` §6
  documents (first member by prefix) to a union: a contract change, not a restoration (rule 4).
- **Leading issue:** None
- **Explanation:** every method's answer is fixed by Phase 2's table; the tests in Phase 3 pin each
  scenario, and the existing router, conformance and OpenDAL router tests guard what stays.
- **Open questions:** None. Proposed resolutions, all shown to the user at the Phase 2 gate and
  accepted with `proceed all`:
  1. `contains` is fixed here (AC-7) rather than filed: it is on the same `AssetManager` path.
  2. `listdir_asset_info` forwards only for a sole contributor (AC-6); otherwise it is composed
     (AC-5), or children from other members would be dropped.
  3. `is_dir` is a union of covering members, not routed through `find_store`: `listdir` merges every
     covering member and ignores `is_supported`, so routing would make them disagree.
  4. A member's own prefix stays the member's to answer; the uncreated-prefix inconsistency is filed
     as `STORE-ROUTER-LISTS-AN-UNCREATED-MEMBER-PREFIX`.
  5. `contains` returning `Ok(false)` instead of `Err(KeyNotSupported)` for unowned keys is an
     observable change (callers reach the recipe provider), accepted as §4 behaviour.

## Scope Changes

2026-10-13, at the Phase 1 gate, both open questions were folded in at the user's request. The
source issue is size S; with AC-9 and AC-10 the design is M. At the Phase 2 gate the user asked for
the full form (one file per phase), which this folder now uses.

- **AC-9** Directory metadata names every child the router lists (was open question 1)
  - WHEN the router is asked `get_metadata` for a directory owned by one member while another member is mounted beneath it or also lists inside it
  - THEN the record's `children` are the router's `listdir_asset_info` of that key — the names `router.listdir` gives — and every other field is the owning member's
- **AC-10** One directory rule for the whole router (was open question 2, `STORE-ROUTER-IS-DIR-IGNORES-IS-SUPPORTED`)
  - WHEN the router is asked `is_dir` for a key that any member covering it reports as a directory (not only the first such member), or for a key above a member's mount point
  - THEN it answers `Ok(true)` whatever the members' order and `is_supported` filters, and `Ok(false)` otherwise, so `is_dir`, `contains` and `listdir` agree
