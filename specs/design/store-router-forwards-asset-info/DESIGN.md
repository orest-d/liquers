---
id: STORE-ROUTER-FORWARDS-ASSET-INFO
kind: design
title: Store router forwards asset info to its members
form: compact
workflow: liquers-project
status: in_review
phase: high-level
area: [core/store]
issues: [STORE-ROUTER-DOES-NOT-FORWARD-ASSET-INFO]
created: 2026-10-13
---
# Store router forwards asset info to its members

Step 0.2 (D1) of [`agent-memory-mvp/execution-plan.md`](../agent-memory-mvp/execution-plan.md).

## Phase 1: High-Level Design

### Purpose

`AsyncStoreRouter` (`liquers-core/src/store.rs`) is how every multi-store environment is assembled,
so a member store must answer through it exactly as it answers alone. Today it does for `get`,
`get_metadata`, `set`, `listdir`, `is_dir` and the rest, but not for `get_asset_info` and
`listdir_asset_info`: those fall to the `AsyncStore` trait defaults running *on the router*. A member
that overrides them — first of all the front-matter adapter of `store-adapters`, which describes a
folder from its `README.md` (`agent-memory-mvp/tiers-and-storage.md` §2 C) — is silently ignored
once mounted. This design makes the router forward both, so folder tiers work with the adapter
mounted as a member instead of wrapping the whole router.

### Problem Example

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

### Scope and Acceptance Criteria

In scope: the router's `get_asset_info`, `listdir_asset_info` and `contains`, plus tests. Out of
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

### Systems touched and placement

`liquers-core/src/store.rs` `impl AsyncStore for AsyncStoreRouter` only — no new type, trait method,
crate or command. Tests in the same file and in `liquers-core/tests/store_conformance_CONF.rs`.

### Documentation intent

`specs/reference/STORE_SEMANTICS.md`: one paragraph stating that a router forwards every trait method
— including `get_asset_info` and `listdir_asset_info` — to the owning member, and answers keys above
its members itself (History row, `reviewed:` bump). `agent-memory-mvp/tiers-and-storage.md` keeps
its note; the issue closes with a resolution pointing here.

### Design Readiness

Readiness is assessed at the end of Phase 4. Decisions taken in Phase 1, each with the alternative
it rejects:

1. **`contains` is fixed here (AC-7)**, not filed separately: it sits on the same `AssetManager` path
   to the router and is a one-branch change in the same impl. Alternative: a separate issue, which
   would leave folder tiers above two mounts failing after this design ships.
2. **`listdir_asset_info` forwards only for a sole owner (AC-6)**; otherwise it is computed per child
   through the forwarding `get_asset_info` (AC-5). Forwarding to the member owning the directory key
   unconditionally would drop children contributed by other members — the same omission the
   router's `listdir` merge exists to avoid.

### Open Questions

1. **Directory metadata of a member-owned directory with another member mounted beneath it.**
   `router.get_metadata(k)` forwards wholesale to the member owning `k`, so its `children` come from
   that member alone and omit the nested mount point that `router.listdir(k)` reports. The same
   shape as AC-5/AC-6 would fix it (forward, then replace `children` with the router's listing when
   another member contributes). Recommended: **include it** as AC-9 — it is the same rule in the same
   impl and keeps `get_metadata` and `listdir_asset_info` agreeing. Alternative: file it as an issue.
2. **`is_dir` and `find_store` disagree on routing.** `is_dir` picks the first member by prefix
   alone; every other method also requires `is_supported`. With a narrowing member (an overlay on one
   file) listed first, `is_dir` asks the wrong member. No such member exists in tree. Recommended:
   **file it as an issue**, not fix it here — changing `is_dir`'s routing changes behaviour for every
   environment and needs its own test matrix. Filed as `STORE-ROUTER-IS-DIR-IGNORES-IS-SUPPORTED`
   (P3); if you prefer to fold it in, it becomes AC-10.
