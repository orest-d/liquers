---
id: PY-RECIPE-PROVIDER-NOT-CONFIGURABLE
kind: feature
title: A Python environment cannot configure a recipe provider other than the default
status: draft
priority: P3
complexity: M
area: [py]
design:
created: 2026-10-18
github:
---

## Problem

`EnvCell::new` (`liquers-py/src/environment.rs`) refuses any `EnvironmentConfig.recipes` other
than `RecipeProviderChoice::default()` with `ConfigurationError`: the handle does not pass the
configured choice to the builder it assembles.

## Impact

A Python program cannot use manifest-based or trivial recipe providers through `config=`. It can
still use the default provider.

## Expected behaviour

Every `RecipeProviderChoice` that a native `DefaultEnvironment` accepts works from
`lq.Environment(config=...)`, with a test per choice; or the unsupported choices are listed in the
guide's §6 Python with the reason.

## Discovery

Intentionally omitted scope of `specs/design/liquers-py/`, recorded in Phase 5 and in the
`ENVIRON` row of `liquers-py/README.md`.
