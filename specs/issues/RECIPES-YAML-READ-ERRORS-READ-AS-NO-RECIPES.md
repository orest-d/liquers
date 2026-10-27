---
id: RECIPES-YAML-READ-ERRORS-READ-AS-NO-RECIPES
kind: issue
title: Every recipes.yaml read error is treated as an empty recipe list
status: draft
priority: P2
complexity: S
area: [core/assets]
design: recipes-yaml-read-errors-propagate
created: 2026-10-20
github:
---
## Problem

`DefaultRecipeProvider::cached_recipes` (`liquers-core/src/recipes.rs`) matches the
`get_bytes(<dir>/recipes.yaml)` result and maps **every** `Err` to an empty recipe list. Only the
earlier `has_recipes` (`store.contains`) propagates errors, so a file that exists but cannot be read
— a permission error, a transport failure on a remote store, a race with a removal — silently reads
as "no recipes".

## Problem Example

Re-checked at HEAD (`cd6d5da`, 2026-10-26): unchanged. A file store whose `data/recipes.yaml`
exists but is unreadable (mode `000`): `has_recipes(data)` is `Ok(true)`, then
`cached_recipes` gets `KeyReadError` from `get_bytes`, maps it to an empty list, and
`-R/data/top.csv` fails with `AssetNotFound` ("no recipe produces it") instead of the read error.
`recipe` and `recipe_plan` reach `cached_recipes` without `has_recipes`, so the not-found arm must
stay; only other errors should propagate.

## Impact

A recipe-backed key then fails as not found (or resolves to a plain store read), hiding the real
error from the user. Likely rare on a local store, more likely on network stores.

## Expected behaviour

Only the store's not-found error maps to an empty list; every other error propagates with the
directory key.

## Discovery

Found while reviewing `reference/api/DOC_08_RECIPES_PLANS.md` against the code at `96279f14` before promoting it to [`reference/RECIPES_AND_PLANS.md`](../reference/RECIPES_AND_PLANS.md) (2026-10-20).
