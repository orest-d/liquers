# Phase 1: High-Level Design - Recipe-Provider Listing Contract

## Design Readiness

- **Readiness:** ready
- **Leading issue:** None. The leading *source* is `RECIPE-CONTAINS-DEFAULT-ASSUMES-ENUMERABILITY`.
- **Explanation:** Every contract question was decided by the maintainer (2026-10-04, 2026-10-05).
  The post-Phase-4 review (2026-10-05) corrected two defects of the former manifest design and
  removed one speculative criterion of the former listing design. No question remains.
- **Open questions:** None

## Feature Name

Recipe-provider listing contract.

## Purpose

A recipe provider answers two separate questions: which keys a folder **shows** and which keys it
can **produce**. The asset manager mirrors that split. The folder cache of `ManifestRecipeProvider`
stays fresh after writes through Liquers. A deep listing contains the shallow listing of every
directory it walks.

## Problem and Evidence

| Part | Defect at HEAD (2026-10-05) |
|---|---|
| A | `AsyncRecipeProvider::contains` (`liquers-core/src/recipes.rs` ≈550) means "listed" by default, while the overrides in `ManifestRecipeProvider` (`liquers-records/src/provider.rs` ≈378) and `RecipeProviderChain` (`recipes.rs` ≈1048) mean "producible". A new on-demand provider inherits "listed", so its producible keys look absent |
| B | `ManifestRecipeProvider.folders` caches each folder's `*.manifest.yaml` names for the provider's lifetime (`provider.rs` ≈61-66). A manifest added or removed later is never noticed, so its chunks never resolve on a long-running server |
| C | `AssetManager::listdir_keys_deep` (trait default `assets.rs` ≈5300, identical override ≈7517) adds recipe keys only for stored *subdirectories*. `keys()` misses root-level recipes and `listdir(d) ⊄ listdir_keys_deep(d)` |

## Decision Record

| Question | Decision | Date, by |
|---|---|---|
| Provider trait shape | `contains(key)` = among what its folder shows (`assets_with_recipes`); new `can_make(key)` = can be produced. Both overridable; `can_make` defaults to `recipe_opt(key).is_some()`; `contains` keeps its default | 2026-10-04, maintainer |
| Asset manager and HTTP | Mirror the split. `AssetManager::contains` = stored or listed; new `AssetManager::can_make` = stored or producible. The guards before a `get` (axum `submit_key`, websocket subscribe) use `can_make`. HTTP `key/contains` keeps `contains`; a new `key/can_make` route reports `can_make` | 2026-10-05, maintainer |
| Freshness of the manifest folder cache | Event-driven only. Writes through Liquers are seen at once; out-of-band edits need `clear_cache` or a restart | 2026-10-04, maintainer |
| HTTP Store API writes | Notify too (uploading a manifest to a running server is the issue's scenario) | 2026-10-04, maintainer |
| What one invalidation drops | **Only the folder-name listings** of the directory and its subtree. Parsed manifests stay, because they are already version-checked on every hit and a removed manifest is evicted by that check | 2026-10-05, review |
| Deep listing over recipe-only directories | **Not descended.** Only store directories are walked; no provider in the repository declares directories, and descending would cost a `has_recipes` call per unstored recipe name | 2026-10-05, review |
| Merge of the three designs | One design, one implementation PR, Part A first | 2026-10-05, maintainer |

## Core Interactions

- **Recipes / providers:** `AsyncRecipeProvider` gains `can_make` and `directory_changed`, both
  with defaults; `RecipeProviderChain` forwards both.
- **Assets:** `AssetManager` gains `can_make`; `get_asset_info` and `refresh_listing_version` use
  the new provider methods; `listdir_keys_deep` is rebuilt on `listdir`.
- **Records:** `ManifestRecipeProvider` drops its `contains` override, adds `directory_changed` and
  `clear_cache`, and guards its folder cache against a fill racing an invalidation.
- **Axum:** the asset API guards use `can_make`; a new `key/can_make` route; the Store API write
  handlers notify through the asset manager.
- **Query / Store:** unchanged.

## Crate Placement

`liquers-core` (`recipes.rs`, `assets.rs`), `liquers-records` (`provider.rs`), `liquers-axum`
(`assets/`, `store/handlers.rs`), `liquers-lib` (one integration test only).

## Acceptance Criteria

**A — listed vs producible**

- A1. `AsyncRecipeProvider` has `contains` (listing search, unchanged) and `can_make`
  (`recipe_opt(key).is_some()`), both overridable, `can_make ⊇ contains`.
- A2. A provider that produces an unlisted key and overrides neither: `contains == false`,
  `can_make == true`.
- A3. `DefaultRecipeProvider` and `TrivialRecipeProvider`: `contains == can_make` for every key.
- A4. `ManifestRecipeProvider`: `contains` true only for explicit chunks; `can_make` true for
  explicit and template chunks.
- A5. `RecipeProviderChain`: `contains` = any member's `contains`; `can_make` = any member's
  `can_make`.
- A6. Describing (`get_asset_info`, axum unevaluated-key metadata) and submitting (axum
  `submit_key`, websocket subscribe) an unevaluated key use `can_make`, so a template chunk can be
  described, submitted and subscribed to.
- A7. Errors from `recipe_opt` propagate from `can_make`.

**B — folder-cache invalidation**

- B1. After a manifest is written into folder `d` through the asset manager, the next lookup in `d`
  sees it.
- B2. After a manifest is removed through the asset manager, its chunks stop resolving.
- B3. After `removedir(d)`, no folder listing under `d` remains cached.
- B4. B1-B3 also hold for writes and deletes through the HTTP Store API.
- B5. `ManifestRecipeProvider::clear_cache()` drops every cached listing and manifest.
- B6. A lookup in an unchanged folder costs no `listdir`; a write of a non-manifest file into a
  folder does not force any manifest to be re-parsed.
- B7. A listing fill that races an invalidation is never left in the cache.
- B8. Providers without caches are unaffected (no-op default).

**C — complete deep listings**

- C1. For every **store** directory `d` under (and including) `key`, every key of
  `listdir_keys(d)` is in `listdir_keys_deep(key)`.
- C2. The result contains no key outside `key`'s subtree and not `key` itself; it is sorted and
  duplicate-free.
- C3. `keys()` includes root-level recipe keys.
- C4. Every key in `listdir_keys_deep(key)` satisfies `AssetManager::contains` (stored or listed);
  manifest template chunks are not listed.
- C5. `DefaultAssetManager` and `ImmediateAssetManager` behave identically.
- C6. `removedir(d)` also removes the live assets of `d`'s own recipe keys (the same treatment its
  subdirectories' recipe keys already get).

## Scope and Non-Goals

Non-goals: changing `assets_with_recipes` or `has_recipes`; a store-level directory version; file
watching or a TTL; descending into directories that only a recipe provider declares; changing the
store contract (`STORE_SEMANTICS.md` constrains stores, not providers); `liquers-web` and
`liquers-py` (neither implements `AsyncRecipeProvider` nor calls `contains`, checked 2026-10-05).

## Compatibility

Source-compatible for out-of-tree providers: both new provider methods have defaults. Behaviour
changes, all intended: HTTP `key/contains` answers `false` for an unlisted producible key (a
template chunk) and `key/can_make` answers `true`; deep listings grow by the missing recipe keys;
`removedir` also unmaps the live assets of the directory's own recipes; manifest answers are fresh
after mediated writes; Store API writes now also refresh registered listing versions.

## Documentation Intent

- **Reference:** extend `reference/api/DOC_08_RECIPES_PLANS.md` (provider contract),
  `reference/ASSETS.md` (manager methods, deep listing), `reference/RECORD_STREAMS.md` (manifest
  provider freshness) and `reference/WEB_API_SPECIFICATION.md` (new route, changed semantics). No
  new reference: each fact has an existing owner.
- **Guide:** extend `guides/RECORD_STREAM_GUIDE.md` (uploading a manifest to a running server;
  `clear_cache` for out-of-band edits), `guides/WEB_API_GUIDE.md` (`key/can_make` example) and
  `guides/ASSET_MANAGER_IMPLEMENTATION_GUIDE.md` (what a manager must call after a write). No new
  guide.
- **Other documents:** none.
- **Updates:** the three source issues are closed; the README capability map links this design.

## Design Dependencies

| Relationship | Target | Effect |
|---|---|---|
| supersedes | designs `recipe-contains-addressability`, `manifest-folder-listing-invalidation`, `listdir-keys-deep-recipe-union` | their content is carried here, with the review's corrections |
| owns (leading) | issue `RECIPE-CONTAINS-DEFAULT-ASSUMES-ENUMERABILITY` | Part A |
| owns | issue `MANIFEST-PROVIDER-FOLDER-LISTING-NEVER-REFRESHES` | Part B |
| owns | issue `ASSET-MANAGER-LISTDIR-KEYS-DEEP-OMITS-TOP-LEVEL-RECIPES` | Part C |
| overlaps | design `record-streams` (complete) | built `ManifestRecipeProvider`; its `contains` test moves to `can_make` |
| overlaps | design `axum-assets-endpoints` (complete) | owns the routes and guards changed in A6 |
| overlaps | design `listdir-keys-deep-child-check` | the *store* default's recursion guard; same name, different code path |
| overlaps | design `save-to-store-skip-outcome` | the hook in `refresh_listing_version` fires from `save_to_store` only on a real write, which that design keeps |
| requires | — | nothing must land first |

## Open Questions

None.
