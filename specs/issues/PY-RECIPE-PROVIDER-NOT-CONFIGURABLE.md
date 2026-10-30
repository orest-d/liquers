---
id: PY-RECIPE-PROVIDER-NOT-CONFIGURABLE
kind: feature
title: A Python environment cannot configure a recipe provider other than the default
status: draft
priority: P3
complexity: S
area: [py]
design: recipe-provider-chain-config
created: 2026-10-18
github:
---

## Problem

`EnvCell::new` (`liquers-py/src/environment.rs`) refuses any `EnvironmentConfig.recipes` other
than `RecipeProviderChoice::default()` with `ConfigurationError`: the handle does not pass the
configured choice to the builder it assembles.

## Problem Example

```python
import liquers as lq
env = lq.Environment(config={"recipes": "trivial"})
# raises ConfigurationError: "a recipe provider other than the default cannot be configured
# from Python yet" (EnvCell::new, liquers-py/src/environment.rs; unit test
# a_non_default_recipe_provider_is_refused)
```

Re-sized M → S on 2026-10-20: `RecipeProviderChoice` has two variants (`Default`, `Trivial`), and
the builder already accepts them (`EnvironmentBuilder::with_recipe_provider_choice`); the fix is one
call in `EnvCell::configured_builder` plus a test per choice. A "manifest-based" provider is not a
`RecipeProviderChoice`: with `records` on, `liquers-lib` appends `ManifestRecipeProvider` to the
default chain, and replacing the base drops it unless `RecordsRecipeProvider::with_records_recipe_provider`
is called too (record-streams Phase 4 decision 1).

## Impact

A Python program cannot use manifest-based or trivial recipe providers through `config=`. It can
still use the default provider.

Re-checked at HEAD (2026-10-26): unchanged (`EnvCell::new`, `liquers-py/src/environment.rs`). Because
`liquers-py` builds `liquers-lib` with `records`, accepting a configured choice through
`with_config` today would silently drop `ManifestRecipeProvider`
(`CONFIGURED-RECIPE-CHOICE-DROPS-MANIFEST-PROVIDER`); land that fix first or together.

## Expected behaviour

Every `RecipeProviderChoice` that a native `DefaultEnvironment` accepts works from
`lq.Environment(config=...)`, with a test per choice; or the unsupported choices are listed in the
guide's §6 Python with the reason.

## Discovery

Intentionally omitted scope of `specs/design/liquers-py/`, recorded in Phase 5 and in the
`ENVIRON` row of `liquers-py/README.md`.

## Design

Moved 2026-10-30 to `recipe-provider-chain-config`, which absorbs this issue (Phase 1 question 6,
accepted by Orest); the earlier design is superseded.
