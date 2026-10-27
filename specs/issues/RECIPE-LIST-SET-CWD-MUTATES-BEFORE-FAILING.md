---
id: RECIPE-LIST-SET-CWD-MUTATES-BEFORE-FAILING
kind: issue
title: RecipeList::set_cwd mutates part of the list before failing
status: closed
priority: P3
complexity: S
area: [core/assets]
design: recipe-list-set-cwd-validation
created: 2026-10-20
github:
---
## Problem

`RecipeList::set_cwd` (`liquers-core/src/recipes.rs`) assigns the working key to each recipe in
order and returns `NotSupported` at the first recipe that already has a `cwd`, leaving the earlier
ones mutated. (That its error does not name the recipe is
[`RECIPE-CWD-ERROR-DOES-NOT-NAME-THE-RECIPE`](RECIPE-CWD-ERROR-DOES-NOT-NAME-THE-RECIPE.md).)

## Problem Example

Re-checked at HEAD (`cd6d5da`, 2026-10-26): unchanged. A `RecipeList` of three recipes, the third
with `cwd: other`: `set_cwd("data")` returns `Err(NotSupported)` with `recipes[0].cwd` and
`recipes[1].cwd` already set to `data`. Designed together with
`RECIPE-CWD-ERROR-DOES-NOT-NAME-THE-RECIPE` (same function).

## Impact

`DefaultRecipeProvider` discards the list on error, so nothing observes the partial state today;
a programmatic caller could.

## Expected behaviour

Validate the whole list before mutating it.

## Discovery

Found while reviewing `reference/api/DOC_08_RECIPES_PLANS.md` against the code at `96279f14` before promoting it to [`reference/RECIPES_AND_PLANS.md`](../reference/RECIPES_AND_PLANS.md) (2026-10-20).

## Resolution

Fixed 2026-10-27 by `design/recipe-list-set-cwd-validation/`. `RecipeList::set_cwd` (`liquers-core/src/recipes.rs`) checks the whole list before mutating: when any entry already has a `cwd` it returns `NotSupported` "CWD can't be explicitly specified in a recipe: recipes[i] (query '…') sets cwd '…'" for every offender and leaves the list unchanged, and it no longer writes to stderr. Evidence: `set_cwd_tests` (`set_cwd_failure_leaves_the_list_unchanged`, `set_cwd_error_names_every_offending_recipe`, `set_cwd_sets_every_recipe`) and `explicit_cwd_in_recipes_yaml_names_the_recipe`.
