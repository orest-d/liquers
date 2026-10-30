# Phase 4: Implementation Plan — recipe-provider-chain-config

## Overview

Bottom-up: the configuration types first, then factories and `resolve_recipes`, then the chain's
every-member rule, then the builder and the environment, then each crate above core. Every step
compiles and passes its own tests on its own. No prerequisite issue. One branch and one PR (#224),
which also carries the design.

## Progress

- [x] Step 1: `RecipeProviderEntry` and `RecipesConfig` with their serde forms (`21fe1ba`)
- [x] Step 2: entry-type factories and `resolve_recipes` (`21fe1ba`)
- [x] Step 3: the chain asks every member (`21fe1ba`)
- [x] Step 4: the builder, the kind and the configuration document (`6ca02b6`)
- [x] Step 5: `GenericEnvironment` setters (`e73b570`)
- [x] Step 6: the manifest provider's own clash check removed (`57eb44f`)
- [x] Step 7: `LibKind`, `manifest` and `with_records_recipe_provider` (`4297b59`)
- [x] Step 8: `liquers-py` accepts every `recipes` value (`ed54f26`)
- [x] Step 9: feature matrix and full test loops (validation, no commit)
- [x] Step 10: documents and records (this documentation commit)

## Implementation Steps

### Step 1: `RecipeProviderEntry` and `RecipesConfig` with their serde forms
- Files / symbols: `liquers-core/src/recipes.rs`: new types beside `RecipeProviderChoice`, plus
  `From<RecipeProviderChoice> for RecipesConfig`.
- Change: hand-written `Serialize` / `Deserialize` through a visitor accepting a string, a sequence
  or a map. The visitor raises `de::Error::custom` with the accepted keywords. `KindDefault` is
  serialized by the caller's `skip_serializing_if = "RecipesConfig::is_kind_default"`.
- Depends on: none.
- Proof: `cargo test -p liquers-core --lib recipes_config_tests` (the six Phase 3 unit tests).
- Rollback: delete the types; nothing uses them yet.

### Step 2: entry-type factories and `resolve_recipes`
- Files / symbols: `liquers-core/src/recipes.rs`: `RecipeProviderTypeInfo` (+
  `unavailability_error`), `RecipeProviderFactory`, `RecipeProviderConstructor`,
  `RecipeProviderTypeMap`, `ChainedRecipeProviderFactory`, `core_recipe_provider_factory`,
  `resolve_recipes`.
- Change: as Phase 2, modelled on `liquers-core/src/store_factory.rs` `StoreTypeMap` /
  `ChainedStoreFactory` / `unknown_store_type_error`. Only `recipes_yaml` is declared.
- Depends on: Steps 1 and 3 (`resolve_recipes` returns a named chain).
- Proof: `cargo test -p liquers-core --lib recipes` with unit tests for an unknown type, an
  unavailable type, a duplicate, and the `default` splice.
- Rollback: delete; nothing calls it yet.

### Step 3: the chain asks every member
- Files / symbols: `liquers-core/src/recipes.rs` `RecipeProviderChain`: `named`, `names`, private
  `owner_of` replacing `provider_index_for`; `recipe_opt`, `recipe`, `recipe_plan`,
  `get_asset_info`, `can_make`; the doc table (`contains`).
- Change: every member is asked, any error fails, and two `Some`s are a configuration error naming
  both members. `new` and `push` name members `#n` by position.
- Depends on: Step 1 only for ordering; runs before Step 2.
- Proof: `cargo test -p liquers-core --lib recipe_provider_chain_tests` (the module's ten tests,
  with `appended_provider_is_consulted_after_the_configured_one` rewritten and `first_some_wins`
  renamed, plus the new ones); `cargo test -p liquers-core --lib --tests`. Before Step 7 a
  `recipes.yaml` / manifest clash still reports the manifest provider's own error; no core test
  depends on it.
- Rollback: revert the impl block.

### Step 4: the builder, the kind and the configuration document
- Files / symbols: `liquers-core/src/environment_builder.rs` `AssetManagerKind::{default_recipes,
  recipe_provider_factory}` (defaulted), `EnvironmentBuilder` field `base: RecipeBase`,
  `with_recipes`, `with_recipe_provider_factory`, `with_appended_recipe_entry`,
  `with_recipe_provider_choice`, `build`; the test at `:805-816`. `liquers-core/src/environment_config.rs`
  `EnvironmentConfig.recipes: RecipesConfig`, `with_config`, the tests at `:185-232`.
- Also: the two `liquers-py/src/environment.rs` lines that compare or assign `config.recipes`
  (`:114`, `:932`) are adapted here, so the workspace keeps compiling; the refusal itself goes in
  Step 8.
- Depends on: Steps 1-3.
- Proof: `cargo test -p liquers-core --lib --tests`, including the new
  `tests/recipe_provider_chain_config.rs` (all but its `GenericEnvironment` tests);
  `cargo check -p liquers-py -p liquers-lib -p liquers-agent`.
- Rollback: revert both files; Steps 1-3 stand alone.

### Step 5: `GenericEnvironment` setters
- Files / symbols: `liquers-core/src/context.rs` `GenericEnvironment::{try_with_recipes,
  with_recipe_provider_choice}`; a private failing provider for an unbuildable kind default.
- Depends on: Step 4.
- Proof: `cargo test -p liquers-core --test recipe_provider_chain_config` (all), and
  `cargo test -p liquers-core --lib --tests`.
- Rollback: revert the file.

### Step 6: the manifest provider's own clash check removed
- Files / symbols: `liquers-records/src/provider.rs` `check_no_recipes_yaml_collision` and its two
  call sites (`:362`, `:390`); the renamed test (Phase 3).
- Depends on: Step 3 (the chain takes over the rule).
- Proof: `cargo test -p liquers-records --all-features --lib --tests`.
- Rollback: restore the function.

### Step 7: `LibKind`, `manifest` and `with_records_recipe_provider`
- Files / symbols: `liquers-lib/src/environment.rs` `LibKind::{default_recipes,
  recipe_provider_factory}`, `RecordsRecipeProvider`, the `LibKind` and `DefaultEnvironment` doc
  comments; new `liquers-lib/tests/recipe_provider_chain_config.rs`. The existing test
  `with_records_recipe_provider_serves_a_manifest_chunk_over_a_trivial_base` stays.
- Change: `default_recipe_provider` keeps the unconfigured chain; `manifest` declared unavailable
  without `records`.
- Depends on: Step 5.
  Also the comment at `liquers-web/src/environment.rs:106`.
- Depends on: Steps 5 and 6 (the lib clash test needs the chain's error).
- Proof: `cargo test -p liquers-lib --lib --tests`, then
  `cargo test -p liquers-lib --no-default-features --features records --test recipe_provider_chain_config`
  and `cargo test -p liquers-lib --no-default-features --test recipe_provider_chain_config`.
- Rollback: revert; core's defaults still apply.

### Step 8: `liquers-py` accepts every `recipes` value
- Files / symbols: `liquers-py/src/environment.rs` `EnvCell::new` (refusal removed),
  `EnvCell::configured_builder` (`with_recipes`), `mod tests`; `liquers-py/tests/test_ENVIRON_environment.py`.
- Depends on: Step 7. Run after `cargo clean`, separately (`CLAUDE.md`).
- Proof: `bash scripts/test-liquers-py.sh` (whole suite and its Rust unit tests).
- Rollback: revert both files.

### Step 9: feature matrix and full test loops
- Proof: `cargo test -p liquers-core --lib --tests`; `cargo test -p liquers-lib --lib --tests`;
  `cargo test -p liquers-records --all-features --lib --tests`; `cargo test -p liquers-agent`;
  `cargo test -p liquers-axum` (its examples and tests use the changed setters);
  `bash scripts/check-build-matrix.sh`; `cargo check -p liquers-web --target wasm32-unknown-unknown`
  with and without `records`. Run `cargo clean` between groups when disk is short.
- Rollback: not applicable; a failure returns to its step.

### Step 10: documents and records
- Files: Documentation Updates below; the three absorbed issues set `closed` with resolution notes
  (`DOCS_STRUCTURE_GUIDE.md` §4.3); `DESIGN.md` phase and status; `specs/index.csv` regenerated.
- Proof: `python3 scripts/docs_index.py --check`.
- Rollback: revert the documents.

## Testing Plan

Each step runs its own proof. Steps 1-7 use the native loop; Step 8 runs after `cargo clean`.
Step 9 is the final check, because Steps 1, 4 and 7 touch `#[cfg(feature = "records")]` code and a
public type in `liquers-core`.

## Rollback Plan

Steps 1-3 are additive; Steps 4-8 each revert one or two files. A partial landing is safe after
Step 5 (core complete, lib on its old default, the manifest provider's check still reporting a
`recipes.yaml` clash in its own words) or after Step 7.

## Documentation Updates

Phase 2's Documentation Architecture list, each with a History row and `reviewed:` bump in the same
commit. `specs/command_registry.yaml` is not affected, since no command changes. Regenerate
`specs/index.csv`.

## Phase 5 Entry Criteria

- [x] Implementation finished and validated
- [x] User and review comments answered
- [x] Documentation checkable against implemented and tested behaviour
