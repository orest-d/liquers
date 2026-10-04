# Phase 1: High-Level Design - Folder-Cache Invalidation for `ManifestRecipeProvider`

## Design Readiness

- **Readiness:** needs-decision
- **Leading issue:** **Open design question - freshness contract:** which writes a running
  provider is guaranteed to notice. Recommended: every write made through a Liquers API (asset
  manager, HTTP Store API); writes made behind Liquers' back (editing the directory on disk) are
  not noticed until the cache is cleared or the process restarts.
- **Explanation:** A working, event-driven design exists on top of a hook the asset manager
  already calls after every mediated write (`refresh_listing_version`); what remains open is
  whether that contract is enough or a time bound is also wanted, which changes observable
  behaviour for operators.
- **Open questions:**
  1. **Open design question - freshness contract.** *Recommended:* event-driven invalidation
     only (Liquers-mediated writes are seen at once; out-of-band writes need `clear_cache` or a
     restart), documented. *Alternative A:* add a TTL so out-of-band writes are seen within a
     bound — needs a clock that works on wasm32 (`std::time::Instant` panics there), and costs a
     `listdir` per folder per TTL even when nothing changes. *Alternative B:* a store-level
     directory version (`AsyncStore` change, `core/store` scope) — the general fix, much larger.
  2. **Open design question - Store API hook:** the HTTP Store API (`liquers-axum/src/store/
     handlers.rs`) writes straight to the store, bypassing the asset manager. *Recommended:*
     have its write/delete handlers call the provider hook for `key.parent()`, because "upload a
     new manifest to a running server" is the issue's scenario. *Alternative:* treat Store API
     writes as out-of-band (documented), keeping `liquers-axum` untouched.
  3. **Proposed resolution - scope of one invalidation:** a change notice for directory `d`
     drops cached state for `d` **and its subtree** (folder listings and parsed manifests), so a
     `removedir` — which notifies only the removed directory's parent — still clears the removed
     folders.

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
4. (Recommended Q2) The same holds for writes and deletes through the HTTP Store API.
5. `ManifestRecipeProvider::clear_cache()` drops everything, for hosts reacting to out-of-band
   changes.
6. A lookup in an unchanged folder still costs no `listdir` (the cache still works).
7. Providers without caches are unaffected; the hook's default is a no-op.

## Affected Systems

`liquers-core` (recipe-provider trait, chain, asset manager hook), `liquers-records` (provider),
`liquers-axum` (store handlers, if Q2 recommended). No query, data-format or store change.

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
- Out-of-band writes remain stale by design under the recommended contract; that must be stated
  where hosts read about the provider, not only in a doc comment (the issue's third option).
- Concurrency: an invalidation racing a `manifest_names` fill can re-insert a listing read just
  before the write. Close it by re-checking after the fill (a generation counter per provider,
  bumped by invalidation; a fill only inserts if the generation is unchanged).

## Review

Feasible with existing events; decisions are about the guarantee offered, with a recommended
answer that needs no clock and no store change.
