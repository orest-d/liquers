---
id: NO-RECIPE-PROVIDER-CHAIN
kind: feature
title: An environment holds one recipe provider, with no way to compose several
status: draft
priority: P3
complexity: S
area: [core/assets]
design:
created: 2026-09-20
github:
---
# An environment holds one recipe provider, with no way to compose several

## What is missing

`AsyncRecipeProvider` (`liquers-core/src/recipes.rs:477`) is a clean per-directory trait:
`has_recipes(dir)`, `assets_with_recipes(dir)`, `recipe_opt(key)`, `recipe(key)`, `recipe_plan(key)`
and `contains(key)`. Two implementations exist — `TrivialRecipeProvider` (`:581`), which answers
"none" to everything, and `DefaultRecipeProvider` (`:647`), which reads `<dir>/recipes.yaml`.

An environment holds **one**. So a second provider cannot sit beside the default; it must replace it.
There is no composition, and no way for one directory to be served by one provider and a sibling by
another.

Stores solved the same problem with `AsyncStoreRouter` and `StoreRouterBuilder`, where the first
backend to claim a key wins. Recipes have no equivalent.

## Why it matters

Any provider that interprets something other than `recipes.yaml` is currently all-or-nothing. Two
concrete cases:

- **Record-stream folders.** `specs/design/record-streams/chunking-and-resumability.md` §4a describes
  a folder whose chunks are described by a manifest rather than a `recipes.yaml`, with chunks becoming
  keyed assets. That provider would have to replace `DefaultRecipeProvider`, breaking every ordinary
  recipe folder in the same environment. **A chain is a prerequisite for that work**, not part of it.
- **Generated or virtual recipe collections** more generally — a provider synthesizing recipes from a
  pattern, from a database catalogue, or from a remote index.

## Shape of a fix

A `RecipeProviderChain` holding `Vec<Box<dyn AsyncRecipeProvider<E>>>`, delegating in order:
`has_recipes` returns on the first provider that claims the directory, and the per-key methods
delegate to that same one. First-to-claim-wins matches the store router's rule, so the semantics are
already familiar.

The one decision needing care: whether claiming is per **directory** (simple, matches
`has_recipes`) or per **key** (more flexible, allows two providers to serve different files in one
folder). Per-directory is almost certainly right — a folder with mixed recipe sources is hard to
reason about, and the record-stream case explicitly wants a folder to be claimed whole.

Small and self-contained; `S` reflects that the trait and the precedent both already exist.

## Update 2026-09-25 — keyed record chunks depend on this

`specs/design/record-streams/` needs **keyed chunk assets** for a manifest's `stored`/`cached` flags and
for per-chunk `arguments`/`links` (`manifest-format.md` §4b, Phase 2 open question 18). A keyed asset is
created only through `AssetManager::get(key)`, which asks the recipe provider for the recipe, so keyed
chunks need three pieces: chunk keys named by the manifest, **a recipe provider that serves those keys
from the manifest composed with the folder's `recipes.yaml` provider**, and **recipe-level
`stored`/`cached` flags the asset manager honours**. The record design recommends designing the second
and third as a small prerequisite project rather than as records features.

