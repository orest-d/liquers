---
id: STORE-ROUTER-FORWARDS-ASSET-INFO
kind: design
title: Store router forwards asset info to its members
form: compact
workflow: liquers-project
status: in_review
phase: architecture
area: [core/store]
issues: [STORE-ROUTER-DOES-NOT-FORWARD-ASSET-INFO, STORE-ROUTER-IS-DIR-IGNORES-IS-SUPPORTED]
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

### Scope Changes

2026-10-13, at the Phase 1 gate, both open questions were folded in at the user's request. The
source issue is size S; with AC-9 and AC-10 the design is M (five-page limit) and stays compact.

- **AC-9** Directory metadata names every child the router lists (was open question 1)
  - WHEN the router is asked `get_metadata` for a directory owned by one member while another member is mounted beneath it or also lists inside it
  - THEN the record's `children` are the router's `listdir_asset_info` of that key — the names `router.listdir` gives — and every other field is the owning member's
- **AC-10** One directory rule for the whole router (was open question 2, `STORE-ROUTER-IS-DIR-IGNORES-IS-SUPPORTED`)
  - WHEN the router is asked `is_dir` for a key that any member covering it reports as a directory (not only the first such member), or for a key above a member's mount point
  - THEN it answers `Ok(true)` whatever the members' order and `is_supported` filters, and `Ok(false)` otherwise, so `is_dir`, `contains` and `listdir` agree

## Phase 2: Architecture

### Solution

**One rule: a sole contributor is forwarded to; several contributors are composed by the router.**
The *contributors* of a key `k` are the members `listdir` already consults: those whose prefix covers
`k` (`k.has_key_prefix(prefix)`) and those mounted strictly beneath it. The *owner* is `find_store(k)`,
unchanged. With one contributor that is also the owner, every method forwards wholesale — today's
behaviour and the cheap path, and what makes a member override visible. Otherwise the router asks the
owner first and composes directory answers from all contributors.

| Method | New behaviour | AC |
|---|---|---|
| `is_dir` | `true` if `k` is the root or lies strictly above a member's prefix (today's rule), else `true` if **any** covering member's `is_dir` is `true` (first `true` wins, errors propagate), else `false` | AC-10, AC-3 |
| `contains` | owner's `contains` if `true`, else `self.is_dir(k)`; never `KeyNotSupported` | AC-7 |
| `get_asset_info` | owner's `get_asset_info`; on `KeyNotFound` (or no owner) a router-built directory `AssetInfo` if `self.is_dir(k)`, else `KeyNotFound` | AC-1–AC-4 |
| `listdir_asset_info` | sole contributor: forwarded; otherwise per child of `self.listdir_keys(k)` through `self.get_asset_info`, sorted as the trait default sorts | AC-5, AC-6 |
| `get_metadata` | owner's answer; if it is a directory record and there is no sole contributor, `children` replaced by `self.listdir_asset_info(k)`; on `KeyNotFound` (or no owner) the existing above-members branch, now gated by the new `is_dir` | AC-9, AC-3 |

`listdir`, `get`, the write methods and `default_metadata` / `finalize_metadata*` are unchanged.

Decisions and rejected alternatives:

- **`is_dir` is a union, not routed through `find_store`.** The issue suggested routing it like the
  other methods; that would make `is_dir("cfg")` false when an overlay lists `cfg` but does not
  support the directory key, and `listdir` — which merges every covering member and ignores
  `is_supported` — would still list it. A directory exists if any member says so; data has one owner.
- **A member's own prefix is the member's to answer**, as today. Declaring every mount point a
  directory would make a member mounted at a single file (`config.yaml`, supported for that key only)
  a directory too. The cost stays as it is: an uncreated `AsyncFileStore` prefix is listed by its
  parent but not contained — filed as `STORE-ROUTER-LISTS-AN-UNCREATED-MEMBER-PREFIX`.
- **Fallback on `KeyNotFound` only.** Any other member error is a backend failure (§4) and propagates.
  The guard is `Err(e) if e.error_type == ErrorType::KeyNotFound` on a `Result`, followed by `Err(e)`;
  no default arm on a Liquers enum.
- **Data wins over a derived directory.** If the owner holds data at `k` and another member is mounted
  beneath `k`, `get_metadata` / `get_asset_info` return the owner's data answer, as today; `is_dir`
  answers `true` and `listdir` lists the mount. Recorded, not resolved: such a layout is a
  configuration error and no store refuses it today.
- Rejected: a new trait method (`AsyncStore::contributes`) — the router is the only composer; a
  wrapper store would not need it. Rejected: changing `AssetManager` to skip `contains` — it would fix
  AC-7's symptom and leave the router breaking §4 for every other caller.

**Known-issue preflight.** No open issue blocks this. `design/store-adapters/` (in review) assumes it
(its AC-8). `STORE-NO-READ-ONLY-ADAPTER`, `STORE-METADATA-HANDLING-NOT-COMPOSABLE` and
`STORE-METADATA-LAYOUT-HARDCODED-PER-STORE` touch stores but not the router's dispatch. The two
OpenDAL router tests (`router01_a_prefixed_store_does_not_claim_the_whole_router`,
`opendal_router_is_dir_reaches_a_prefixed_store`) keep passing; their doc comments describe the old
`is_dir` and are updated. Command namespaces involved: none.

### Changes

All in `liquers-core/src/store.rs`; no public signature changes, no new types, no commands, no
`specs/command_registry.yaml` change.

```rust
// Module-level, shared by `AsyncStore::listdir_asset_info` (default) and the router.
fn sort_asset_info(info: &mut [AssetInfo]);

impl AsyncStoreRouter {
    /// Root, or strictly above a member's prefix. Sync: prefixes only.
    fn is_mount_directory(&self, key: &Key) -> bool;
    /// The one member that can answer for everything inside `key`: the only contributor, and the owner.
    fn sole_contributor(&self, key: &Key) -> Option<&dyn AsyncStore>;
    /// Directory record built by the router (key, filename, `is_dir`), as `get_metadata`'s
    /// above-members branch builds it today; extracted so `get_asset_info` shares it.
    fn directory_record(key: &Key) -> MetadataRecord;
}
// `impl AsyncStore for AsyncStoreRouter`: new overrides `get_asset_info`, `listdir_asset_info`;
// changed `is_dir`, `contains`, `get_metadata`.
```

`get_metadata`'s children replacement matches `Metadata::MetadataRecord(record)` (replace when
`record.is_dir`) and `Metadata::LegacyMetadata(_)` (returned unchanged: it has no typed `children`).
Ownership: `sole_contributor` returns `self.stores[i].as_ref()`; nothing is cloned. Async: helpers are
synchronous and cheap (prefix comparisons); every method stays `async`, with the existing
`async_trait` / `?Send` wasm attributes. No `unwrap`/`expect`; errors only `Error::key_not_found`.

**Documentation.** `specs/reference/STORE_SEMANTICS.md` §6: replace "`is_dir` and `listdir` select on
`key_prefix()` alone" with the router rules above (forwarding, contributors, union `is_dir`), History row, `reviewed:` bump. `specs/guides/STORE_IMPLEMENTATION_GUIDE.md` §Prefix
bullet and §9 router row: same correction, History row. Code comments: the module doc of
`liquers-core/src/store_conformance/rules/prefix.rs` and the two OpenDAL tests above.
`affects_docs: [STORE_SEMANTICS, STORE_IMPLEMENTATION_GUIDE]`.

### Risks

| Risk | Consequence | Handling |
|---|---|---|
| `contains` no longer errors for unowned keys | `AssetManager::get_asset_info` and other callers fall through to the recipe provider instead of failing — the §4 behaviour, but observable | Phase 3 greps tests asserting `KeyNotSupported` from a router; AC-7 test through `AssetManager` |
| `is_dir` now asks members after the first | A member listed later can turn a `false` into `true` | That is AC-10; run `C3`, `store_router_STORE.rs` and the OpenDAL router tests |
| `is_dir` asks more members on a miss | One extra `is_dir` per additional covering member (deployments have one or two) | Accepted; hit path unchanged |
| Non-sole `get_metadata` computes children twice | The owner's own listing is discarded | Accepted: only when members nest |
| Overlay layouts untested in tree | AC-10's case has no real member | A test-only narrowing member in the unit tests |

Certainty: high. Tests likely to change: none expected to break; `router_unowned_key_metadata_not_found`
must still pass. Recovery: one file, revert the commit.

## Phase 3: Examples and Tests

## Phase 4: Implementation Plan

## Phase 5: Documentation
