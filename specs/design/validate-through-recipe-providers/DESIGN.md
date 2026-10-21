---
id: VALIDATE-THROUGH-RECIPE-PROVIDERS
kind: design
title: Validate the recipes an environment's recipe provider chain serves
form: compact
status: in_review
phase: implementation
readiness: needs-decision
autofix: not-eligible
area: [core/validate, core/plan]
issues: [VALIDATE-CANNOT-SEE-NON-STANDARD-RECIPE-PROVIDERS]
affects_docs: [specs/reference/api/DOC_08_RECIPES_PLANS.md, .claude/skills/liquers-validate/SKILL.md]
created: 2026-10-20
---
# Validate the recipes an environment's recipe provider chain serves

Bulk design (`specs/guides/autonomous_bulk_design.md`), 2026-10-20: Phases 1-4 written, not
approved, nothing implemented. First deliverable slice; the `liquers-lib`-linked CLI is the
follow-up `LIB-RECIPE-PROVIDERS-HAVE-NO-VALIDATION-CLI`.

## Phase 1: High-Level Design

### Purpose

`liquers-validate` plans the recipes of one `recipes.yaml` file it parses itself. Recipes served by
any other `AsyncRecipeProvider` — a record manifest's chunks (`ManifestRecipeProvider`), a
generative provider, an integration's own — are planned by nothing until someone evaluates them.
Validation should go through the provider chain, so every provider is covered by one mechanism
(VISION: recipes as declarative, checkable data).

### Problem Example

`data/sales.manifest.yaml` holds the explicit chunk
`-R/raw/sales/jan.csv/-/ns-rec/to_recrd/jan.csv` (typo). The manifest loads (its structure is
checked) and the chunk query parses, but nothing plans it: the error
`Action 'to_recrd' not registered` (what `liquers-validate -- '<chunk query>'` reports, verified
2026-10-20) appears only when `-R/data/jan.csv` is evaluated. Likewise `liquers-validate` sees a
folder tree's `recipes.yaml` files only one `--recipes FILE` at a time. Should: a library call over
an environment validates every recipe its provider lists under a folder (and named extra keys),
returning the same `ValidationResult`s; the core CLI exposes it for a directory tree with the
default provider.

### Scope and Acceptance Criteria

- **AC-1** Listed recipes are planned
  WHEN `validate_provider_recipes` runs over folders of an environment
  THEN every key `assets_with_recipes` lists is fetched with `recipe` and planned against the given registry, one `ValidationResult` per key, with the key and its provider-assigned `cwd` respected
- **AC-2** Any provider
  WHEN the environment's provider is a `RecipeProviderChain` holding `ManifestRecipeProvider`
  THEN a manifest's explicit chunks are validated, and the Problem Example's typo is an `Error` result naming the chunk key
- **AC-3** Generated keys by request
  WHEN extra keys are given that a provider makes but does not list (template chunks)
  THEN each is validated through `recipe_opt` and marked as not listed; a key no provider makes is an `Error` result
- **AC-4** Provider failures are findings
  WHEN a folder's listing or a recipe lookup fails (malformed YAML, colliding chunk names)
  THEN that becomes an `Error` result for the folder or key, and the remaining folders are still validated
- **AC-5** Tree mode in the core CLI
  WHEN `liquers-validate --store-root DIR [--folder KEY]…` runs
  THEN it reads `DIR` through a read-only file store with `DefaultRecipeProvider`, validates every listed recipe under the folders (default: the whole tree), and uses the same envelope and exit codes as `--recipes`
- **AC-6** Default behaviour unchanged
  WHEN the CLI runs without `--store-root`
  THEN it opens no store, exactly as today

Non-goals: evaluating anything; enumerating all of a generative provider's keys; a CLI linked
against `liquers-lib` (follow-up); a new `AsyncRecipeProvider` method.

### Design Readiness

- **Readiness:** needs-decision
- **Automatic fixing:** not-eligible — new `pub` functions and a new `ValidationResult` field in
  `liquers-core`, a new CLI option (rule 4).
- **Leading issue:** question 1 (store access in the core CLI).
- **Explanation:** the library and CLI mode are fully described in `liquers-core`; the open
  questions concern the tool's "never opens a store" promise and how generative keys are sampled.
- **Open questions:**
  1. **Open design - store in the core CLI:** an opt-in `--store-root` that opens a read-only file
     store (recommended: still offline and side-effect-free; the skill text "never opens a store"
     becomes "never unless `--store-root` is given"), or library only.
  2. **Open design - sampling generative keys:** the caller names keys (recommended for this slice)
     versus a defaulted provider hook `example_keys(dir)` that `ManifestRecipeProvider` answers with
     its first template chunks (a trait addition; follow-up if wanted).
  3. **Proposed resolution - library placement:** `liquers_core::validate::provider`, async, the one
     async part of `validate` (it must read through a store); results reuse `ValidationResult`.

### Design Dependencies

- `query-validation` (complete) — origin of the tool; E4, not extended.
- `registry-query-default-text-form` — **overlaps**: also changes what the validator reads
  (registry files); no order.
- `recipe-provider-listing-contract` (complete) — the `contains` / `can_make` contract this relies
  on; E4.
- `recipe-override-addressing` — **overlaps**: recipes using qualified names validate through
  `Recipe::to_plan` with no change here.

### Consolidated Findings

- Providers need an `EnvRef<E>` (they read through the store), so provider validation cannot be
  synchronous or store-free; the core CLI's default path stays both.
- `validate_recipe` derives the key from `Recipe::store_to_key`; a provider recipe must instead be
  planned with `to_plan_for_key` against the key it was served for (manifest chunks), hence
  `validate_recipe_for_key`.
- The motivating case (manifests) is reached by the library from any crate, and from a CLI only
  after the follow-up, because `ManifestRecipeProvider` lives above core.

## Phase 2: Architecture

### Solution

A new module `liquers-core/src/validate/provider.rs`:

```rust
pub struct ProviderValidation { pub folders: Vec<Key>, pub extra_keys: Vec<Key>, pub deep: bool }

/// Validates what the environment's recipe provider serves. Err only for an invocation error
/// (an unparseable folder); provider failures are Error results (AC-4).
pub async fn validate_provider_recipes<E: Environment>(
    envref: EnvRef<E>,
    request: &ProviderValidation,
    level: ValidationLevel,
    cmr: &CommandMetadataRegistry,
) -> Result<Vec<ValidationResult>, Error>;
```

Folders: each given folder, plus (when `deep`) the distinct parents of
`envref.get_async_store().listdir_keys_deep(folder)`. Per folder: `provider.has_recipes`, then
`assets_with_recipes`; per listed key `provider.recipe(key)` then `validate_recipe_for_key`. Extra
keys go through `recipe_opt`. `liquers-core/src/validate/mod.rs` gains
`pub fn validate_recipe_for_key(recipe, key, index, level, cmr) -> ValidationResult` (as
`validate_recipe`, but `RecipeCheck::Stored` with the served key), and `report.rs`
`ValidationResult` gains `#[serde(skip_serializing_if = "Option::is_none", default)] pub listed:
Option<bool>` (additive; `None` for query and file inputs).

CLI (`liquers-core/src/bin/liquers_validate.rs`): `--store-root DIR`, repeatable `--folder KEY`,
`--key KEY`; conflicts with `--recipes`/`--query-file`/queries. It builds a
`GenericEnvironment` with `with_async_store(AsyncFileStore::new(DIR, &Key::new()))` wrapped
read-only (`store/read_only.rs`), `DefaultRecipeProvider`, runs the library on a current-thread
tokio runtime, and reuses `build_report`. The registry is the one the CLI already loads.

Rejected: parsing manifests in the CLI (one file format per provider, the problem itself); a new
provider trait method in this slice (Q2).

Known-issue preflight: `DefaultRecipeProvider::get_recipes` maps any read failure to an empty list
(DOC-08 P1 gap) — a listing error may be invisible; noted, not blocking.

### Changes

Files: `validate/provider.rs` (new), `validate/mod.rs` (`validate_recipe_for_key`, re-exports),
`validate/report.rs` (`listed`), `bin/liquers_validate.rs` (options). Errors: invocation errors
`Error::general_error`; findings as `ValidationResult::failed` with the provider's error. Async
only in the provider module. Documents: `.claude/skills/liquers-validate/SKILL.md` (tree mode and
the store caveat), DOC-08 §Provider contract (a validation paragraph). No command or registry
change.

### Risks

| Risk | Mitigation | Certainty |
|---|---|---|
| Large trees: deep listing is slow | `--folder` narrows; listing is the store's own | high |
| `get_recipes` hides read errors | documented; DOC-08 gap | high |
| The CLI needs a runtime | core's tokio already has `rt` and `macros` (`Cargo.toml`); `cli` adds nothing | high |
| JSON envelope consumers see a new field | additive, omitted when `None` | high |

## Phase 3: Examples and Tests

### Examples

`liquers-validate --store-root examples/data --folder reports` validates every recipe of
`examples/data/reports/recipes.yaml` and its subfolders. In Rust (e.g. a `liquers-records` test):

```rust
let results = validate_provider_recipes(envref, &ProviderValidation {
    folders: vec![parse_key("data")?], extra_keys: vec![parse_key("data/sales_0000.csv")?], deep: false,
}, ValidationLevel::Plan, &cmr).await?;
```

### Tests

- `liquers-core/src/validate/provider.rs` `mod tests` (memory store, `DefaultRecipeProvider`):
  `lists_and_plans_every_recipe_in_a_tree` (AC-1), `unknown_extra_key_is_an_error_result` (AC-3),
  `malformed_recipes_yaml_is_a_folder_error_and_others_continue` (AC-4),
  `served_key_is_used_for_planning` (AC-1).
- `liquers-records/tests/manifest_validation.rs` (new, core + records only):
  `manifest_chunk_typo_is_reported` — explicit chunk with `to_recrd` gives an `Error` result for
  `data/jan.csv` (AC-2); `template_chunk_validated_when_named` (AC-3).
- `liquers-core/tests/validate_integration.rs` (existing):
  `store_root_validates_a_tree` over a temp dir (AC-5), `no_store_without_store_root` (AC-6).
- Run: `cargo test -p liquers-core --features cli --lib validate`, `cargo test -p liquers-core --features cli --test validate_integration`,
  `cargo test -p liquers-records --lib --tests`.

## Phase 4: Implementation Plan

### Steps

- [ ] 1. `validate/report.rs` `listed`; `validate/mod.rs` `validate_recipe_for_key` — `cargo test -p liquers-core --lib validate`. Rollback: revert (additive).
- [ ] 2. `validate/provider.rs` `ProviderValidation`, `validate_provider_recipes` and its unit tests — `cargo test -p liquers-core --lib validate::provider`. Depends on 1. Rollback: delete the module.
- [ ] 3. `bin/liquers_validate.rs` `--store-root`, `--folder`, `--key` — `cargo test -p liquers-core --features cli --test validate_integration`. Depends on 2. Rollback: revert the bin.
- [ ] 4. `liquers-records/tests/manifest_validation.rs` — `cargo test -p liquers-records --test manifest_validation`. Depends on 2.
- [ ] 5. Documents: SKILL.md, DOC-08 (History row, `reviewed:`); close the source issue; the follow-up `LIB-RECIPE-PROVIDERS-HAVE-NO-VALIDATION-CLI` stays open — `python3 scripts/docs_index.py --check`.

### Validation

`cargo test -p liquers-core --features cli --lib --tests`; `cargo test -p liquers-records --lib --tests`;
no `cfg(feature)` or dependency change, so no build matrix.
