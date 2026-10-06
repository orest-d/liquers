# Phase 2: Solution and Architecture - Recipe-Provider Listing Contract

## Overview

Three coordinated changes, implemented in this order:

- **Part A** splits "listed" from "producible" at the provider (`contains` / `can_make`) and at the
  asset manager, and moves every guard that precedes describing or producing a key to `can_make`.
- **Part B** adds an invalidation hook to the provider contract. The asset manager calls it from
  `refresh_listing_version`, which every manager-mediated write already calls, and the HTTP Store
  API calls that same method after its own writes. `ManifestRecipeProvider` uses it to drop folder
  listings, with a race guard that is correct by construction.
- **Part C** rebuilds `AssetManager::listdir_keys_deep` on the manager's own shallow `listdir`, so
  the deep listing contains the shallow one at every store directory, and removes the duplicated
  `DefaultAssetManager` overrides so both managers share one implementation.

## Known-Issue Preflight

| Issue | Status / priority | Relevance | Blocking? |
|---|---|---|---|
| `RECIPE-CONTAINS-DEFAULT-ASSUMES-ENUMERABILITY` | draft, P2 | source (Part A) | — |
| `MANIFEST-PROVIDER-FOLDER-LISTING-NEVER-REFRESHES` | draft, P2 | source (Part B) | — |
| `ASSET-MANAGER-LISTDIR-KEYS-DEEP-OMITS-TOP-LEVEL-RECIPES` | draft, P2 | source (Part C) | — |
| `SAVE-TO-STORE-REPORTS-CANCELLED-WRITE-AS-PERSISTED` | draft, P2 (design `save-to-store-skip-outcome`) | `save_to_store` calls `refresh_listing_version` only after a real write; that design keeps the call on its `Written` path. Either order works | no |
| `ENVIRONMENT-MANAGER-REFERENCE-CYCLE` | draft, P2 | same asset-manager area; lifetimes, not listing semantics | no |

## Data Structures

`ManifestRecipeProvider` (`liquers-records/src/provider.rs`) gains one field:

```rust
pub struct ManifestRecipeProvider {
    manifests: scc::HashMap<Key, CachedManifest>,
    folders: scc::HashMap<Key, Arc<Vec<String>>>,
    /// Bumped by every invalidation. A folder listing read while it moved is not kept: it may
    /// predate the write that caused the invalidation.
    generation: std::sync::atomic::AtomicU64,
}
```

`AtomicU64` is available on `wasm32-unknown-unknown`. No serialized type changes. The axum
response for the new route is `{ "can_make": bool }`, a sibling of the existing `ContainsResult`.

## Trait Implementations

### `AsyncRecipeProvider<E>` (`liquers-core/src/recipes.rs`)

| Method | Default | `Trivial` | `Default` | `RecipeProviderChain` | `ManifestRecipeProvider` |
|---|---|---|---|---|---|
| `contains` | listing search (unchanged body) | default | default | **new body:** any member's `contains` | **override deleted**; the default lists explicit chunks |
| `can_make` (new) | `recipe_opt(key).is_some()` | default | default | **moved body:** today's `contains` (`provider_index_for(..).is_some()`) | default (today's override body is exactly the default) |
| `directory_changed` (new) | no-op | default | default | forwards to every member, in order | drops `folders` entries under the directory |

`DefaultRecipeProvider` gives equal `contains` and `can_make` answers because `RecipeList::get`
matches on the same `recipe.filename()` its listing uses. Test providers (`recipes.rs` ≈2058,
`plan.rs` ≈2938, `liquers-core/tests/stored_cached_flags.rs` ≈102) keep the defaults.

### `AssetManager<E>` (`liquers-core/src/assets.rs`)

| Method | Change |
|---|---|
| `contains` | doc only: "stored, or listed by the recipe provider". The identical `DefaultAssetManager` override (≈7472) is deleted |
| `can_make` (new, default) | stored, or `provider.can_make` |
| `get_asset_info` (≈5206) | `rp.contains` → `rp.can_make` |
| `refresh_listing_version` (≈5552) | first statement calls `provider.directory_changed(dir)`, **before** the early return |
| `listdir_keys_deep` (≈5300) | rebuilt (below); the `DefaultAssetManager` override (≈7517) is deleted |
| `keys`, `listdir`, `listdir_keys` | `DefaultAssetManager`'s overrides (≈7482-7515) have the trait default's body; delete them so one implementation serves both managers (C5) |

## Sync vs Async

Every new method is async under the trait's existing `async_trait` attributes (`?Send` on wasm).
`directory_changed` returns `()`: a map removal cannot fail, and the write that triggered it must not
fail because of it. No lock is held across an `.await`. The race guard uses an atomic counter rather
than a lock, so lookups stay lock-free.

## Function Signatures

```rust
// liquers-core/src/recipes.rs, trait AsyncRecipeProvider<E>
/// Whether `key` is among the assets its directory **shows** — the entries
/// [`Self::assets_with_recipes`] lists for `key.parent()`. See [`Self::can_make`].
async fn contains(&self, key: &Key, envref: EnvRef<E>) -> Result<bool, Error>; // body unchanged

/// Whether this provider can **produce** `key`: every key [`Self::contains`] reports, plus keys
/// made on demand without being listed (template-generated names). Must be `true` whenever
/// `contains` is. Override only with a cheaper test giving the same answer.
async fn can_make(&self, key: &Key, envref: EnvRef<E>) -> Result<bool, Error> {
    Ok(self.recipe_opt(key, envref).await?.is_some())
}

/// The contents of directory `dir`, or of something below it, changed. Called by the asset
/// manager after every write or removal it mediates, with the written key's parent. A provider
/// caching directory-derived state drops it for `dir` and its subtree. The default does nothing.
async fn directory_changed(&self, dir: &Key) {
    let _ = dir;
}

// liquers-core/src/assets.rs, trait AssetManager<E>
/// Whether `key` can be got: stored, or producible by the recipe provider (listed or not).
async fn can_make(&self, key: &Key) -> Result<bool, Error> {
    let store = self.get_envref().get_async_store();
    if store.contains(key).await? {
        return Ok(true);
    }
    self.get_recipe_provider().can_make(key, self.get_envref()).await
}

/// Every key under `key`: for each store directory `d` in the subtree, everything
/// [`Self::listdir_keys`] reports for `d`, recipe-declared keys included. Never `key` itself.
async fn listdir_keys_deep(&self, key: &Key) -> Result<Vec<Key>, Error> {
    let store = self.get_envref().get_async_store();
    let mut keys = BTreeSet::new();
    let mut pending = vec![key.clone()];
    while let Some(dir) = pending.pop() {
        for name in self.listdir(&dir).await? {
            let child = dir.join(&name);
            if store.is_dir(&child).await? {
                pending.push(child.clone());
            }
            keys.insert(child);
        }
    }
    Ok(keys.into_iter().collect())
}

// liquers-records/src/provider.rs, impl ManifestRecipeProvider
/// Drops every cached manifest and folder listing, for a host reacting to a change made behind
/// Liquers' back (directly on disk). Changes made through Liquers need no call.
pub async fn clear_cache(&self) {
    self.generation.fetch_add(1, Ordering::SeqCst);
    self.folders.clear_async().await;
    self.manifests.clear_async().await;
}

// in impl AsyncRecipeProvider<E> for ManifestRecipeProvider
async fn directory_changed(&self, dir: &Key) {
    self.generation.fetch_add(1, Ordering::SeqCst);
    self.folders.retain_async(|folder, _| !folder.has_key_prefix(dir)).await;
}
```

**Race guard in `manifest_names`.** Load the generation *before* `store.listdir`; insert the listing;
then load the generation *again* and, if it moved, remove the entry just inserted, but only if it
is still the same `Arc` (`remove_if_async` with `Arc::ptr_eq`). All atomics use `SeqCst`. Why this
closes the race, and the former design's "insert only if unchanged" did not: an invalidation is
"bump, then retain". If its bump precedes the re-check, the fill removes its own entry. If the bump
follows the re-check, the insert already happened, so the invalidation's retain removes it. The
former check-then-insert left a window between the check and the insert in which a whole
invalidation could run and miss the entry.

**Why parsed manifests are not dropped.** `get_manifest` checks each cached manifest against the
store's metadata version on every hit, and evicts it when the store reports it absent
(`provider.rs` ≈118-135). Dropping `manifests` on every directory change would force a re-parse
after every chunk write, because stored chunks live in the manifest's own folder. That is a cost
with no correctness benefit.

**Deep listing.** `store.is_dir` decides descent, so a key that is both stored and recipe-declared
(a persisted computed value) is a file. Order of traversal is irrelevant; the result is a sorted
set. The loop with an explicit stack avoids boxed async recursion under `async_trait`.

## Integration Points

| Crate | File | Change |
|---|---|---|
| core | `src/recipes.rs` | `can_make`, `directory_changed`; trait-level doc paragraph naming "show" vs "produce" and `can_make ⊇ contains`; chain `contains` / `can_make` / `directory_changed` |
| core | `src/assets.rs` | `AssetManager::can_make`, `get_asset_info`, `refresh_listing_version`, `listdir_keys_deep`; delete `DefaultAssetManager`'s `contains`, `keys`, `listdir`, `listdir_keys`, `listdir_keys_deep` overrides after confirming each body equals the default |
| records | `src/provider.rs` | delete `contains` override; `generation`; guarded `manifest_names`; `directory_changed`; `clear_cache`; type doc comment's freshness contract replaces "for the life of the provider" |
| axum | `src/assets/common.rs` | ≈253 `provider.contains` → `provider.can_make`; `submit_key` (≈278) `manager.contains` → `manager.can_make` |
| axum | `src/assets/websocket.rs` | subscribe guards (≈261, ≈276) → `manager.can_make` |
| axum | `src/assets/key_handlers.rs`, `src/assets/builder.rs` | new `key_can_make_handler`; route `{b}/key/can_make/{*key}` beside `key/contains` (≈180) |
| axum | `src/store/handlers.rs` | private `refresh_after_write(&env, &key)` calling `env.get_asset_manager().refresh_listing_version(&key.parent())`, after each successful `set` (≈80, ≈576, ≈701), `set_metadata` (≈211), `remove` (≈114 and the `delete_entry` / `get_remove` paths), `makedir` (≈383) and `removedir` (≈414) |
| lib | `tests/records_manifest_refresh.rs` | new integration test (`#![cfg(feature = "records")]`) |

Routing the Store API through `refresh_listing_version` rather than calling the provider directly
keeps one notification path, and also refreshes registered listing versions after Store API
writes, which they skip today.

## Documentation Architecture

| Path | Kind / audience | Change |
|---|---|---|
| `specs/reference/api/DOC_08_RECIPES_PLANS.md` | reference, internal | provider contract: `contains` vs `can_make`, `can_make ⊇ contains`, `directory_changed`; chain table row for `contains` corrected (≈166) and `can_make` added; the manifest override paragraph (≈177) rewritten |
| `specs/reference/ASSETS.md` | reference, internal | `AssetManager::contains` / `can_make`; what `listdir_keys_deep` / `keys()` return; `removedir` now unmaps the directory's own recipe assets |
| `specs/reference/RECORD_STREAMS.md` | reference, internal | caches paragraph (≈364-368): the folder listing is event-driven, invalidated by mediated and Store API writes; `clear_cache`; template chunks are producible, not listed |
| `specs/guides/RECORD_STREAM_GUIDE.md` | guide, internal | uploading a manifest to a running server; `clear_cache` after editing files directly |
| `specs/reference/WEB_API_SPECIFICATION.md` | reference, internal | `key/can_make` row; `key/contains` meaning (stored or listed); remove the deep-listing caveat (≈294) |
| `specs/guides/WEB_API_GUIDE.md` | guide, internal | `key/can_make` beside the `key/contains` example (≈280) |
| `specs/guides/ASSET_MANAGER_IMPLEMENTATION_GUIDE.md` | guide, internal | a manager calls `refresh_listing_version` after every write, which now also notifies the provider |
| `specs/README.md` | map | capability map: link this design while in progress |

`affects_docs` in `DESIGN.md` lists these seven documents.

## Relevant Commands

None. No command is added or changed; `specs/command_registry.yaml` is unaffected. The behaviour is
reached through queries such as `-R/data/sales/daily_0042.csv` (a resource key: `-R/` consumes the
rest as the key).

## Error Handling

`can_make` propagates `recipe_opt` errors with `?`. `listdir_keys_deep` propagates `listdir` and
`is_dir` errors, as today. `directory_changed` and `clear_cache` cannot fail. The Store API handlers
call `refresh_listing_version` only after a successful write; its own `listdir` failure is already
reported with `eprintln!` and never undoes the write. No new error type; no `Error::new`.

## Rejected Alternatives

- **Keep `AssetManager::contains` meaning "can be got"** — rejected by the maintainer (2026-10-05).
- **Rename the manifest provider's `contains` override to `can_make`** (the former design) — its
  body is exactly the new default; keeping it is dead code.
- **A TTL, or a store-level directory version** — no reliable clock on wasm32; an `AsyncStore`
  change for every backend.
- **Drop parsed manifests on invalidation** (the former design) — redundant with the per-hit
  version check, and it forces a re-parse after every stored chunk write.
- **Insert the folder listing only if the generation is unchanged** (the former design) — a
  check-then-insert that leaves a window; replaced by insert-then-recheck.
- **Descend into directories only a recipe provider declares** (former criterion 5) — no provider
  declares directories; costs a `has_recipes` call per unstored recipe name.
- **Patch the old deep-listing algorithm to also add `key`'s own recipes** — fixes C3 but keeps two
  algorithms that must agree by hand, which is how the defect arose.

## Risk Table

| Aspect | Assessment |
|---|---|
| Likely files | `recipes.rs`, `assets.rs`, `provider.rs`, axum `common.rs`, `websocket.rs`, `key_handlers.rs`, `builder.rs`, `store/handlers.rs`, one lib test |
| Existing-test impact | records `contains_answers_for_a_template_name_far_beyond_any_listing` → `can_make`; core `contains_is_true_if_any_provider_has_the_recipe` → `can_make` (its `MockProvider` declares recipes but lists nothing); axum tests asserting `contains: true` for a template chunk, if any |
| Compatibility | out-of-tree providers compile unchanged; HTTP `key/contains` narrows; listings grow |
| Concurrency | race guard proven above; no lock across await |
| Performance | one `retain_async` over the small `folders` map per write; one `listdir` per folder after a write into it; no manifest re-parse |
| Recovery | each part reverts independently; both new provider methods have harmless defaults |
| Certainty | high |
