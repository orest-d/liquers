---
id: RECIPE-MISSING-RECIPE-IS-A-GENERAL-ERROR
kind: issue
title: Asking a recipe provider for a recipe it does not have raises a General error, not KeyNotFound
status: draft
priority: P3
complexity: S
area: [core/assets, core/error]
created: 2026-11-01
github:
---
## Problem

`AsyncRecipeProvider::recipe` and `recipe_plan` are documented as "a missing recipe is an
error", but which error is not fixed. `RecipeProviderChain::recipe` (`liquers-core/src/recipes.rs`,
"No recipe found for key …") and `DefaultRecipeProvider::recipe` / `recipe_plan` raise
`Error::general_error`, while a provider built on `recipe_opt` (the trivial pattern
`recipe_opt(...)?.ok_or_else(|| Error::key_not_found(key))`, used by the `liquers-py` adapter
and the core test mocks) raises `KeyNotFound`. A caller cannot tell "no recipe" from any other
general failure without matching the message.

Seen through `liquers-py`: `env.recipes.recipe(key)` for a key no member has raises
`lq.GeneralError`, where `lq.KeyNotFoundError` would be expected
(`test_RECIPE01_found_and_missing_recipe` accepts any `LiquersError` for now).

## Expected behaviour

A missing recipe is `KeyNotFound` with the key, from every provider and from the chain, so
`recipe` agrees with `recipe_opt` returning `None`. `MISSING-RESOURCE-REPORTED-AS-GENERAL-ERROR`
(closed) settled the same question for evaluation.

## Discovery

`specs/design/liquers-py-recipes/`, Phase 4 step 4 tests.
