---
id: RECIPE-PROVIDER-BY-NAME
kind: feature
title: Recipe providers cannot be selected by name, so a configuration document cannot choose one
status: draft
priority: P3
complexity: S
area: [core/recipes, web]
design: queued-manager-startup-readiness
created: 2026-08-27
github:
---
## Problem

A recipe provider is chosen by constructing a Rust value — `Arc::new(DefaultRecipeProvider)` or
`Arc::new(TrivialRecipeProvider)` — and passing it to the environment. There is no mapping from a
name to a provider, so a configuration document cannot express `recipes: default`.

Stores solved this already: `StoreConfig` names a `type`, and `StoreFactory` implementations turn
that name into a backend. Recipe providers have no equivalent.

## Why it matters

An `EnvironmentConfig` (sketched in `specs/design/queued-manager-startup-readiness/phase3-examples.md`
§Scenario 4) needs to select a recipe provider from data for the JavaScript and Python setup path.
Everything else in that document — store, asset-manager options — is expressible; the recipe
provider is the one field that would have to stay in code for no principled reason.

The choice genuinely matters: `TrivialRecipeProvider` resolves no recipes at all, and
`DefaultRecipeProvider` reads them through the environment's store. Getting it wrong makes every
`-R/` query fail, which is why `liquers-web` sets it explicitly at construction.

## Expected behavior

A named lookup, small and closed by default: `"default"` and `"trivial"` resolve to the built-in
providers, with a registration hook for a host that supplies its own.

## Fix direction

Mirror `StoreFactory` at a smaller scale — a `RecipeProviderFactory` trait plus a resolver keyed by
name, or, if extensibility is not wanted yet, a plain serde enum with a conversion:

```rust
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RecipeProviderChoice { #[default] Default, Trivial }
```

The enum is enough for the configuration sketch and is a much smaller change; the factory trait is
only needed once a host wants to register its own provider by name. Prefer the enum, and note that
it must be matched exhaustively — no `_ =>` arm — so a third provider is a compile error rather
than a silent fallback.

## Verification

1. `RecipeProviderChoice::Default` yields a provider that resolves a recipe from a store.
2. `RecipeProviderChoice::Trivial` yields one that resolves none.
3. Round-trips through YAML and JSON.
4. Exhaustive match enforced (adding a variant fails to compile until handled).
