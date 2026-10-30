# Phase 5: Documentation - recipe-provider-chain-config

## Completion Preconditions

- [x] Implementation is finished and validated
- [x] All user comments are answered or incorporated
- [x] All review comments are answered or incorporated
- [x] Documentation is consistent with the implemented and tested behavior
- [x] Documentation is included in the implementation PR when practical (PR #224)

## Implementation Summary

Every `GenericEnvironment` now holds a `RecipeProviderChain`, configured as an ordered list of
entries, as Orest asked on 2026-10-30. The implementation conforms to the approved Phases 1-4.

- **Configuration.** `EnvironmentConfig.recipes` is a `RecipesConfig`. Absent or `default` is the
  kind's list. `trivial` (aliases `none`, `no_recipes`) is `[]`. A list holds entries, each a bare
  name or `{type: …, args…}`, and `manifest` ≡ `{type: manifest}`. A `default` entry splices the
  kind's list. Written back, the kind default omits the key.
- **Entry types.** `RecipeProviderFactory`, `RecipeProviderTypeInfo`, `RecipeProviderTypeMap` and
  `ChainedRecipeProviderFactory` follow the store-factory model. `resolve_recipes` is the single
  definition of a chain. Core declares `recipes_yaml`. `liquers-lib` declares `manifest`, which is
  unavailable without `records`. `EnvironmentBuilder::with_recipe_provider_factory` lets an
  integration add its own types.
- **The chain asks every member.** The one member with a recipe owns the key. Two owners is a
  `ConfigurationError` naming both. An error from any member fails the lookup; this was decided at
  the Phase 2 gate. `ManifestRecipeProvider` dropped its own `recipes.yaml` check, because the chain
  rule covers it for every pair.
- **Setters.**
  - New on the builder: `with_recipes` and `with_appended_recipe_entry`, which is idempotent.
  - New on `GenericEnvironment`: `try_with_recipes`.
  - `with_recipe_provider_choice` becomes `with_recipes(choice.into())`. Its infallible
    environment form installs an `UnresolvedRecipeProvider` when a kind's own default does not
    resolve.
  - `with_records_recipe_provider` appends the `manifest` entry.
- **Results.**
  - A configured `liquers-lib` build keeps manifests: through a document, `default`, or
    `with_default_recipe_provider()`, which is the `liquers-web` path
    (`CONFIGURED-RECIPE-CHOICE-DROPS-MANIFEST-PROVIDER`).
  - `liquers-py` accepts every `recipes` value (`PY-RECIPE-PROVIDER-NOT-CONFIGURABLE`).

**Additions beyond Phase 2, all small:**

- `AssetManagerKind::unconfigured_recipes()` lets the builder resolve the unconfigured case through
  `resolve_recipes`, so an appended entry can see what is already in the chain. It is `[]` for the
  core kinds and `KindDefault` for `LibKind`. It must describe the same chain as
  `default_recipe_provider()`, and `LibKind` now builds that chain from its list.
- `resolve_recipes` takes `&ChainedRecipeProviderFactory<E>` rather than
  `&dyn RecipeProviderFactory<E>`. The chained type's `create_at` adds the entry position to errors.
- A base given by `with_recipe_provider` is the member `provider`.
- `UnresolvedRecipeProvider` and `lib_recipe_provider_factory` are public.

## Documentation Delivered

### New Reference Documents
None. The chain's contract belongs in the existing environment-configuration and recipes
references.

### New Guide Documents
None. Configuring and extending the chain is a section of the existing construction guide.

### Existing Documents Reviewed or Updated

Each document below got a `## History` row and `reviewed: 2026-10-30`.

- `reference/ENVIRONMENT_CONFIG.md`: the `recipes` grammar; the per-kind defaults table; §The recipe
  provider chain, rewritten (entries, factories, every-member rule, setters, `LibKind`, the
  migration hazard, feature unification for `liquers-agent`); the build-time entry errors.
- `reference/RECIPES_AND_PLANS.md`: §Composing providers and §Selecting a provider by name.
- `reference/PYTHON_INTEGRATION.md` §3 and §8; `guides/PYTHON_GUIDE.md` §4, §6 and §10.
- `reference/JAVASCRIPT_INTEGRATION.md` §3: manifests are kept.
- `guides/ENVIRONMENT_CONSTRUCTION_GUIDE.md`: setters table, document example, §Library defaults,
  declaring an entry type.
- `guides/LANGUAGE-INTEGRATION_GUIDE.md` §RECIPE: a language provider is supplied as an entry type.
  This also corrects a wrong claim: a chain's `contains` asks each member's `contains`, not its
  `recipe_opt`.
- Also updated, outside `affects_docs`, because they restated the old chain:
  - `reference/RECORD_STREAMS.md`
  - `guides/RECORD_STREAM_GUIDE.md` (§3.4, pitfall 14)
  - `reference/PROJECT_OVERVIEW.md`
- `liquers-py/README.md`: the `ENVIRON` row. `liquers-web/README.md` needed no change; its only
  recipe line is about store `contains`.

### Links and Capability Map
`specs/README.md` places the design under §Assets and their lifecycle, as "Recipe-provider chain
configured as a list", and removes it from §Not yet placed.

## Issues Filed

- `PY-NO-LANGUAGE-DEFINED-RECIPE-PROVIDER`: a recipe provider cannot yet be written in Python.
  That is the liquers-py M4 `RECIPE` feature, now reduced to an entry type, a registry and an
  adapter.

Closed: `RECIPE-PROVIDER-CHAIN-NOT-CONFIGURABLE`, `CONFIGURED-RECIPE-CHOICE-DROPS-MANIFEST-PROVIDER`
and `PY-RECIPE-PROVIDER-NOT-CONFIGURABLE`. The superseded designs `kind-aware-recipe-provider-choice`
and `py-recipe-provider-choice` stay `superseded`.

## Important Learning

- **Order-independence comes from asking every member.** Before this design, the
  `recipes.yaml`/manifest clash rule held only because the manifest provider read `recipes.yaml`
  itself. Asking every member makes the rule hold for any two providers. It also turns a misbehaving
  member into a visible error instead of a shadowed one.
- **A kind default is now data.** `default_recipes()` is a list of entries, so a document, a setter
  and an unconfigured environment resolve the same chain by construction instead of by convention.
- **The cost of the strict error rule.** An exception or malformed file in any provider fails every
  lookup. That suits Liquers' providers, which rarely fail. Language integrations have to follow it,
  and §RECIPE says so.

## Conformance and Remaining Work

The requested scope, the approved scope and the implemented scope match. The Python-defined provider
is out of scope by design (Phase 1); it is filed as `PY-NO-LANGUAGE-DEFINED-RECIPE-PROVIDER` and
planned for M4. Nothing else remains.

## Validation

- `cargo test -p liquers-core --lib --tests`: all suites pass, including the 12 tests of
  `tests/recipe_provider_chain_config.rs`.
- `cargo test -p liquers-lib --lib --tests`: all 41 suites pass. `recipe_provider_chain_config`
  also passes with `--no-default-features` (2 tests) and with
  `--no-default-features --features records` (8 tests).
- `cargo test -p liquers-records --all-features --lib --tests`: passes.
- `bash scripts/test-liquers-py.sh`: 244 passed and 6 skipped, plus 21 Rust unit tests.
- `bash scripts/check-build-matrix.sh` (wasm32 included): all 44 configurations OK.
- After merging `main`: `cargo test -p liquers-core --lib --tests`, `cargo test -p liquers-agent` and
  `cargo test -p liquers-axum` pass; CI green on the PR head.
- `python3 scripts/docs_index.py --check`: clean.
