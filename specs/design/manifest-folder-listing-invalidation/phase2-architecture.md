# Phase 2: Solution and Architecture - Folder-Cache Invalidation for `ManifestRecipeProvider`

## Chosen Solution (decided 2026-10-04)

### 1. Trait hook — `liquers-core/src/recipes.rs`

```rust
/// Tells the provider that the contents of directory `key` (or something below it) changed.
///
/// Called by the asset manager after every write or removal it mediates, with the written
/// key's parent. A provider that caches directory-derived state drops it for `key` and its
/// subtree. The default does nothing.
async fn directory_changed(&self, key: &Key) {
    let _ = key;
}
```

No `envref` parameter (a cache drop needs none) and no `Result` (a failed invalidation must not
fail the write that triggered it; there is nothing that can fail in a map removal).

`impl AsyncRecipeProvider<E> for RecipeProviderChain<E>`: forward to every provider in order.

### 2. Manager call — `liquers-core/src/assets.rs`

At the top of `AssetManager::refresh_listing_version(&self, dir)`:

```rust
self.get_recipe_provider().directory_changed(dir).await;
```

Doc comment updated: "Also tells the recipe provider, unconditionally". No other manager edit.

### 3. Provider — `liquers-records/src/provider.rs`

- New field `generation: AtomicU64` (`std::sync::atomic`; available on wasm32).
- `directory_changed(dir)`: `generation.fetch_add(1)`, then
  `folders.retain_async(|k, _| !k.has_key_prefix(dir)).await` and the same on `manifests`.
  (`Key::has_key_prefix` treats `dir` itself as a prefix of itself.)
- `manifest_names`: read `generation` before `store.listdir`; after it, insert only if the
  generation is unchanged (otherwise return the fresh names without caching). This closes the
  fill-after-invalidate race.
- `pub async fn clear_cache(&self)`: bump generation, clear both maps.
- Doc comment on the type: replace "cached for the life of the provider" with the freshness
  contract (mediated writes seen; out-of-band writes need `clear_cache`).

### 4. Store API — `liquers-axum/src/store/handlers.rs`

After each successful `store.set`, `store.set_metadata`, `store.remove`/`removedir` in the
handlers (≈80, ≈211, ≈576, ≈701 and the delete handlers), call
`env.get_recipe_provider().directory_changed(&key.parent()).await`. A small private helper
`notify_directory_changed(&env, &key)` keeps it to one line per site.

## Rejected Alternatives

- **TTL** — wasm clock problem and periodic cost (Phase 1, Alt A).
- **Store directory version** — `core/store` contract change for every backend (Alt B).
- **Call the hook from each manager write site** — `refresh_listing_version` already *is* that
  set of sites; duplicating would drift.
- **Return `Result` from the hook** — nothing to report; would force error handling into ten
  write paths.

## Files and Symbols

| Crate | File | Symbols |
|---|---|---|
| core | `src/recipes.rs` | `AsyncRecipeProvider::directory_changed` (new default), `RecipeProviderChain` impl |
| core | `src/assets.rs` | `AssetManager::refresh_listing_version` |
| records | `src/provider.rs` | `ManifestRecipeProvider` fields, `manifest_names`, `directory_changed`, `clear_cache`, docs, tests |
| axum | `src/store/handlers.rs` | write/delete handlers |

`liquers-lib/src/environment.rs` needs no change (it builds the chain).

## Ownership, Sync/Async, Errors

All async under the trait's existing `async_trait` attributes (`?Send` on wasm). `scc`
`retain_async` / `clear_async` exist in scc 3.x (used crate version 3.8.8). No errors produced.

## Compatibility

Additive trait method with default; out-of-tree providers unaffected. Observable change: fresher
answers after writes.

## Questions

- **Freshness contract** and **Store API hook**: decided (Phase 1 decision record).
- **Implementation detail - generation counter:** chosen over a lock to keep reads lock-free.

## Risk Table

| Aspect | Assessment |
|---|---|
| Likely files | four files across core, records, axum |
| Affected workflows/crates | every manager write (one extra no-op/async call); manifest lookups |
| Existing-test impact | records provider tests (caching assertions — `a_manifest_edit_with_a_new_stored_version_is_picked_up`, explicit-chunk listing tests) must stay green; core listing-version tests unchanged |
| New validation | add/remove manifest via manager; removedir subtree; Store API upload (axum); cache-hit still avoids `listdir` (counting store); race guard (sequential simulation) |
| Compatibility/data | none |
| Concurrency/performance | one hook call per write; retain is O(cache size) per write — caches are per-folder, small; generation guard for the fill race |
| Security | none |
| Recovery | default no-op hook makes reverting the provider part alone safe |
| Certainty | high |

## Review

Against Phase 1: criteria 1-3 via hook + subtree drop; 4 via axum helper; 5 `clear_cache`; 6 cache
retained when no notice; 7 default no-op. Against code: `refresh_listing_version`, the chain's
provider list, `manifest_names`, scc version, `Key::has_key_prefix`, and the axum handlers were
read at HEAD.
