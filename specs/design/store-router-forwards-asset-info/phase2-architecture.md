# Phase 2: Solution & Architecture — Store router forwards asset info

## Overview

**One rule: a sole contributor is forwarded to; several contributors are composed by the router.**
The *contributors* of a key `k` are the members `listdir` already consults: those whose prefix covers
`k` (`k.has_key_prefix(prefix)`) and those mounted strictly beneath it. The *owner* is `find_store(k)`,
unchanged. With one contributor that is also the owner, every method forwards wholesale — today's
behaviour and the cheap path, and what makes a member override visible. Otherwise the router asks the
owner first and composes directory answers from all contributors.

| Method | New behaviour | AC |
|---|---|---|
| `is_dir` | `true` if `k` is the root or lies strictly above a member's prefix (today's rule), else `true` if **any** covering member's `is_dir` is `true` (first `true` wins, errors propagate), else, when `k` is a member's prefix, `true` unless the owner's `contains(k)` is `true` (it then holds data there), else `false` | AC-10, AC-11, AC-3 |
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
- **A member's own prefix is a directory unless its owner holds data there** (decision 6, option A,
  revised in Step 4). Composition asks each child's owner, and an empty member does not report its
  own prefix as a directory, so a parent containing an empty nested mount failed `get_metadata`.
  Declaring every mount point a directory unconditionally would make a member mounted at a single
  file a directory too; the `contains` check keeps it a file. One extra `contains`, only for a key
  equal to a member's prefix.
- **Fallback on `KeyNotFound` only.** Any other member error is a backend failure (§4) and propagates.
- **Data wins over a derived directory.** If the owner holds data at `k` and another member is mounted
  beneath `k`, `get_metadata` / `get_asset_info` return the owner's data answer, as today; `is_dir`
  answers `true` and `listdir` lists the mount. Recorded, not resolved: such a layout is a
  configuration error and no store refuses it today.
- Rejected: a new trait method (`AsyncStore::contributes`) — the router is the only composer.
  Rejected: changing `AssetManager` to skip `contains` — it would fix AC-7's symptom and leave the
  router breaking §4 for every other caller.

## Known-Issue Preflight

| Issue | Status | Priority | Impact on this design | Fix first? | Blocking? | Action |
|---|---|---|---|---|---|---|
| `STORE-ROUTER-DOES-NOT-FORWARD-ASSET-INFO` | draft | P2 | Source issue | — | No | Closed by this design |
| `STORE-ROUTER-IS-DIR-IGNORES-IS-SUPPORTED` | draft | P3 | Folded in (AC-10) | — | No | Closed by this design |
| `STORE-ROUTER-LISTS-AN-UNCREATED-MEMBER-PREFIX` | draft | P3 | Blocks AC-9 for empty nested mounts (found in Step 4) | Folded in | Yes, resolved | Closed by this design (AC-11) |
| `design/store-adapters/` | in review | — | Assumes this (its AC-8) | — | No | Unblocked by this |
| `STORE-NO-READ-ONLY-ADAPTER`, `STORE-METADATA-HANDLING-NOT-COMPOSABLE` | open | P2 | Stores, not the router's dispatch | No | No | None |

## Interfaces

All private to `liquers-core/src/store.rs`; no `pub` item changes, no new types.

```rust
// Module-level, shared by `AsyncStore::listdir_asset_info` (default) and the router.
fn sort_asset_info(info: &mut [AssetInfo]);

impl AsyncStoreRouter {
    /// Root, or strictly above a member's prefix. Sync: prefixes only.
    fn is_above_members(&self, key: &Key) -> bool;
    /// The only contributor to `key`, when it is also `find_store(key)`.
    fn sole_contributor(&self, key: &Key) -> Option<&dyn AsyncStore>;
    /// Directory record built by the router, as `get_metadata`'s above-members branch builds it
    /// today; extracted so `get_asset_info` shares it.
    fn directory_record(key: &Key) -> MetadataRecord;
}
// `impl AsyncStore for AsyncStoreRouter`: new overrides `get_asset_info`, `listdir_asset_info`;
// changed `is_dir`, `contains`, `get_metadata`.
```

Ownership: `sole_contributor` returns `self.stores[i].as_ref()`; nothing is cloned. Sync/async: the
helpers are synchronous prefix comparisons; every trait method stays `async` under the existing
`async_trait` / `?Send` (wasm32) attributes. Commands: none. Value types: none.

## Integration Points

- `liquers-core/src/store.rs`: `AsyncStore::listdir_asset_info` (default) calls `sort_asset_info`;
  `impl AsyncStore for AsyncStoreRouter` as above.
- Callers that see the change: `AssetManager::get_asset_info` / `listdir_asset_info`
  (`liquers-core/src/assets.rs`, through `contains` and `get_asset_info`), the axum store and assets
  APIs, `liquers-web`'s builder, `liquers-store`'s `create_router*` — all through the trait, unchanged.
- Comments that describe the old `is_dir` selection: the module doc of
  `liquers-core/src/store_conformance/rules/prefix.rs`, and the doc comments of
  `router01_a_prefixed_store_does_not_claim_the_whole_router` and
  `opendal_router_is_dir_reaches_a_prefixed_store` in `liquers-store/src/opendal_store.rs`.

## Error Handling

No new error kinds. `KeyNotFound` from `Error::key_not_found(key)` as today. The owner fallback is
`Err(e) if e.error_type == ErrorType::KeyNotFound` on a `Result`, followed by `Err(e)` — a match on
`Result`, not a default arm on a Liquers enum. Every other member error propagates (§4: absence is
not an error, a backend failure is). `contains` no longer returns `KeyNotSupported`.

`get_metadata`'s children replacement matches `Metadata::MetadataRecord(record)` (replace when
`record.is_dir`) and `Metadata::LegacyMetadata(_)` (returned unchanged: no typed `children`).

## Relevant Commands

None. No command namespace is involved; `specs/command_registry.yaml` does not change.

## Documentation Architecture

- `specs/reference/STORE_SEMANTICS.md` (reference, store implementers and callers, `core/store`) §6:
  replace "`is_dir` and `listdir` select on `key_prefix()` alone" with the router rules above —
  forwarding, contributors, union `is_dir`, `contains` above and outside members. History row,
  `reviewed:` bump.
- `specs/guides/STORE_IMPLEMENTATION_GUIDE.md` (guide) §Prefix bullet and §9 router row: same
  correction. History row, `reviewed:` bump.
- `affects_docs: [STORE_SEMANTICS, STORE_IMPLEMENTATION_GUIDE]`. `specs/README.md`: the design's row
  is generated by `scripts/docs_index.py`.

## Risks

| Assessment | Finding |
|---|---|
| Files likely to change | `liquers-core/src/store.rs`; tests in `liquers-core/tests/`; comments in `store_conformance/rules/prefix.rs` and `liquers-store/src/opendal_store.rs`; the two documents above |
| Crates and workflows affected | `liquers-core` only in code; every environment with a router at run time |
| Existing tests likely to change | None expected. `router_unowned_key_metadata_not_found`, `C3` and the OpenDAL router tests must keep passing; tests asserting `KeyNotSupported` from a router's `contains` would change (none found) |
| New validation | Router unit tests per scenario, an `AssetManager` test, conformance suite `C6` |
| Compatibility / data / concurrency / performance / security | `contains` answers `Ok(false)` where it errored, so `AssetManager` reaches the recipe provider for unowned keys. `is_dir` asks one more member per additional covering member on a miss (deployments have one or two). Non-sole `get_metadata` discards the owner's children and lists again — only when members nest. No data or concurrency change |
| Recovery | One file of code; revert the commit |
| Certainty and open questions | High; none open |
