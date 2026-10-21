---
id: CONFIGURED-RECIPE-CHOICE-DROPS-MANIFEST-PROVIDER
kind: issue
title: Building a library environment from a configuration document silently drops the manifest recipe provider
status: draft
priority: P3
complexity: S
area: [lib/commands, records, core/context]
design:
created: 2026-10-20
github:
---

## Problem

With `records` on, `LibKind::default_recipe_provider` (`liquers-lib/src/environment.rs`) chains
`ManifestRecipeProvider` after the `recipes.yaml` provider. `EnvironmentBuilder::with_config`
(`liquers-core/src/environment_config.rs`) always calls `with_recipe_provider_choice(config.recipes)`,
which replaces that base, so an environment built from an `EnvironmentConfig` — even one that says
nothing about recipes — no longer resolves manifest chunk keys. record-streams Phase 4 decision 1
says `liquers-lib` calls `RecordsRecipeProvider::with_records_recipe_provider` on its configured
construction paths; the trait's own doc says nothing calls it yet.

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
