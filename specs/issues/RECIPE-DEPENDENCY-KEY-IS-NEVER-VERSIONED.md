---
id: RECIPE-DEPENDENCY-KEY-IS-NEVER-VERSIONED
kind: issue
title: Editing a recipe does not expire the assets computed from it, because recipe dependency keys never get a version
status: draft
priority: P2
complexity: M
area: [core/assets, core/plan]
design:
created: 2026-10-20
github:
---

## Problem

The dependency walk records `-R-recipe/<key>` (`DependencyKey::from_recipe_key`,
`DependencyRelation::Recipe`, `liquers-core/src/plan.rs`) as a dependency of every plan that reads a
recipe-served key. Nothing ever registers a version for such a key: `from_recipe_key` has no call
site outside `plan.rs` and tests. A write of `recipes.yaml` publishes the version of that file's key
and calls `AsyncRecipeProvider::directory_changed`, which only refreshes the provider's cache. So
the edges from recipe keys record `Version::unknown()` and never cascade. An asset computed from
the old recipe stays `Ready`, in memory and in the store, after the recipe is edited.

Inferred from reading the code at 18732dd. Not reproduced; a provider or a command may
invalidate assets in a way not found here.

## Problem Example

`data/recipes.yaml` declares `r.txt` = `hello/upper`. Evaluate `data/r.txt`, which is `Ready`. Rewrite
`recipes.yaml` so that `r.txt` = `hello/lower`, through `set_binary`. `get(data/r.txt)` still serves
the upper-case value.

## Impact

Stale values after a recipe edit until the asset is removed or expires for another reason. It also
blocks reusing analysis summaries across evaluations
(`DEPENDENCY-ANALYSIS-SUMMARIES-NOT-REUSED-ACROSS-EVALUATIONS`), whose invalidation would rely on
recipe versions.

## Expected behaviour

A recipe edit gives each changed recipe key a new version, for instance a hash of the recipe's
serialized form registered when the provider resolves it or when its directory changes, so its
dependents expire like those of any other changed dependency.

## Discovery

Bulk design 2026-10-20 (`payload-requirement-skips-dependency-analysis`), when checking whether
recipe edits could invalidate a cached analysis summary.
