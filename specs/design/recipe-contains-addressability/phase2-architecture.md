# Phase 2: Solution and Architecture - `AsyncRecipeProvider::contains` Answers Addressability

## Chosen Solution (recommended option)

`liquers-core/src/recipes.rs`, trait `AsyncRecipeProvider<E>`:

```rust
/// Returns whether the complete asset `key` can be produced by this provider.
///
/// This is *addressability*, not *listing*: a provider may produce keys that
/// [`Self::assets_with_recipes`] deliberately does not list (generated or on-demand names), and
/// such keys must answer `true` here. The default asks [`Self::recipe_opt`]; override it only with
/// a cheaper test that gives the same answer.
async fn contains(&self, key: &Key, envref: EnvRef<E>) -> Result<bool, Error> {
    Ok(self.recipe_opt(key, envref).await?.is_some())
}
```

No other code changes. `RecipeProviderChain::contains` (any member) and
`ManifestRecipeProvider::contains` (pattern match, same as its `recipe_opt` lookup) keep their
overrides.

## Alternative (if the decision is "required method")

Remove the default body. Add:

| Impl | Body |
|---|---|
| `TrivialRecipeProvider` | `Ok(false)` |
| `DefaultRecipeProvider` | `Ok(self.recipe_opt(key, envref).await?.is_some())` |
| `MockProvider` (`recipes.rs` tests ≈2058) | same as above |
| `CountingRecipeProvider` (`plan.rs` tests ≈2938) | same — note it counts calls; check its assertions count `recipe_opt` |
| `TaggedRecipeProvider` (`liquers-core/tests/stored_cached_flags.rs` ≈102) | same |

and a CHANGELOG/History note that out-of-tree providers must add `contains`.

## Rejected Alternatives

- **Keep the enumerating default and only document it.** Leaves the silent `false`.
- **Default = `has_recipes(parent) && recipe_opt(key).is_some()`.** `has_recipes` is itself a
  listing-shaped question for some providers; `recipe_opt` alone is the definition.

## Files and Symbols

`liquers-core/src/recipes.rs`: `AsyncRecipeProvider::contains` (default body and doc), trait doc
paragraph distinguishing listing/addressability; `mod tests` new tests.
`specs/reference/api/DOC_08_RECIPES_PLANS.md`: contract text.

## Errors, Ownership, Sync/Async

`recipe_opt` errors propagate (`?`); `Ok(None)` → `false`. Async, `async_trait` (Send on native,
`?Send` on wasm — the trait's existing attributes). `envref` moved into the single call.

## Compatibility

Recommended: source compatible. A provider that relied on the default and whose `recipe_opt`
returns `Some` for an unlisted key changes from `false` to `true` — the intended fix. A provider
whose `recipe_opt` errors where its listing did not now errors in `contains`: acceptable and
documented.

## Interactions

`AssetManager::contains` and the recipe probe at `assets.rs` ≈5206 become correct for
unlisted-producible keys. `liquers-validate` with a recipe overlay (`VALIDATE-CANNOT-SEE-NON-
STANDARD-RECIPE-PROVIDERS`) is unaffected.

## Questions

- **Open design question - trait shape:** Phase 1.
- **Implementation detail - doc placement:** trait-level doc gets one paragraph; method doc as
  above.

## Risk Table

| Aspect | Assessment |
|---|---|
| Likely files | `liquers-core/src/recipes.rs`; one reference doc |
| Affected workflows/crates | any `contains` on a key served by a default-`contains` provider |
| Existing-test impact | none expected (Default/Trivial answers unchanged); `CountingRecipeProvider` call-count assertions to be re-run |
| New validation | pattern provider without override; default-provider parity; error propagation |
| Compatibility/data | none (recommended); compile break (alternative) |
| Concurrency/performance | one `recipe_opt` per `contains`; equal cost for `DefaultRecipeProvider` |
| Security | none |
| Recovery | restore the old default body |
| Certainty | high |

## Review

Against Phase 1: criteria 1-2 by construction; 3 by `RecipeList::get` equivalence and existing
overrides; 4 by `?`; 5 by the reference edit. Against code: all seven provider impls located and
their `contains`/`recipe_opt` read at HEAD.
