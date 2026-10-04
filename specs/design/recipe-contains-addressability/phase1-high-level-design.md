# Phase 1: High-Level Design - `AsyncRecipeProvider::contains` Answers Addressability

## Design Readiness

- **Readiness:** needs-decision
- **Leading issue:** **Open design question - trait shape:** fix the provider's default
  `contains` to ask `recipe_opt` ("can you produce it") instead of searching the listing
  (non-breaking), or remove the default so every provider must write its own (the issue's
  suggestion; breaks every provider outside this repository).
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

There are three `contains`, one per layer:

| Layer | Method | Answers |
|---|---|---|
| Store | `AsyncStore::contains(key)` | is something **physically stored** at `key`? |
| Recipe provider | `AsyncRecipeProvider::contains(key)` | can the provider **make** `key` (does it have a recipe for it)? |
| Assets | `AssetManager::contains(key)` (`liquers-core/src/assets.rs` ≈5216) | store says yes, **or** the recipe provider says yes |

So the asset layer already learns the recipe half from the recipe provider; nothing about that is
wrong. The defect is one level down, in *how the provider answers by default*.

A provider has two separate questions to answer about a directory:

- `assets_with_recipes(dir)` — what to **show** when the directory is listed;
- `recipe_opt(key)` — what it can **produce** if asked for a key.

These can legitimately differ. `ManifestRecipeProvider` can produce chunk `data_0042.csv` from a
template but lists only a manifest's explicit chunks, since a template's chunk names are
unbounded. A conversion provider could produce `table.parquet` from `table.csv` without listing a
`.parquet` twin of every file.

The trait's default `contains` (`liquers-core/src/recipes.rs` ≈550) answers the "can you
produce it" question by looking at the "what do you show" list: `has_recipes(parent)`, then
searching `assets_with_recipes(parent)` for the name. For a provider whose two answers differ, it
says `false` for a key that `recipe_opt` would produce. Since `AssetManager::contains` and
`AssetManager::get_asset_info` (≈5206) trust it, such a key looks absent at the asset layer, with
no error.

Example, for a provider that produces `X.parquet` from `X.csv` and lists nothing:

| Call for `data/table.parquet` | Today | After the fix |
|---|---|---|
| `provider.recipe_opt` | `Some(recipe)` | `Some(recipe)` |
| `provider.contains` (default) | `false` (not in the list) | `true` |
| `AssetManager::contains` | `false` | `true` |
| `AssetManager::get(...)` | works (it uses `recipe_opt`) | works |

The two shipped providers that need it already override `contains` with a correct answer
(`ManifestRecipeProvider` ≈378, `RecipeProviderChain` ≈1048). The issue is that the next provider
of this kind inherits the wrong default silently. For `DefaultRecipeProvider` (`recipes.yaml`)
both questions have the same answer, so it is not affected.

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
