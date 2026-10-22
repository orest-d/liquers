---
id: CORE-ASSET-GC
kind: issue
title: Assets are never garbage collected
status: accepted
priority: P3
complexity: L
area: [core/assets, core/store]
design: asset-garbage-collection
created: 2026-08-08
github:
---
## Problem

Nothing removes assets that are no longer referenced. `WeakAssetRef` exists and the expiration
monitor evicts on expiry, but an asset that simply stops being wanted persists in the cache and in
the store.

## Problem Example

```rust
for i in 0..100_000 {
    m.get_asset(&parse_query(&format!("hello-{i}"))?).await?.get().await?;   // handle dropped
}
// query_assets now holds 100 000 assets and their values, for the life of the process.
```

## Impact

Unbounded growth in any long-running deployment. Expiration bounds *stale* data, not *unwanted*
data.

## Expected behaviour

A collection policy — reachability, or age plus a low-water mark — with explicit semantics for what
happens to a stored representation when its in-memory asset is collected.

Wants a design: what makes an asset unreachable is not obvious once recipes can name it by key.

## Update, 2026-10-10 (`plan-policy`)

The maintainer added a requirement while designing the cache strategies in `plan-policy`: a
global limit on the **size** of the cached data. For example, an internet-facing service bounds its
in-memory cache in bytes, separately for ad-hoc queries and for recipes. `plan-policy` decides
*whether* a value is kept (the `assets.recipe_cache_strategy` / `assets.query_cache_strategy`
settings, a command's `cached: false`). How much is kept, and what is evicted first, belongs here.
The size limit needs a size estimate per value, which `ValueInterface` does not provide today.

## Discovery

Migration triage, 2026-08-08. Source: work packages WP-18/19. Verified against HEAD: no GC mechanism exists. See `specs/archive/2026-08-08-docs-migration-plan.md` §4.0c.

## Update, 2026-10-21 (bulk design triage)

Still reproduces at HEAD (`18732dd`): the maps of `DefaultAssetManager` and
`ImmediateAssetManager` lose entries only by expiry, `remove`, replacement or volatility eviction.
Design: [`asset-garbage-collection`](../design/asset-garbage-collection/). It answers "what makes
an asset unreachable" (only the manager's map holds it; a recipe naming its key does not keep it
alive) and bounds the maps by count and idle time. The byte budget and store pruning are split off
as `VALUE-HAS-NO-MEMORY-SIZE-ESTIMATE` and `STORED-DERIVED-VALUES-ARE-NEVER-PRUNED`.
