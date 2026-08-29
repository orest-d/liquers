For [`issues/RECIPE-PROVIDER-BY-NAME.md`](../../issues/RECIPE-PROVIDER-BY-NAME.md). Written before implementation; Q1 below was settled at the gate — see [`DESIGN.md`](./DESIGN.md) §Gate decision.

# Phase 1 — High-level design

## Problem and evidence

A recipe provider is selected by constructing a Rust value. Both providers are unit structs in
`liquers-core/src/recipes.rs` — `TrivialRecipeProvider` (`:567`) and `DefaultRecipeProvider`
(`:609`) — and every selection is a literal: `env.with_recipe_provider(Box::new(DefaultRecipeProvider))`
appears at 20+ call sites across `liquers-core/src/assets.rs`, `interpreter.rs`, `plan.rs`,
`tests/manager_parametric.rs`; `liquers-web/src/environment.rs:103` calls
`with_default_recipe_provider()`. There is no function anywhere that maps a *name* to a provider.

Stores solved the same problem: `StoreConfig` carries a `type` string, and
`liquers-store/src/store_builder.rs` turns that string into a backend, with `StoreFactory` as the
extension point. Recipe providers have no equivalent, so `EnvironmentConfig`
([`environment-builder/phase3-examples.md`](../environment-builder/phase3-examples.md)
§Scenario 4, line 338) sketches `recipes: default` with
no type behind it.

The choice is not cosmetic: `TrivialRecipeProvider::recipe` returns an error for every key, so an
environment given it fails every `-R/` query that resolves through a recipe.

## Expected behaviour and acceptance criteria

1. `liquers-core` exposes a small closed enum whose values are `default` and `trivial`, deriving
   `Serialize` and `Deserialize`.
2. It converts to `Arc<dyn AsyncRecipeProvider<E>>` for any `E: Environment`, by an **exhaustive**
   match with no `_` arm, so a third provider is a compile error.
3. `default` yields a provider that resolves a recipe from a store; `trivial` yields one that
   resolves none. Asserted by behaviour, not by type name.
4. It round-trips through both YAML and JSON, in the lowercase spelling a configuration document
   uses.
5. No existing environment, constructor or default changes behaviour.

## Affected users, workflows and systems

`core/assets` (recipes) only, at rest: nothing calls the new type until the configuration document
of `STORE-CONFIG-IN-CORE` and the `EnvironmentBuilder` exist. The consumers this unblocks are the
JavaScript (`liquers-web`) and Python (`liquers-py`) document-driven setup paths. Query, Store,
Commands and UI are untouched.

## Scope and non-goals

In scope: the enum, its conversion, its string form, and unit tests.

Explicitly **not** in this issue:

- a `RecipeProviderFactory` trait or any host-registration hook — the issue asks for the enum and
  says the factory is "only needed once a host wants to register its own provider by name";
- `EnvironmentConfig` itself, which is blocked on `STORE-CONFIG-IN-CORE`;
- changing `with_recipe_provider` signatures, environment defaults, or any call site;
- wiring the enum into `liquers-web` or `liquers-py`.

## Compatibility constraints

Purely additive. The one durable commitment is the **spelling**: once a document may say
`recipes: default`, renaming the variants breaks published configuration files. Nothing else in the
change is hard to reverse.

## Known questions and assumptions

- **Q1 — which variant is `#[default]`?** The issue's snippet marks `Default`. The
  `environment-builder` Phase 2 decision (§"The recipe-provider default is per-crate") makes
  `EnvironmentBuilder::new()` default to **`TrivialRecipeProvider`**, with `liquers-lib` supplying
  `DefaultRecipeProvider` for its own constructor. If the enum defaults to `Default`, then a
  configuration document that omits `recipes:` and a builder that is not configured disagree. This
  needs a decision; see Phase 2 §Open questions.
- Assumption: both providers stay stateless unit structs, so a value-free enum can construct either.
  Verified — neither has fields.

## Documentation assessment

Small maintenance only: `specs/reference/api/DOC_08_RECIPES_PLANS.md` describes the two providers
and would gain one sentence naming their string forms. No new reference or guide.
`specs/README.md` gains a capability line for this design, which is record maintenance rather
than documentation work.
