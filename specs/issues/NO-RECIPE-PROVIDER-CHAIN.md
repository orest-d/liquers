---
id: NO-RECIPE-PROVIDER-CHAIN
kind: feature
title: An environment holds one recipe provider, with no way to compose several
status: closed
priority: P3
complexity: S
area: [core/assets]
design: record-streams
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

## Update 2026-09-25 — designed within `record-streams`

The user chose to build keyed record chunks in the `record-streams` project, so this is resolved there
as piece B — a core `RecipeProviderChain`, with `ManifestRecipeProvider` appended when `records` is on. See `specs/design/record-streams/phase2-architecture.md` §"Keyed chunks". The status
stays `draft` until the work starts; the record's own `status` is concluded with that project.

## Resolution

Implemented in `record-streams` Phase 4, Step 1.3 (`specs/design/record-streams/phase4-implementation.md`).

`RecipeProviderChain<E>` (`liquers-core/src/recipes.rs`) holds `Vec<Arc<dyn AsyncRecipeProvider<E>>>`
and implements `AsyncRecipeProvider<E>` per Phase 2 §"B. A recipe provider that serves chunk keys
from manifests": `recipe_opt` is first-`Some`-wins (an earlier provider's `Err` propagates without
consulting the rest); `contains` and `has_recipes` are true if any provider answers so (`contains`
goes through `recipe_opt`, not a directory-level check, since a provider's own `contains` default
may depend on directory state a chain member does not populate); `assets_with_recipes` is the union
in provider order, without duplicates; `recipe`, `recipe_plan` and `get_asset_info` delegate to the
first provider whose `recipe_opt` claims the key, and return the same not-found error
`DefaultRecipeProvider` gives for a missing key when none does.

Composition is in code, not configuration, matching decision 1 in the Phase 4 plan:
`RecipeProviderChoice` is unchanged (a configuration document cannot name a provider from another
crate). `EnvironmentBuilder::with_appended_recipe_provider` records providers in a builder-held
`Vec` and `build()` composes `RecipeProviderChain::new([base, appended…])` — base is the configured
provider, or `K::default_recipe_provider()` when none is configured — only when the `Vec` is
non-empty. `GenericEnvironment::with_appended_recipe_provider` does the equivalent for a
already-built environment, wrapping the current provider and the new one in a chain (nested chains
are correct, one indirection deeper).

Tests: `liquers-core/src/recipes.rs::recipe_provider_chain_tests` — `first_some_wins`,
`contains_is_true_if_any_provider_has_the_recipe`,
`assets_with_recipes_is_the_union_without_duplicates`, `push_adds_a_provider_after_construction`,
`recipe_opt_is_none_when_no_provider_has_the_key` (Phase 3 §4.1, ported as pinned), and
`appended_provider_is_consulted_after_the_configured_one` (Phase 4's added test, exercising
`EnvironmentBuilder`). All pass under `cargo test -p liquers-core --lib --tests` (845 lib tests, 0
failures); `cargo check -p liquers-lib -p liquers-axum -p liquers-py` and
`cargo check -p liquers-core --target wasm32-unknown-unknown` also pass.

The original "shape of a fix" sketch (`Vec<Box<dyn …>>`, per-directory first-claim-wins) was
superseded during design: providers are shared (`Arc`, since the same provider may need to sit in
more than one place — the builder's own base plus any appended chain) and the truthy behaviour is
per-method ("any provider" for `has_recipes`/`contains`, "first `Some`" for `recipe_opt`) rather
than a single directory-level claim, matching what a generative provider like the manifest chunk
provider (`ManifestRecipeProvider`, built in a later Phase 4 step) needs: it cannot enumerate its
keys, so it cannot "claim" a directory outright the way `has_recipes` alone would suggest.

`ManifestRecipeProvider` itself is out of scope for this issue and is designed as
`liquers-records`'s own piece; this issue covers the core chain and appending mechanism only.

