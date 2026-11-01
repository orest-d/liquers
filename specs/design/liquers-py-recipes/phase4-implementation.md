# Phase 4: Implementation Plan — liquers-py-recipes

## Overview

The two `liquers-core` changes come first: they stand alone, every environment's chain goes
through them, and their tests run without Python. Then the `lq.Recipe` properties, the adapter and
factory, and the `env.recipes` handle. The liquers-py steps need `liquers-py-store` Phase 4
Steps 1, 2 and 4 (`pycall.rs`, `_factories.py`, the build guard); Steps 1-3 here can run in
parallel with that work. No issue has to be fixed first (Phase 2 preflight).

## Progress

- [ ] Step 1: repeated recipe provider types in `liquers-core`
- [ ] Step 2: a public plan constructor for a recipe
- [ ] Step 3: `lq.Recipe` keyword constructor and properties
- [ ] Step 4: the Python recipe provider adapter and factory
- [ ] Step 5: the `env.recipes` handle
- [ ] Step 6: exports, docstrings and the conformance row

## Implementation Steps

### Step 1: repeated recipe provider types in `liquers-core`

- Files / symbols: `liquers-core/src/recipes.rs` (`expand_recipes`, `resolve_recipes`);
  `liquers-core/src/environment_builder.rs` (`build`, the `AppendedRecipe::Entry` arm).
- Change: `expand_recipes` refuses an entry equal to an earlier one (`RecipeProviderEntry`
  equality: type and arguments) instead of a repeated type, with the message "Recipe provider
  entry '<type>' (<arguments>) is listed more than once". `resolve_recipes` names a member by its
  type when the type occurs once, else `type#position`. `build` keeps the member *types* beside
  the names and skips an appended entry when its type is among them, as today.
- Depends on: none.
- Proof: `cargo test -p liquers-core --lib recipes::` (`expand_recipes_refuses_an_identical_entry`,
  `the_same_type_with_different_arguments_makes_two_members`,
  `repeated_type_members_are_named_by_position`, `named_chain_reports_member_names` unchanged),
  `cargo test -p liquers-core --test recipe_provider_chain_config`, `cargo test -p liquers-lib
  --test recipe_provider_chain_config` (`an_appended_entry_is_skipped_when_its_type_is_listed`).
- Rollback: revert; no caller depends on repeats yet.

### Step 2: a public plan constructor for a recipe (parallel with Step 1)

- Files / symbols: `liquers-core/src/recipes.rs` `impl Recipe { pub async fn
  plan_with_dependencies }`, `create_plan_with_init_metadata`.
- Change: the private function's body becomes the public method; the function calls it (the
  analysis error still ignored).
- Depends on: none.
- Proof: `plan_with_dependencies_matches_the_default_provider`; `cargo test -p liquers-lib --lib
  --tests` unchanged.
- Rollback: revert.

### Step 3: `lq.Recipe` keyword constructor and properties (parallel with Steps 1-2)

- Files / symbols: `liquers-py/src/recipes.rs` `Recipe::new`, getters and setters `cwd`,
  `volatile`, `expires`.
- Change: `#[pyo3(signature = (query, title="", description="", *, cwd=None, volatile=false,
  expires=None))]`; `cwd` is `str | None`, `expires` an `lq.Expires`. Positional calls keep
  working.
- Depends on: none.
- Proof: a new `test_RECIPE_return_forms` case for the constructor; the existing
  `test_OBJECT_*` recipe tests unchanged.
- Rollback: revert.

### Step 4: the Python recipe provider adapter and factory

- Files / symbols: new `liquers-py/src/recipe_provider.rs` (`RecipeMethods`, `PyRecipeProvider`,
  `PyRecipeProviderType`, `PyRecipeProviderFactory`); `src/environment.rs`
  (`Declarations.recipe_types`, `configured_builder`, `shutdown`,
  `Environment.register_recipe_provider_factory`).
- Change: the Phase 2 adapter. `recipe_opt` converts `lq.Recipe | str | None`, setting `cwd` to
  the key's folder when absent; `recipe_plan` calls `Recipe::plan_with_dependencies`; a listing
  entry outside `folder` is an `ExecutionError`. Registration refuses `LibKind`'s provider types,
  the keywords and a registered Python type, derives arguments with `_declare(…, reserved={"type"})`,
  and marks `Pending`. `configured_builder` passes the factory to
  `with_recipe_provider_factory`; the build guard covers the factory call; `shutdown()` takes the
  types out with the store's.
- Depends on: Steps 1-3; `liquers-py-store` Steps 1, 2 and 4.
- Proof: `test_RECIPE_python_providers.py` (except the inspection test),
  `test_RECIPE_registration.py`.
- Rollback: revert Steps 4-6 together.

### Step 5: the `env.recipes` handle

- Files / symbols: `src/recipe_provider.rs` `RecipeProviders` pyclass; `src/environment.rs`
  `#[getter] recipes`.
- Change: each method resolves `envref` on the calling thread and asks
  `envref.get_recipe_provider()`; listings come back as `lq.ResourceName`, plans as `lq.Plan`.
- Depends on: Step 4.
- Proof: `test_RECIPE01_found_and_missing_recipe`, `test_RECIPE_inspection_methods_and_async_twins`.
- Rollback: with Step 4.

### Step 6: exports, docstrings and the conformance row

- Files / symbols: `src/lib.rs`, `python/liquers/__init__.py` (`RecipeProviders`),
  `tests/test_DOCS_docstrings.py`, `liquers-py/README.md` `RECIPE` row.
- Change: register and export; a docstring on every new class and method.
- Depends on: Step 5.
- Proof: `bash scripts/test-liquers-py.sh`.
- Rollback: with Step 4.

## Testing Plan

Steps 1-2: `cargo test -p liquers-core` and `cargo test -p liquers-lib --lib --tests`, since the
duplicate rule reaches every environment. Steps 3-6: `bash scripts/test-liquers-py.sh` on 3.10,
3.14 and 3.14t (CI). `liquers-web` builds its chain through the same core code:
`cargo test -p liquers-web --target wasm32-unknown-unknown --features debug-handles` after a
`cargo clean` (CI runs it). No feature flag changes, so no build-matrix run.

## Rollback Plan

Revert the commits. Steps 1-2 are core-only and land alone; a document using a repeated type then
stops building again, which is the only behaviour they add. Steps 4-6 land together.

## Documentation Updates

Phase 5, from Phase 2's documentation architecture: `specs/reference/PYTHON_INTEGRATION.md`
(§Recipe providers, §3), `specs/guides/PYTHON_GUIDE.md` §6, `specs/reference/ENVIRONMENT_CONFIG.md`
(duplicate rule, member names, the `{type: python, …}` example replaced),
`specs/guides/ENVIRONMENT_CONSTRUCTION_GUIDE.md`, `specs/reference/RECIPES_AND_PLANS.md`
(`Recipe::plan_with_dependencies`), `specs/guides/LANGUAGE-INTEGRATION_GUIDE.md` (§RECIPE, §6),
`liquers-py/README.md`, `specs/README.md`; issue `PY-NO-LANGUAGE-DEFINED-RECIPE-PROVIDER` closed.
Each changed `reference/` or `guides/` document gets a `## History` row and a `reviewed:` bump;
`python3 scripts/docs_index.py` regenerates `specs/index.csv`. No command changes.

## Phase 5 Entry Criteria

- [ ] Implementation finished and validated
- [ ] User and review comments answered
- [ ] Documentation checkable against implemented and tested behaviour
