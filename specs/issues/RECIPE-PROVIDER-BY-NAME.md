---
id: RECIPE-PROVIDER-BY-NAME
kind: feature
title: Recipe providers cannot be selected by name, so a configuration document cannot choose one
status: closed
priority: P0
complexity: S
area: [core/assets, web]
design: recipe-provider-selection
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

An `EnvironmentConfig` (sketched in `specs/design/environment-builder/phase3-examples.md`
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

## Priority rationale

Recorded **P0** by maintainer decision (2026-08-27): this is a prerequisite for the document-driven
JavaScript and Python setup path, and that work cannot start until it lands.

Note the tension with `DOCS_STRUCTURE_GUIDE.md` §4.4, which defines P1 as "something blocking
planned work" and reserves P0 for incorrect results, data loss, a panic on a supported path, or a
documented feature that does not work. This issue is none of those; it is scheduling weight, applied
deliberately. Either §4.4 should gain a clause for hard prerequisites, or this should settle at P1.

## Verification

1. `RecipeProviderChoice::Default` yields a provider that resolves a recipe from a store.
2. `RecipeProviderChoice::Trivial` yields one that resolves none.
3. Round-trips through YAML and JSON.
4. Exhaustive match enforced (adding a variant fails to compile until handled).

## Resolution

Closed 2026-08-29 by `liquers-core::recipes::RecipeProviderChoice`, implemented on branch
`claude/recipe-provider-selection-budfor` under
[`guides/autonomous_issue_fixing.md`](../guides/autonomous_issue_fixing.md); the reasoning record is
[`design/recipe-provider-selection/`](../design/recipe-provider-selection/).

The enum route was taken, as this issue proposed: `default` and `trivial`, `#[default]` on
`Default`, four exhaustive matches with no `_` arm, plus `provider()` → `Arc<dyn
AsyncRecipeProvider<E>>`, `boxed_provider()` → `Box<…>`, `as_str()`, `FromStr` and `Display`. The
maintainer settled the set at those two on 2026-08-29: custom providers are too varied to
standardize, so they stay ad hoc and no `RecipeProviderFactory` or registration hook is added. The
same decision gave `trivial` the input aliases `none` and `no_recipes`; serialization still emits
`trivial`.

Evidence: six colocated tests in `liquers-core/src/recipes.rs` plus a rustdoc doctest cover all four
verification points — behaviour of each provider through `AsyncRecipeProvider` against an
`AsyncMemoryStore`, YAML and JSON round-trips, alias acceptance and unknown-name rejection, and the
default variant. Exhaustiveness is enforced by the compiler rather than by a test, which is recorded
in the design's Phase 3. `cargo test -p liquers-core --lib` passes 669 tests and
`scripts/check-build-matrix.sh` passes all 14 configurations, wasm32 included. No existing test
needed adjustment and no existing symbol changed.

The priority tension recorded above is left open: the P0 was scheduling weight, and reconciling
that with `DOCS_STRUCTURE_GUIDE.md` §4.4 is a change to the priority vocabulary, not to this issue.
