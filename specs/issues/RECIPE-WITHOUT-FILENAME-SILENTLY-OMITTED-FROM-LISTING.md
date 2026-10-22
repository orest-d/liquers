---
id: RECIPE-WITHOUT-FILENAME-SILENTLY-OMITTED-FROM-LISTING
kind: issue
title: A recipe with no valid filename is silently left out of the recipe listing and lookup
status: draft
priority: P3
complexity: S
area: [core/assets]
design:
created: 2026-10-20
github:
---
## Problem

`DefaultRecipeProvider` names each recipe by the filename of its query. Every place that reads a
`recipes.yaml` skips a recipe whose `Recipe::filename()` is an error (an unparsable query) or `None`
(a query without a filename), with no diagnostic: the name index built in `cached_recipes`,
`assets_with_recipes`, and `RecipeList::get` all use `if let Ok(Some(filename)) = recipe.filename()`
(`liquers-core/src/recipes.rs`).

## Impact

A typo in a recipe's query, or a forgotten filename, makes the recipe disappear from directory
listings and from key lookup. The author sees a missing asset, not an error naming the recipe.

## Expected behaviour

Report such a recipe: reject the file, or keep the others and record a warning that reaches the
author (for example in the listing's metadata or the directory asset's log).

## Discovery

Found while reviewing `reference/api/DOC_08_RECIPES_PLANS.md` against the code at `96279f14` before promoting it to [`reference/RECIPES_AND_PLANS.md`](../reference/RECIPES_AND_PLANS.md) (2026-10-20); the old document listed it as a gap without an issue.
