---
id: RECIPE-PROVIDERS-FAIL-ON-A-KEY-THE-STORE-DOES-NOT-SUPPORT
kind: issue
title: Recipe providers fail a query when the store refuses the recipes.yaml or manifest key as unsupported
status: closed
priority: P1
complexity: S
area: [core/assets, records, web]
design: record-streams
created: 2026-09-27
github:
---
# Recipe providers fail a query when the store refuses the `recipes.yaml` or manifest key as unsupported

## Resolution

Fixed 2026-09-27, record-streams Phase 4 Step 8.3. The fix is in two places:
- `DefaultRecipeProvider::has_recipes` answers `false` when `contains` refuses the key as
  `KeyNotSupported`;
- `ManifestRecipeProvider` treats `KeyNotSupported` like `KeyNotFound` in `get_manifest`, and
  answers "no manifests" from `manifest_names`, without caching that answer.

A key the store cannot hold cannot hold a recipe list or a manifest. Tests:
`recipes.rs` `default_provider_finds_no_recipe_under_an_unsupported_key` and `provider.rs`
`an_unsupported_key_has_no_manifest_recipe`, each against a store that refuses every key. The
three `liquers-web` e2e tests this broke now pass.

## Problem

Every `-R/<folder>/<name>` query first asks the recipe provider whether `<name>` has a recipe.
`DefaultRecipeProvider` does this through `store.contains(<folder>/recipes.yaml)`. A store router
answers `Err(KeyNotSupported)` for any key none of its members covers
(`AsyncStoreRouter::contains`). One such member is an `http` store configured with a fixed key list.
The error propagated, so the query failed with `Key 'data/recipes.yaml' not supported by store store
router`, although the resource itself was in the key list and readable.

`ManifestRecipeProvider` had the same weakness in `listdir`, `get_metadata` and `get`: it tolerated
`KeyNotFound` only.

## Impact

In `liquers-web`, a fetched resource could not be evaluated at all. Three e2e tests failed:
`STORE07 a fetched resource evaluates end to end`, `STORE07 a nested fetched key resolves` and
`STORE11 fetch and localStorage coexist in one configuration`. The base commit of `record-streams`
fails them identically, so the defect predates that design. It went unnoticed because the e2e loop
had not been run since.

## Discovery

Found 2026-09-27 in record-streams Step 8.3, the first full run of the three `liquers-web` loops
after the implementation. The failure was reproduced on the design's base commit to establish that
it predates the branch.
