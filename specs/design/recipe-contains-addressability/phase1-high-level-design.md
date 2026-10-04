# Phase 1: High-Level Design - `AsyncRecipeProvider::contains` Answers Addressability

## Design Readiness

- **Readiness:** needs-decision
- **Leading issue:** **Open design question - trait shape:** keep a default `contains` that
  delegates to `recipe_opt` (non-breaking), or remove the default so every implementer must write
  one (the issue's suggestion; a breaking change for out-of-tree providers).
- **Explanation:** Either option makes `contains` answer "can this key be produced" instead of
  "is this key listed", and both are fully specified below; the choice is a public-API
  compatibility decision, so it stays visible.
- **Open questions:**
  1. **Open design question - trait shape.** *Recommended:* change the default body to
     `Ok(self.recipe_opt(key, envref).await?.is_some())`. Consequence: correct for every provider
     whose `recipe_opt` is correct (the one method a provider cannot get wrong without being
     broken anyway), no downstream breakage, consistent with CLAUDE.md "add new methods with
     default implementations when possible / prefer extending traits over modifying them". Cost:
     a provider whose `recipe_opt` is expensive pays that cost in `contains` until it overrides.
     *Alternative:* make `contains` required. Consequence: forces each author to decide (the
     issue's argument by analogy with no-`_ =>` matches) and breaks compilation of every
     out-of-tree provider (e.g. the `orest-d/stockplottertest` prototype) and five in-tree impls.
  2. **Proposed resolution - documentation of addressable ⊋ listed:** state in the recipes
     reference that `assets_with_recipes` is what to *show* and `contains`/`recipe_opt` what can
     be *produced*, and that the asset key space may exceed the store key space. No alternative.

## Problem and Evidence

`AsyncRecipeProvider::contains` (`liquers-core/src/recipes.rs` ≈550) defaults to
`has_recipes(parent)` then a search of `assets_with_recipes(parent)`: it **enumerates**. A
provider that can produce keys it deliberately does not list (pattern/template-generated names,
conversion on demand) silently answers `false`. `ManifestRecipeProvider`
(`liquers-records/src/provider.rs` ≈378) had to override it for exactly this reason, as does
`RecipeProviderChain` (≈1048). `AssetManager::contains` (`assets.rs` ≈5216) and the manager's
recipe probe (≈5206) consult it, so a wrong `false` makes an addressable key look absent.

## Expected Behaviour and Acceptance Criteria

1. For any provider, `contains(key) == recipe_opt(key).is_some()` unless the provider overrides
   `contains` with a cheaper equivalent.
2. A provider that produces an unlisted key (pattern provider) reports `contains == true` with
   no override.
3. `TrivialRecipeProvider`, `DefaultRecipeProvider`, `RecipeProviderChain`,
   `ManifestRecipeProvider` give the same answers as today for every key they list.
4. Errors from `recipe_opt` (e.g. malformed `recipes.yaml`) propagate from `contains`, as the
   enumerating default's `get_recipes` errors do today.
5. The recipes reference states the listed/addressable distinction.

## Affected Systems

Recipe providers (core, records, tests), `AssetManager::contains`, validation tooling reading
providers. No query, store or serialization change.

## Scope and Non-Goals

In scope: the default (or its removal), in-tree provider adjustments, tests, reference text.
Non-goals: changing `assets_with_recipes` semantics, `has_recipes`, or the store contract
(`STORE_SEMANTICS.md` §2 constrains stores, not recipe providers).

## Compatibility

Recommended option: source compatible; behaviour changes only for a provider whose listing and
`recipe_opt` disagree — which is the bug. Alternative: compile break for every implementer
without a `contains`.

## Documentation Assessment

`reference/api/DOC_08_RECIPES_PLANS.md` (provider contract) — add the distinction (History row,
`reviewed:`). `reference/ASSETS.md` review for "listed implies exists" statements.
Close the issue.

## Design Dependencies

- `overlaps` `record-streams` (complete): built the first unlisted-but-addressable provider and
  its tests; no change to it, and its override stays valid (cheaper and equivalent).
- `overlaps` `manifest-folder-listing-invalidation`: same provider, independent change.

## Consolidated Findings

- The issue frames the fix as "remove the default", but the defect is *what the default
  computes*, not that a default exists. Delegating to `recipe_opt` removes the enumerability
  assumption without breaking anyone, so it is recommended; the required-method option remains
  fully specified as the alternative.
- `DefaultRecipeProvider`'s current answer equals `recipe_opt(...).is_some()` exactly:
  `RecipeList::get` matches on the same `recipe.filename()` the listing uses (`recipes.rs` ≈807).
- Under the recommendation no in-tree impl needs editing; under the alternative, five do
  (`TrivialRecipeProvider`, `DefaultRecipeProvider`, and test providers in `recipes.rs` ≈2058,
  `plan.rs` ≈2938, `liquers-core/tests/stored_cached_flags.rs` ≈102).
- Performance: `DefaultRecipeProvider::recipe_opt` reads `recipes.yaml` once, the old default
  read it once too (`has_recipes` + `assets_with_recipes`). No regression for the shipped
  provider.

## Review

Feasible either way; recommended option is the smaller and compatible one.
