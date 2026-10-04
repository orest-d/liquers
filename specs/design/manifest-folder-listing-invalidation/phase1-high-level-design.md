# Phase 1: High-Level Design - Folder-Cache Invalidation for `ManifestRecipeProvider`

## Design Readiness

- **Readiness:** ready
- **Leading issue:** None
- **Explanation:** The maintainer accepted the recommended answers on 2026-10-04. The design is
  event-driven on top of a hook the asset manager already calls after every write it mediates
  (`refresh_listing_version`), so no clock and no store change is needed.
- **Open questions:** None

## Decision Record

| Question | Decision (2026-10-04) | Consequence |
|---|---|---|
| Freshness contract | **Event-driven only** (recommended answer accepted) | Writes through a Liquers API are seen at once; edits made behind Liquers' back (directly on disk) need `ManifestRecipeProvider::clear_cache` or a restart. Documented where hosts read about the provider. Rejected: a TTL (no reliable clock on wasm32, periodic `listdir` cost) and a store-level directory version (an `AsyncStore` change for every backend) |
| Store API hook | **Yes** (recommended answer accepted) | The HTTP Store API's write and delete handlers notify the provider for `key.parent()`, because uploading a new manifest to a running server is the issue's scenario |
| Scope of one invalidation | Directory **and its subtree** (proposed resolution, accepted with the rest) | `removedir`, which notifies only the removed directory's parent, still clears the removed folders |

## Problem and Evidence

`ManifestRecipeProvider` (`liquers-records/src/provider.rs`) caches per folder the list of
`*.manifest.yaml` names (`folders: scc::HashMap<Key, Arc<Vec<String>>>`, filled by
`manifest_names`) for the provider's lifetime. Its own doc comment records that a manifest added
to or removed from a folder after the first listing is never noticed. The parsed-manifest cache
(`manifests`) is version-checked per lookup, but a stale folder list means a new manifest is
never looked up at all: `contains`, `recipe_opt`, `assets_with_recipes` and `has_recipes` answer
as if it did not exist, silently, on a long-lived server.

The asset manager already has the right event: `AssetManager::refresh_listing_version(dir)`
(`liquers-core/src/assets.rs` ≈5552) is called after every manager-mediated write and removal
(10 call sites: `save_to_store`, `set`/`set_state` in both managers, `remove`, `removedir`,
`makedir`, …) — but returns early unless a listing dependency exists, and never tells the recipe
provider.

## Expected Behaviour and Acceptance Criteria

1. After a manifest is written into folder `d` through the asset manager, the next
   `contains`/`recipe_opt`/`assets_with_recipes` on `d` sees it.
2. After a manifest is removed through the asset manager, its chunks stop resolving and its
   name is no longer probed.
3. After `removedir(d)`, no cached listing or manifest under `d` remains.
4. The same holds for writes and deletes through the HTTP Store API.
5. `ManifestRecipeProvider::clear_cache()` drops everything, for hosts reacting to out-of-band
   changes.
6. A lookup in an unchanged folder still costs no `listdir` (the cache still works).
7. Providers without caches are unaffected; the hook's default is a no-op.

## Affected Systems

`liquers-core` (recipe-provider trait, chain, asset manager hook), `liquers-records` (provider),
`liquers-axum` (store handlers). No query, data-format or store change.

## Scope and Non-Goals

Non-goals: a store-level directory version; filesystem watching; TTL (unless chosen);
changing the per-manifest version check.

## Compatibility

New trait method with a default (CLAUDE.md: extend traits with defaults). New public
`clear_cache` on the provider. No behaviour change except fresher answers.

## Documentation Assessment

`reference/RECORD_STREAMS.md` / `guides/RECORD_STREAM_GUIDE.md` (where the provider is
described) — state the freshness contract; `reference/api/DOC_08_RECIPES_PLANS.md` — the new hook
in the provider contract. Update the provider's own doc comment. Close the issue.

## Design Dependencies

- `overlaps` `record-streams` (complete): built the provider and its caches; this changes only
  the folder cache's lifetime.
- `overlaps` `recipe-contains-addressability`: same trait, independent methods; either order.
- `overlaps` `listdir-keys-deep-recipe-union`: both touch asset-manager listing code paths;
  no ordering.

## Consolidated Findings

- Hook point: the top of `refresh_listing_version`, **before** its early return, because the
  provider must hear about every mediated write, not only those with listing dependents. Every
  mediated write already calls it, so no new call sites in the managers.
- The hook is a trait method with a no-op default, forwarded by `RecipeProviderChain` to every
  member (the chain is how `liquers-lib` installs the manifest provider).
- Invalidation drops the subtree, so `removedir`'s parent-only notification suffices.
- Drop the `manifests` entries in the subtree too: the issue notes the two caches are
  independent, and a removed manifest's parse would otherwise linger.
- Out-of-band writes remain stale by design under the decided contract; that must be stated
  where hosts read about the provider, not only in a doc comment (the issue's third option).
- Concurrency: an invalidation racing a `manifest_names` fill can re-insert a listing read just
  before the write. Close it by re-checking after the fill (a generation counter per provider,
  bumped by invalidation; a fill only inserts if the generation is unchanged).

## Review

Feasible with existing events; the guarantee offered was decided on 2026-10-04 and needs no
clock and no store change.
