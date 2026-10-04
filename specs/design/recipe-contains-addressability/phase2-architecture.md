# Phase 2: Solution and Architecture - `contains` (Listed) and `can_make` (Producible)

## Provider Trait — `liquers-core/src/recipes.rs`

```rust
/// Whether `key` is among the assets its directory **shows** — the entries
/// [`Self::assets_with_recipes`] lists for `key.parent()`.
///
/// Not the same as being producible: see [`Self::can_make`]. The default searches the listing.
async fn contains(&self, key: &Key, envref: EnvRef<E>) -> Result<bool, Error> {
    /* current default body, unchanged */
}

/// Whether this provider can **produce** `key`: every key [`Self::contains`] reports, plus keys
/// it makes on demand without listing them (template-generated names, conversions).
///
/// The default asks [`Self::recipe_opt`]. Override only with a cheaper test that gives the same
/// answer. Must be `true` whenever `contains` is.
async fn can_make(&self, key: &Key, envref: EnvRef<E>) -> Result<bool, Error> {
    Ok(self.recipe_opt(key, envref).await?.is_some())
}
```

Trait-level doc: one paragraph naming the two questions (show vs produce) and the subset rule.

## Providers

| Impl | `contains` | `can_make` |
|---|---|---|
| `TrivialRecipeProvider` | default (`has_recipes` is `false` → `false`) | default (`recipe_opt` is `None` → `false`) |
| `DefaultRecipeProvider` | default | default — equal answers, because `RecipeList::get` matches on the same `recipe.filename()` the listing uses (≈807) |
| `RecipeProviderChain` (≈1048) | **new body:** `true` if any member's `contains` is | **moved body:** `Ok(self.provider_index_for(key, envref).await?.is_some())` (today's `contains`) |
| `ManifestRecipeProvider` (`liquers-records/src/provider.rs` ≈378) | default (its `assets_with_recipes` lists explicit chunks) | **moved override:** today's `contains` body, doc comment updated to name `can_make` |
| test providers (`recipes.rs` ≈2058, `plan.rs` ≈2938, `liquers-core/tests/stored_cached_flags.rs` ≈102) | default | default |

## Callers Switched to `can_make`

| File | Site | Change |
|---|---|---|
| `liquers-core/src/assets.rs` | `AssetManager::get_asset_info` (≈5206) | `rp.contains` → `rp.can_make` |
| `liquers-axum/src/assets/common.rs` | unevaluated-key metadata (≈253) | `provider.contains` → `provider.can_make` |

## Asset Manager (recommended answer to the open question)

`trait AssetManager<E>` (`liquers-core/src/assets.rs`):

```rust
/// Whether `key` is stored, or listed by the recipe provider in its directory.
async fn contains(&self, key: &Key) -> Result<bool, Error> {
    /* store.contains || provider.contains — body unchanged */
}

/// Whether `key` can be got: stored, or producible by the recipe provider (listed or not).
/// A `true` here means `get(key)` has something to evaluate or load.
async fn can_make(&self, key: &Key) -> Result<bool, Error> {
    let store = self.get_envref().get_async_store();
    if store.contains(key).await? {
        return Ok(true);
    }
    self.get_recipe_provider().can_make(key, self.get_envref()).await
}
```

`DefaultAssetManager`'s `contains` override (≈7472) has the same body as the trait default; keep
it (or delete it in favour of the default — implementation detail). `ImmediateAssetManager`
uses the defaults.

`liquers-axum`:

| Site | Change |
|---|---|
| `assets/common.rs` `submit_key` (≈278) | `manager.contains` → `manager.can_make` |
| `assets/websocket.rs` subscribe (≈261, ≈276) | `manager.contains` → `manager.can_make` |
| `assets/key_handlers.rs` `key_contains_handler` (≈142) | unchanged — reports `contains` |
| `assets/key_handlers.rs` | new `key_can_make_handler`, same shape, `ContainsResult`-like `{ can_make: bool }` body |
| `assets/builder.rs` (≈180) | route `{b}/key/can_make/{*key}` beside `key/contains` |

**Under the alternative** (no manager split): `AssetManager::contains`'s provider call becomes
`can_make`, nothing is added at the manager or HTTP layer, and the axum guards stay as they are.

## Rejected Alternatives

- **One method, default via `recipe_opt`** (the earlier recommendation). Superseded by the
  maintainer's two-method decision, which keeps the listing question answerable.
- **Remove the default** of either method — breaks out-of-tree providers; not needed.
- **Default `can_make` = `contains || recipe_opt`** — redundant when `recipe_opt` covers listed
  keys, and doubles the work for the common case.

## Errors, Ownership, Sync/Async

`can_make` propagates `recipe_opt` errors (`?`). Same `async_trait` attributes as the rest of the
trait (`?Send` on wasm). No new error types.

## Questions

- **Open design question - asset-manager and HTTP naming:** Phase 1.
- **Implementation detail - keep or delete `DefaultAssetManager`'s identical `contains`
  override:** either.

## Risk Table

| Aspect | Assessment |
|---|---|
| Likely files | `recipes.rs`, `assets.rs`, `liquers-records/src/provider.rs`, axum `common.rs`, `websocket.rs`, `key_handlers.rs`, `builder.rs` |
| Affected workflows/crates | describing/submitting unevaluated recipe keys; manifest chunks; HTTP `key/contains` (recommended answer) |
| Existing-test impact | records `contains_answers_for_a_template_name_far_beyond_any_listing` → `can_make`; core chain test may need its mock to list the key; axum `key/contains` tests on template chunks (if any) change expectation |
| New validation | default split on an on-demand provider; parity for default provider; chain forwarding for both; manifest provider split; manager `can_make`; axum route and guards |
| Compatibility/data | out-of-tree providers compile unchanged; HTTP `key/contains` narrows (recommended answer) |
| Concurrency/performance | `can_make` costs one `recipe_opt`; equal to today's chain path |
| Security | none |
| Recovery | each layer reverts independently; `can_make` default is harmless |
| Certainty | high |

## Review

Against Phase 1: criteria 1-2 → trait; 3-5 → provider table; 6 → caller tables; 7 → `?`; 8 →
docs. Against code: all provider impls, both manager `contains`, `get_asset_info`, and the axum
callers and route were read at HEAD.
