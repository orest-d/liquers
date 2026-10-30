---
id: RECIPE-PROVIDER-CHAIN-NOT-CONFIGURABLE
kind: feature
title: A configuration document cannot describe the recipe provider chain, only pick one built-in provider
status: draft
priority: P2
complexity: L
area: [core/context, lib/commands, records, py, web]
design: recipe-provider-chain-config
created: 2026-10-30
github:
---
## Problem

An environment's recipe providers are a `RecipeProviderChain` (`liquers-core/src/recipes.rs`), but
a configuration document can only name one built-in provider: `EnvironmentConfig.recipes` is a
`RecipeProviderChoice` (`default` or `trivial`), and `EnvironmentBuilder::with_config`
(`liquers-core/src/environment_config.rs`) installs it as the chain's *base*. Every other member is
added in code (`with_appended_recipe_provider`), always after the base. The `RecipeProviderChoice`
doc gives the reason: a choice is data and cannot name a provider living in another crate.

Two consequences:

- Applying any document (even one with no `recipes:` key, whose serde default is `default`)
  replaces `liquers-lib`'s base chain `[DefaultRecipeProvider, ManifestRecipeProvider]` with the
  bare `recipes.yaml` provider, so a configured build silently loses manifest chunk keys
  (`CONFIGURED-RECIPE-CHOICE-DROPS-MANIFEST-PROVIDER`). `liquers-web`'s `WebEnvironment` is a
  `LibKind` environment and loses them the same way through `with_default_recipe_provider()`
  (`liquers-web/src/environment.rs`) when built with `records`.
- A provider from an integration (a manifest provider, a Python or JavaScript provider in M4 of the
  liquers-py plan) cannot be placed anywhere in the chain from a document, only appended at the end
  from code. `liquers-py` therefore refuses any configured choice
  (`PY-RECIPE-PROVIDER-NOT-CONFIGURABLE`).

## Impact

Every configured build (`liquers-agent`, `liquers-py`, a future configured `liquers-axum`) has to
know which providers its crate adds and re-add them in code; one that forgets loses recipes with no
error. Workaround: build the chain in code and never name `recipes` in the document.

## Expected behaviour

The configuration describes the chain itself: `recipes` is an ordered list of provider entries
(`[]` resolves nothing, `default` is the `recipes.yaml` provider, `manifest` the record-manifest
provider, an integration's entry names a registered object). Entries are resolved by a factory
chain the building crate supplies, as store types are (`specs/design/store-factories-in-core/`), so
a document can name a provider from another crate. Every `GenericEnvironment` holds a chain.

## Discovery

Raised by Orest on 2026-10-30 while reviewing the Pre-M4 fixes `kind-aware-recipe-provider-choice`
and `py-recipe-provider-choice`: rather than patch which provider a single keyword installs, make
the chain the configured object. Overlap (`overlap.md`): strong with
`CONFIGURED-RECIPE-CHOICE-DROPS-MANIFEST-PROVIDER` (T1: this removes its cause) and
`PY-RECIPE-PROVIDER-NOT-CONFIGURABLE` (T2/T3: same `EnvironmentConfig.recipes` contract and
`EnvCell` change site). Their designs are at `implementation` with a mechanism this replaces, so
instead of extending them this design proposes to absorb both issues and supersede both designs
(Phase 1 question 6). Near-duplicates reported by `docs_index.py new` were judged: the closed
`NO-RECIPE-PROVIDER-CHAIN` (built the chain in code) and `RECIPE-PROVIDER-BY-NAME` (built the
keyword) are its predecessors, not reopenings; `LIB-RECIPE-PROVIDERS-HAVE-NO-VALIDATION-CLI` overlaps
weakly.
