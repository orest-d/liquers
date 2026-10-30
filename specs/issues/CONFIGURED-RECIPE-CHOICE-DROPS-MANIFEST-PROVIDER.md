---
id: CONFIGURED-RECIPE-CHOICE-DROPS-MANIFEST-PROVIDER
kind: issue
title: Building a library environment from a configuration document silently drops the manifest recipe provider
status: closed
priority: P3
complexity: M
area: [lib/commands, records, core/context]
design: recipe-provider-chain-config
created: 2026-10-20
github:
---

## Resolution

Fixed by `specs/design/recipe-provider-chain-config/` (PR #224). `LibKind`'s default list is
`[recipes_yaml, manifest]` with `records`, and a configuration document (absent `recipes:` or
`default`), `with_recipe_provider_choice(Default)` and `with_default_recipe_provider()` (the
`liquers-web` path) all resolve that list, so manifests are kept. `with_records_recipe_provider()`
appends the `manifest` entry only when it is missing. Evidence:
`configured_lib_build_keeps_manifests`, `default_recipe_provider_setter_keeps_manifests` and
`records_recipe_provider_is_not_added_twice` in `liquers-lib/tests/recipe_provider_chain_config.rs`.

## Problem

With `records` on, `LibKind::default_recipe_provider` (`liquers-lib/src/environment.rs`) chains
`ManifestRecipeProvider` after the `recipes.yaml` provider. `EnvironmentBuilder::with_config`
(`liquers-core/src/environment_config.rs`) always calls `with_recipe_provider_choice(config.recipes)`,
which replaces that base, so an environment built from an `EnvironmentConfig` — even one that says
nothing about recipes — no longer resolves manifest chunk keys. record-streams Phase 4 decision 1
says `liquers-lib` calls `RecordsRecipeProvider::with_records_recipe_provider` on its configured
construction paths; the trait's own doc says nothing calls it yet.

## Problem Example

Re-checked at HEAD (`cd6d5da`, 2026-10-26): unchanged. With `records` on,
`default_environment_builder::<Value, ()>().with_async_store(store)` evaluates a manifest chunk key
(`-R/data/greet.csv` from `data/chunks.manifest.yaml`); the same builder with `.with_config(config, …)`
— any `config`, including one with no `recipes:` key — fails `AssetNotFound`, because
`with_config` calls `with_recipe_provider_choice(Default)`. `GenericEnvironment::with_recipe_provider_choice`
(`liquers-core/src/context.rs`) has the same effect. Re-sized S → M on 2026-10-26: the recommended
fix adds a defaulted `AssetManagerKind` method in `liquers-core` and overrides it in `liquers-lib`.

## Impact

Latent today: `liquers-agent`, the one caller of `with_config`, builds `liquers-lib` without
`records`. It bites the first configured `records` build, for example `liquers-py`'s fix for
`PY-RECIPE-PROVIDER-NOT-CONFIGURABLE` or a configured `liquers-axum` server, with no error: keyed
chunks just stop resolving.

## Expected behaviour

A configured `LibKind` build keeps manifest support. Options: a `liquers-lib` helper
(`with_lib_config`) that calls `with_config` then `with_records_recipe_provider`, used by every
configured path; or `AssetManagerKind` gaining a hook for providers appended after any base. Either
way, a test that a configured build serves a manifest chunk.

## Discovery

Found 2026-10-20 during the bulk-design pass while sizing `PY-RECIPE-PROVIDER-NOT-CONFIGURABLE`.

## Design

Moved 2026-10-30 to `recipe-provider-chain-config`, which absorbs this issue (Phase 1 question 6,
accepted by Orest); the earlier design is superseded.
