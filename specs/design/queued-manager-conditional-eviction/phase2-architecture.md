# Phase 2: Solution and Architecture - Conditional Queued-Manager Cache Eviction

## Overview

Reuse `scc::HashMap::remove_if_async` for queued-manager cache eviction. Keep the existing
`remove_key_asset_if` helper for key assets (`assets.rs` ≈7568), add the query-map counterpart, and
replace the open-coded lookup/compare/drop/remove sequences at ≈6497, ≈6505 and ≈6855:

```rust
/// Removes `query`'s entry only if it is still the asset `asset_id`, under one bucket lock.
async fn remove_query_asset_if(&self, query: &Query, asset_id: u64) -> bool {
    self.query_assets
        .remove_if_async(query, |asset| asset.id() == asset_id)
        .await
        .is_some()
}
```

A private inherent method on `DefaultAssetManager`; no trait change. The key branch keeps taking
`key_mutation_lock` around `remove_key_asset_if`, preserving today's lock order.

## Known-Issue Preflight

| Issue | Status | Priority | Relevance and action | Blocking? |
|---|---|---|---|---|
| `ASSETS-IMPROVEMENTS` | accepted | P2 | Broader asset persistence and eviction work overlaps conceptually but is larger. This repair is an isolated race fix. | no |
| `ENVIRONMENT-MANAGER-REFERENCE-CYCLE` | draft | P2 | Same asset manager area, but lifetime leaks do not affect conditional map removal. | no |

## Files and Symbols

Primary file: `liquers-core/src/assets.rs`. Symbols: `DefaultAssetManager::remove_key_asset_if`,
the `query_assets: scc::HashMap<Query, AssetRef<E>>` field, `remove_expired_from_maps`,
`get_asset` query eviction, and `get` key eviction. Tests should extend existing asset-manager
unit tests near `remove_key_asset_if_respects_id`.

## Data, Ownership, Serialization and Errors

No data format changes. The helper takes borrowed `&Key` or `&Query` plus copied `u64` asset id and
returns `bool`. Removal errors remain ignored where current code deliberately best-effort evicts.

## Sync, Async and API Effects

The queued manager is async and uses `scc`; conditional removal must stay async. No trait or public
API changes are required if helpers remain private methods.

## Alternatives

Rejected: hold an entry guard across unrelated async expiration work; this can increase lock scope
and is not necessary. Rejected: serialize all cache mutations behind a global lock; too broad for a
localized race.

## Risk Assessment

| Assessment | Record |
|---|---|
| Files | 1 source/test file (`assets.rs`) plus specs/index. |
| Impact area | Default queued asset manager cache cleanup. |
| Module/crate reach | Confined to `liquers-core::assets`. |
| Existing-test breakage | None expected; existing eviction behaviour should still remove stale entries. |
| New validation | Unit test proving stale id removal does not evict a replacement for key and query maps where reachable. |
| Behavioural risk | Concurrency correctness improves; no persistence, performance or security regression expected. |
| Recovery | Revert helper use to previous remove path; no migration. |
| Certainty | High; both map types and the current identity-test pattern are present at HEAD. |

## Rust Review

The design uses borrowed keys/queries and copied ids, avoids clones except for map-owned keys, keeps
async primitives async, and introduces no new trait bounds or error type.
