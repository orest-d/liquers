---
id: VALIDATE-CANNOT-SEE-NON-STANDARD-RECIPE-PROVIDERS
kind: feature
title: liquers-validate cannot validate recipes served by a non-standard or generative recipe provider
status: draft
priority: P3
complexity: M
area: [core/commands, core/plan]
design:
created: 2026-09-25
github:
---
# `liquers-validate` cannot validate recipes served by a non-standard or generative recipe provider

## Problem

`liquers-validate` lives in `liquers-core` (`cli` feature) and deliberately links nothing above it: it
checks queries against `specs/command_registry.yaml` and reads `recipes.yaml` itself. So it can only
validate recipes in the one format core's `DefaultRecipeProvider` understands. A recipe produced by
any other provider — a generative provider (the `stockplottertest` prototype), a record manifest
(`specs/design/record-streams/`, whose chunks are recipes served by `ManifestRecipeProvider`), or an
integration's own provider — is invisible to it, although such recipes fail in exactly the ways the
tool exists to catch: a chunk query that does not plan, an argument name absent from the last action.

## Expected behaviour

Validation that goes through the environment's **recipe provider chain** rather than a file format:
enumerate what a provider can list, render what it generates for a sample of keys, and plan each
recipe against the registry. Two shapes are possible, and not exclusive:

- a validator **library** API over `AsyncRecipeProvider`, which the core CLI uses for the default
  provider;
- a second CLI linked against `liquers-lib` (or any crate assembling a full environment), which
  registers the richer providers and reuses that API.

Generative providers cannot be enumerated in full, so validation of a template means validating the
template's rendering for a few indices, and saying so in the report.

## Discovery

Raised by the user on 2026-09-25 in the `record-streams` discussion of what belongs in core: manifest
validation is one instance of a general problem that any dynamic recipe provider has.

## Update 2026-09-27 — after `record-streams` was implemented

Still open. `record-streams` added `ManifestRecipeProvider` and `RecipeProviderChain`. The ns-rec
commands are in `specs/command_registry.yaml`, so a query naming them validates. The chunk recipes
a manifest generates are still invisible to `liquers-validate`, which reads only `recipes.yaml`. A
manifest's structure is checked when it loads, for example for colliding chunk names, and each
chunk query is parsed when it is built. Nothing plans a chunk query against the registry.
