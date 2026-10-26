---
id: KIND-AWARE-RECIPE-PROVIDER-CHOICE
kind: design
title: A recipe provider chosen by name resolves through the environment kind, so liquers-lib keeps its manifest provider
form: compact
status: in_review
phase: implementation
readiness: needs-decision
autofix: not-eligible
area: [core/context, lib/commands, records]
issues: [CONFIGURED-RECIPE-CHOICE-DROPS-MANIFEST-PROVIDER]
created: 2026-10-26
---
# A recipe provider chosen by name resolves through the environment kind

Bulk design (`specs/guides/autonomous_bulk_design.md`), 2026-10-26: Phases 1-4 written, not
approved, nothing implemented.

## Phase 1: High-Level Design

### Purpose

With `records` on, a `liquers-lib` environment serves record-manifest chunk keys because
`LibKind::default_recipe_provider` chains `ManifestRecipeProvider` after the `recipes.yaml`
provider. Any build that names a provider — a configuration document through
`EnvironmentBuilder::with_config`, or code calling `with_recipe_provider_choice` — replaces that
chain with the bare `recipes.yaml` provider, and keyed chunks stop resolving with no error. A
configuration that says `recipes: default` (or nothing) should mean "this environment's default
recipes", which for `liquers-lib` includes manifests.

### Problem Example

Verified at HEAD (`cd6d5da`). A `liquers-lib` build with `records`, a memory store holding
`data/chunks.manifest.yaml` (`manifest: record-stream`, chunk `greet/greet.csv`):

```rust
let envref = default_environment_builder::<Value, ()>()
    // any document mounting a memory store over `data`, e.g. the `store:` sections used in
    // `liquers-core/src/environment_config.rs` tests; no `recipes:` key
    .with_config(config, Box::new(default_store_factory()))
    .build()?;
// … write the manifest through envref.get_async_store() …
evaluate(envref, "-R/data/greet.csv", None).await
```

Today: `with_config` → `with_recipe_provider_choice(RecipeProviderChoice::Default)` (the document
default) → base = `DefaultRecipeProvider` alone; `-R/data/greet.csv` fails `AssetNotFound`.
Without `with_config` (`default_environment_builder().with_async_store(…)`) it evaluates to
`"hello"`. Should: both evaluate to `"hello"`.

Latent at HEAD: the only `with_config` caller, `liquers-agent`, builds `liquers-lib` without
`records`; `liquers-py` builds with `records` and today refuses any configured choice
(`PY-RECIPE-PROVIDER-NOT-CONFIGURABLE`), whose fix hits this.

### Scope and Acceptance Criteria

- **AC-1** A configured default keeps manifests
  WHEN a `liquers-lib` environment with `records` is built with `with_config` whose `recipes` is `default` or absent
  THEN a manifest's keyed chunk resolves, as in the unconfigured build
- **AC-2** The same through the setters
  WHEN `EnvironmentBuilder::with_recipe_provider_choice(Default)` or `GenericEnvironment::with_recipe_provider_choice(Default)` is used on a `LibKind` environment
  THEN the installed provider is `LibKind`'s default chain
- **AC-3** Trivial means no recipes
  WHEN the choice is `trivial`
  THEN neither `recipes.yaml` nor manifest chunks resolve, as `RecipeProviderChoice::Trivial` documents
- **AC-4** Core kinds unchanged
  WHEN `DefaultKind`, `Queued` or `Inline` environments select a choice
  THEN they get exactly `choice.provider()`, as today
- **AC-5** A custom base still composes
  WHEN `with_recipe_provider(custom).with_records_recipe_provider()` is used
  THEN the chain is `[custom, ManifestRecipeProvider]`, as today

Non-goals: the Python binding change itself (`PY-RECIPE-PROVIDER-NOT-CONFIGURABLE`); a
configuration vocabulary for chains (deliberately absent, `RecipeProviderChoice` doc); `liquers-agent`
behaviour (no `records`).

### Design Readiness

- **Readiness:** needs-decision
- **Automatic fixing:** not-eligible — adds a defaulted method to the public trait
  `liquers_core::environment_builder::AssetManagerKind` and changes what two `pub` setters install
  for `LibKind` (rule 4); edits `liquers-core` and `liquers-lib` (rule 6).
- **Leading issue:** question 1.
- **Explanation:** both candidate mechanisms are fully specified; the choice is where a named
  provider is resolved, which is an API decision.
- **Open questions:**
  1. **Open design - mechanism:** (a) **recommended**: `AssetManagerKind::recipe_provider_for(choice)`,
     defaulted to `choice.provider()`, used by both `with_recipe_provider_choice` setters (and so by
     `with_config`); `LibKind` maps `Default` to its chain. Every path is fixed at once, including
     a future `liquers-py` or `liquers-axum` configured build, with no new call to remember.
     (b) a `liquers-lib` extension method `with_lib_config` = `with_config` +
     `with_records_recipe_provider` when the choice is `Default`; confined to `liquers-lib` but every
     configured path must call it instead of `with_config`, and `with_recipe_provider_choice(Default)`
     stays a trap.
  2. **Proposed resolution - `trivial` under `LibKind`:** no recipes at all, manifests included
     (AC-3), per the `Trivial` doc ("no recipes at all"). The alternative — `trivial` drops only
     `recipes.yaml` — would make the word mean different things per crate.
  3. **Implementation detail - `with_records_recipe_provider` after a `Default` choice:** now
     appends a second `ManifestRecipeProvider`; harmless (the first answers; the second is asked only
     on a miss). Its doc says it is for a custom base.

Complexity re-sized **S → M**: a defaulted trait method in `liquers-core` plus the `LibKind`
override and tests in `liquers-lib` (two crates).

### Design Dependencies

- `PY-RECIPE-PROVIDER-NOT-CONFIGURABLE` (issue, designed in another cluster) — **required-by**: its
  fix passes the configured choice to the builder and needs this first, or Python loses manifests.
- `records-recipe-validation-cli` — **overlaps**: that tool builds "the `liquers-lib` default
  chain" for validation; under option (a) the chain has one definition,
  `LibKind::recipe_provider_for(Default)`. No order.
- `record-streams` (complete) — Phase 4 decision 1 asked `liquers-lib` to call
  `with_records_recipe_provider` on configured paths; this supersedes that mechanism for named
  choices. E4, not extended.
- `recipe-provider-selection`, `environment-builder` (complete) — origin of `RecipeProviderChoice`
  and the builder; E4.

### Consolidated Findings

- Three entry points install a named provider: `EnvironmentBuilder::with_recipe_provider_choice`
  (`environment_builder.rs`), `EnvironmentBuilder::with_config` (via the former,
  `environment_config.rs`) and `GenericEnvironment::with_recipe_provider_choice` (`context.rs`,
  also behind `with_default_recipe_provider` / `with_trivial_recipe_provider`). All must route
  through the kind.
- `RecipeProviderChoice::provider` / `boxed_provider` stay kind-agnostic (they have no kind), and
  their docs say so.
- `LibKind::default_recipe_provider` should become `Self::recipe_provider_for(Default)` so the two
  cannot drift.
- `liquers-lib`'s `RecordsRecipeProvider` doc ("`liquers-lib` has no such configured-construction
  path yet — nothing here calls it") and `LibKind::default_recipe_provider`'s doc paragraph on
  bypassing change.

## Phase 2: Architecture

### Solution

Option (a):

1. `liquers-core/src/environment_builder.rs` `AssetManagerKind`:

   ```rust
   /// The provider a named choice installs in an environment of this kind. Defaults to
   /// `choice.provider()`; a kind whose default carries extra providers maps `Default` to it.
   fn recipe_provider_for<E: Environment>(choice: RecipeProviderChoice) -> Arc<dyn AsyncRecipeProvider<E>> {
       choice.provider()
   }
   ```

   `EnvironmentBuilder::with_recipe_provider_choice` calls `K::recipe_provider_for::<GenericEnvironment<V, P, K>>(choice)`.
2. `liquers-core/src/context.rs` `GenericEnvironment::with_recipe_provider_choice`:
   `self.recipe_provider = K::recipe_provider_for(choice);`.
3. `liquers-lib/src/environment.rs` `impl AssetManagerKind for LibKind`:
   `recipe_provider_for` with an explicit `match choice { Default => <today's chain body>, Trivial => choice.provider() }`
   (the `#[cfg(feature = "records")]` split moves here), and
   `default_recipe_provider` → `Self::recipe_provider_for(RecipeProviderChoice::Default)`.
4. Docs in code: `LibKind::default_recipe_provider`, `RecordsRecipeProvider` (custom base only;
   a `Default` choice already carries manifests), `RecipeProviderChoice::provider` (kind-agnostic;
   environments resolve through the kind).

Rejected: option (b) (Phase 1); a `RecipeProviderChoice::Chain` / `Lib` variant (a choice cannot
name a provider in another crate — the documented reason it has none).

### Changes

- `liquers-core/src/environment_builder.rs`: `AssetManagerKind::recipe_provider_for` (new,
  defaulted), `EnvironmentBuilder::with_recipe_provider_choice`.
- `liquers-core/src/context.rs`: `GenericEnvironment::with_recipe_provider_choice`.
- `liquers-lib/src/environment.rs`: `LibKind` (two methods), `RecordsRecipeProvider` doc.
- No command, registry, config schema or binding change; `liquers-py` compiles unchanged (it does
  not implement `AssetManagerKind`). Every other `AssetManagerKind` impl inherits the default.
- Documents: `specs/reference/ENVIRONMENT_CONFIG.md` §The recipe provider chain (lines ~184-200
  today say a configured base "loses manifest support without a word" and prescribe
  `with_records_recipe_provider()`; that becomes "custom base only"),
  `specs/guides/ENVIRONMENT_CONSTRUCTION_GUIDE.md` and
  `specs/guides/ASSET_MANAGER_IMPLEMENTATION_GUIDE.md` where they list `AssetManagerKind`'s
  methods, `specs/reference/RECIPES_AND_PLANS.md` §Composing providers — History rows,
  `reviewed:`.

### Risks

| Risk | Mitigation | Certainty |
|---|---|---|
| A `liquers-lib` user relied on `with_recipe_provider_choice(Default)` meaning `recipes.yaml` only | the manifest provider answers only `*.manifest.yaml`-backed keys and refuses names a sibling `recipes.yaml` defines, so existing keys resolve the same | high |
| Double `ManifestRecipeProvider` after `Default` + `with_records_recipe_provider` | harmless; documented; existing test uses `Trivial`, unaffected | high |
| `records` off builds | the `cfg` split moves unchanged; run the feature matrix | high |

Recovery: revert; the trait method is defaulted, so removal breaks nothing outside `LibKind`.

## Phase 3: Examples and Tests

### Examples

The Problem Example evaluates `-R/data/greet.csv` to `"hello"` (the existing manifest fixture of
`with_records_recipe_provider_serves_a_manifest_chunk_over_a_trivial_base`: chunk query
`greet/greet.csv`, command `greet`).

### Tests

- `liquers-lib/src/environment.rs` `mod tests` (`#[cfg(feature = "records")]`, same fixture):
  - `configured_default_serves_a_manifest_chunk` — `with_config` from a YAML with a memory store
    and no `recipes:`; write the manifest after `build()`; evaluate. Fails at HEAD. Covers AC-1.
  - `default_choice_on_lib_kind_serves_a_manifest_chunk` — builder
    `with_recipe_provider_choice(Default)` + `with_async_store`; and a `DefaultEnvironment::new()`
    with `with_recipe_provider_choice(Default)` and `with_async_store`. Covers AC-2.
  - `trivial_choice_on_lib_kind_serves_no_manifest_chunk` — `with_config` with `recipes: trivial`:
    evaluation fails `AssetNotFound`. Covers AC-3.
  - `with_records_recipe_provider_serves_a_manifest_chunk_over_a_trivial_base` — existing, plus a
    `with_recipe_provider(Arc::new(DefaultRecipeProvider::new()))` variant. Covers AC-5.
- `liquers-core/src/environment_builder.rs` `mod tests`:
  `named_choice_resolves_through_the_kind` — a test kind overriding `recipe_provider_for` to return
  a marker provider for `Default`; `with_recipe_provider_choice(Default)` installs it; `DefaultKind`
  with `Trivial` resolves no recipe. Covers AC-2, AC-4.

Run: `cargo test -p liquers-core --lib environment_builder`; `cargo test -p liquers-lib --lib environment`;
`cargo test -p liquers-lib --no-default-features --features records --lib --tests`.

## Phase 4: Implementation Plan

### Steps

- [ ] 1. Tests in `liquers-lib/src/environment.rs` and `liquers-core/src/environment_builder.rs`; `configured_default_serves_a_manifest_chunk` fails at HEAD — the run commands above. Rollback: delete the tests.
- [ ] 2. `liquers-core/src/environment_builder.rs` `AssetManagerKind::recipe_provider_for`, `EnvironmentBuilder::with_recipe_provider_choice`; `liquers-core/src/context.rs` `GenericEnvironment::with_recipe_provider_choice` — depends on 1 — `cargo test -p liquers-core --lib --tests`. Rollback: revert both (defaulted, nothing else depends yet).
- [ ] 3. `liquers-lib/src/environment.rs` `LibKind::recipe_provider_for` / `default_recipe_provider`, doc updates — depends on 2 — `cargo test -p liquers-lib --lib --tests`, `bash scripts/check-build-matrix.sh`. Rollback: revert the file.
- [ ] 4. References (History, `reviewed:`); close `CONFIGURED-RECIPE-CHOICE-DROPS-MANIFEST-PROVIDER`; note in `PY-RECIPE-PROVIDER-NOT-CONFIGURABLE` that the prerequisite landed — `python3 scripts/docs_index.py --check`.

### Validation

`cargo test -p liquers-core --lib --tests`; `cargo test -p liquers-lib --lib --tests`;
`cargo test -p liquers-agent`; `bash scripts/check-build-matrix.sh` (records on/off, wasm32);
`python3 scripts/docs_index.py --check`; diff review: three source files, their tests, the
references, the two issues.
