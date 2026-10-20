---
id: RECIPE-LIST-SET-CWD-MUTATES-BEFORE-FAILING
kind: issue
title: RecipeList::set_cwd mutates part of the list before failing
status: draft
priority: P3
complexity: S
area: [core/assets]
design:
created: 2026-10-20
github:
---
## Problem

`RecipeList::set_cwd` (`liquers-core/src/recipes.rs`) assigns the working key to each recipe in
order and returns `NotSupported` at the first recipe that already has a `cwd`, leaving the earlier
ones mutated. (That its error does not name the recipe is
[`RECIPE-CWD-ERROR-DOES-NOT-NAME-THE-RECIPE`](RECIPE-CWD-ERROR-DOES-NOT-NAME-THE-RECIPE.md).)

## Impact

`DefaultRecipeProvider` discards the list on error, so nothing observes the partial state today;
a programmatic caller could.

## Expected behaviour

Validate the whole list before mutating it.

## Discovery

Found while reviewing `reference/api/DOC_08_RECIPES_PLANS.md` against the code at `96279f14` before promoting it to [`reference/RECIPES_AND_PLANS.md`](../reference/RECIPES_AND_PLANS.md) (2026-10-20).
