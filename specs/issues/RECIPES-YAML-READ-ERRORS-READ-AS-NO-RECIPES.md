---
id: RECIPES-YAML-READ-ERRORS-READ-AS-NO-RECIPES
kind: issue
title: Every recipes.yaml read error is treated as an empty recipe list
status: draft
priority: P2
complexity: S
area: [core/assets]
design:
created: 2026-10-20
github:
---
## Problem

`DefaultRecipeProvider::cached_recipes` (`liquers-core/src/recipes.rs`) matches the
`get_bytes(<dir>/recipes.yaml)` result and maps **every** `Err` to an empty recipe list. Only the
earlier `has_recipes` (`store.contains`) propagates errors, so a file that exists but cannot be read
— a permission error, a transport failure on a remote store, a race with a removal — silently reads
as "no recipes".

## Impact

A recipe-backed key then fails as not found (or resolves to a plain store read), hiding the real
error from the user. Likely rare on a local store, more likely on network stores.

## Expected behaviour

Only the store's not-found error maps to an empty list; every other error propagates with the
directory key.

## Discovery

Found while reviewing `reference/api/DOC_08_RECIPES_PLANS.md` against the code at `96279f14` before promoting it to [`reference/RECIPES_AND_PLANS.md`](../reference/RECIPES_AND_PLANS.md) (2026-10-20).
