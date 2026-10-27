---
id: RECORDS-RECIPE-VALIDATION-CLI
kind: design
title: A command-line validator for the recipes the liquers-lib provider chain serves, manifests included
form: compact
status: in_review
phase: implementation
readiness: needs-decision
autofix: not-eligible
area: [core/validate, records]
issues: [LIB-RECIPE-PROVIDERS-HAVE-NO-VALIDATION-CLI]
created: 2026-10-26
---
# A command-line validator for the recipes the liquers-lib provider chain serves

Bulk design (`specs/guides/autonomous_bulk_design.md`), 2026-10-26: Phases 1-4 written, not
approved, nothing implemented. **Built on an unimplemented parent:** this design assumes the
library call `validate_provider_recipes` exactly as `validate-through-recipe-providers` Phase 2
specifies it; if that signature changes, Phases 2-4 here change with it.

## Phase 1: High-Level Design

### Purpose

`validate-through-recipe-providers` gives a library call that validates whatever an environment's
recipe provider serves, and a `liquers-validate --store-root` mode limited to core's
`DefaultRecipeProvider`. The provider that motivated it, `ManifestRecipeProvider`, lives in
`liquers-records`, above core, so no command line reaches a manifest's chunk recipes. Authors of
record manifests need the same offline check `recipes.yaml` authors have (VISION: recipes as
declarative, checkable data).

### Problem Example

`data/sales.manifest.yaml` (a `record-stream` manifest) holds the explicit chunk
`-R/raw/sales/jan.csv/-/ns-rec/to_recrd/jan.csv` (typo for `to_record`). Today:
`liquers-validate -- '-R/raw/sales/jan.csv/-/ns-rec/to_recrd/jan.csv'` finds the typo, but only if
the author copies the query out by hand; `liquers-validate --store-root …` (once the parent lands)
reads `recipes.yaml` only, so the chunk is not visited; the error appears when `-R/data/jan.csv`
is first evaluated. Should be:

```bash
cargo run -p liquers-records --features cli --bin liquers-validate-records -- \
  --store-root ./store --folder data
```

reports an `Error` result for key `data/jan.csv` ("Action 'to_recrd' not registered; …"), exit 1,
in the same envelope as `liquers-validate`.

### Scope and Acceptance Criteria

- **AC-1** Manifest chunks are validated from the command line
  WHEN `liquers-validate-records --store-root DIR --folder KEY` runs over a folder holding a manifest with explicit chunks
  THEN every listed chunk key gets one result, planned against the registry, and the Problem Example's typo is an `Error` naming `data/jan.csv`
- **AC-2** `recipes.yaml` recipes are validated too
  WHEN the same folder also holds a `recipes.yaml`
  THEN its recipes are validated in the same run, `recipes.yaml` first, as the `liquers-lib` chain orders them
- **AC-3** Template chunks by request
  WHEN `--key KEY` names a template-generated chunk key
  THEN it is validated and marked not listed (parent AC-3)
- **AC-4** Same envelope and exit codes
  WHEN the run finishes
  THEN stdout is the `liquers-validate` JSON/YAML envelope (`--format`, `--detail`), exit 0 for ok or warnings, 1 for any error, 2 for an invocation error with empty stdout
- **AC-5** One chain definition
  WHEN `liquers-lib`'s `LibKind` default (records on) and this tool build their provider chain
  THEN both call the same `liquers-records` function, so they cannot drift
- **AC-6** Nothing is written
  WHEN the tool runs
  THEN the store is opened read-only and no command is executed

Non-goals: evaluating chunks; linking `liquers-lib` (its commands and optional dependencies); a
registry other than the committed `specs/command_registry.yaml` or `--registry-file`;
enumerating all template chunks (parent question 2).

### Design Readiness

- **Readiness:** needs-decision
- **Automatic fixing:** not-eligible — new binary, new `cli` feature and a new optional dependency
  in `liquers-records`, and a new `pub` function there (rule 4); `liquers-lib` also edited (rule 6).
- **Leading issue:** question 1 (host crate).
- **Explanation:** under the recommended host the tool is fully specified on the parent's
  specified API; the host choice decides build cost and which registry is used.
- **Open questions:**
  1. **Open design - host crate:** (a) **recommended**: `liquers-records`, feature `cli`, bin
     `liquers-validate-records`, planning against the committed registry file like
     `liquers-validate` — builds `liquers-core` + `liquers-records` only (cheap; CLAUDE.md's disk
     limits), same registry and envelope as the core tool. (b) `liquers-lib`, feature `cli` beside
     `export-command-registry` — plans against the live registered commands (no registry file to
     go stale) and uses `LibKind`'s chain directly, but links all of `liquers-lib`.
  2. **Inherited - parent questions:** `validate-through-recipe-providers` question 1 (store access
     in the *core* CLI) does not bind here — a separate tool whose purpose is reading a store; its
     question 2 (sampling generative keys) is answered the same way (`--key`).
  3. **Proposed resolution - binary name:** `liquers-validate-records` (says what it adds; avoids
     clashing with `liquers-validate`).

Complexity re-sized **S → M**: a feature, a dependency, a binary, a `pub` function and an
integration test in `liquers-records`, plus a one-line `liquers-lib` change.

### Design Dependencies

- `validate-through-recipe-providers` — **requires**: supplies `validate_provider_recipes`,
  `ProviderValidation`, `validate_recipe_for_key` and the `listed` result field. Must land first.
- `kind-aware-recipe-provider-choice` — **overlaps**: both touch `LibKind`'s chain definition
  (`default_recipe_provider` / `recipe_provider_for(Default)`); whichever lands second calls the
  shared `liquers-records` function from the place the other left it. No order.
- `recipes-yaml-read-errors-propagate` — **overlaps**: after it, an unreadable `recipes.yaml`
  becomes a folder `Error` result here instead of an empty folder. No order.

### Consolidated Findings

- `ManifestRecipeProvider` has no bound on the value type, so the tool can use
  `liquers_core::value::Value` and `GenericEnvironment` without `liquers-lib`.
- `liquers-records` already depends on `tokio` with `rt` and `macros`; `cli` adds only `clap` and
  `liquers-core/cli` (for the shared `validate` report and registry builder).
- The core bin's registry discovery (`find_committed_registry`, walking up for
  `specs/command_registry.yaml`) is private to that bin; the records bin repeats those ~15 lines
  rather than adding a `pub` item to core.

## Phase 2: Architecture

### Solution

Option (a):

1. `liquers-records/src/provider.rs`:

   ```rust
   /// The recipe providers `liquers-lib` installs by default with `records`: `recipes.yaml` first,
   /// then manifests. Shared so the library default and the validation tool cannot drift.
   pub fn standard_recipe_providers<E: Environment>() -> Vec<Arc<dyn AsyncRecipeProvider<E>>> {
       vec![RecipeProviderChoice::Default.provider::<E>(), Arc::new(ManifestRecipeProvider::new())]
   }
   ```

2. `liquers-lib/src/environment.rs` `LibKind` (records on):
   `Arc::new(RecipeProviderChain::new(liquers_records::provider::standard_recipe_providers()))`
   in place of the inline `vec!` (in `default_recipe_provider`, or in `recipe_provider_for` if
   `kind-aware-recipe-provider-choice` landed first).
3. `liquers-records/Cargo.toml`: `cli = ["liquers-core/cli", "dep:clap"]`, `clap` optional (same
   version spec as `liquers-core`), `[[bin]] name = "liquers-validate-records" path =
   "src/bin/liquers_validate_records.rs" required-features = ["cli"]`.
4. `liquers-records/src/bin/liquers_validate_records.rs`: clap args `--store-root DIR` (required),
   `--folder KEY` (repeatable; default the root, deep), `--key KEY` (repeatable),
   `--registry-file FILE` (repeatable; default: walk up for `specs/command_registry.yaml`),
   `--level`, `--format`, `--detail`, `--quiet` — the same names and meanings as
   `liquers-validate`. It builds
   `GenericEnvironment<Value>` with `ReadOnlyStore::new(Arc::new(AsyncFileStore::new(DIR, &Key::new())))`
   and `RecipeProviderChain::new(standard_recipe_providers())`, runs
   `validate_provider_recipes(envref, &ProviderValidation { folders, extra_keys, deep: true }, level, &registry)`
   on a current-thread runtime, then `build_report` and prints the envelope to stdout, diagnostic
   lines to stderr unless `--quiet`, and exits with `report.exit_code()`; an invocation error
   prints to stderr and exits 2 with empty stdout.

Rejected: option (b) (Phase 1); teaching core's `liquers-validate` about manifests (core cannot
depend on `liquers-records`).

### Changes

- `liquers-records`: `Cargo.toml` (feature, dependency, bin), `src/provider.rs`
  (`standard_recipe_providers`), `src/bin/liquers_validate_records.rs` (new),
  `tests/validate_records_cli.rs` (new, `#![cfg(feature = "cli")]`).
- `liquers-lib/src/environment.rs`: `LibKind` uses `standard_recipe_providers`.
- Errors: invocation errors as `Error::general_error`; findings come from the library call.
- Documents: `.claude/skills/liquers-validate/SKILL.md` (a "manifests" section pointing at the
  tool), `specs/reference/RECORD_STREAMS.md` §Sources, manifests and keyed chunks (validation
  paragraph), `CLAUDE.md` "Validating queries" (one line) — History rows, `reviewed:`.

### Risks

| Risk | Mitigation | Certainty |
|---|---|---|
| Parent API changes before landing | this design names its assumption; re-review Phase 2 when the parent is approved | medium |
| Registry file stale relative to code | same as `liquers-validate`; `registry_export` test keeps it fresh | high |
| `cli` leaks `clap` into default builds | optional dependency, `required-features`; run `scripts/check-build-matrix.sh` records rows | high |
| Writing to the store | `ReadOnlyStore`; AC-6 test checks the tree is unchanged | high |

Recovery: remove the bin and feature; revert the `LibKind` line (behaviour identical).

## Phase 3: Examples and Tests

### Examples

The Problem Example. Query checks with the prebuilt validator (2026-10-26):
`-R/raw/sales/jan.csv/-/ns-rec/to_recrd/jan.csv` → `Error` "Action 'to_recrd' not registered;
command keys tried: …"; the corrected `-R/raw/sales/jan.csv/-/ns-rec/to_record/jan.csv` → `Ok`.

### Tests

`liquers-records/tests/validate_records_cli.rs` (`#![cfg(feature = "cli")]`, runs
`env!("CARGO_BIN_EXE_liquers-validate-records")` over a temp dir, registry file written by the
test with `to_text` and `ns-rec/to_record` entries):

- `manifest_chunk_typo_is_an_error_result` — the Problem Example manifest; JSON stdout has an
  `Error` result keyed `data/jan.csv`; exit 1. Covers AC-1.
- `recipes_yaml_and_manifest_in_one_run` — a `recipes.yaml` with one recipe beside the manifest;
  both keys have results, the `recipes.yaml` one first. Covers AC-2.
- `template_chunk_validated_by_key` — a template manifest; `--key data/sales_0000.csv` gives one
  result with `listed: false`. Covers AC-3.
- `envelope_and_exit_codes` — clean folder exits 0; `--format yaml` parses; an unknown option exits
  2 with empty stdout. Covers AC-4.
- `store_is_not_written` — snapshot of the temp tree (paths and bytes) equal before and after.
  Covers AC-6.

`liquers-lib/src/environment.rs` `mod tests`: the existing
`with_records_recipe_provider_serves_a_manifest_chunk_over_a_trivial_base` and
`default_environment_has_a_recipe_provider` stay green, and a new
`default_chain_is_the_standard_records_chain` (`#[cfg(feature = "records")]`) evaluates a
`recipes.yaml` key and a manifest chunk key through `DefaultEnvironment::new()`. Covers AC-5.

Run: `cargo test -p liquers-records --features cli --test validate_records_cli`;
`cargo test -p liquers-lib --lib environment`.

## Phase 4: Implementation Plan

### Steps

- [ ] 1. Precondition: `validate-through-recipe-providers` implemented (its `validate_provider_recipes` exists); re-check this design's Phase 2 against its final signature; validate the fixture queries with `liquers-validate`. Rollback: none (read-only).
- [ ] 2. `liquers-records/src/provider.rs` `standard_recipe_providers`; `liquers-lib/src/environment.rs` `LibKind` uses it; `default_chain_is_the_standard_records_chain` — depends on 1 — `cargo test -p liquers-lib --lib environment`, `cargo test -p liquers-records --all-features --lib --tests`. Rollback: revert both files.
- [ ] 3. `liquers-records/Cargo.toml` feature/bin, `src/bin/liquers_validate_records.rs`, `tests/validate_records_cli.rs` — depends on 2 — `cargo test -p liquers-records --features cli --test validate_records_cli`; `bash scripts/check-build-matrix.sh`. Rollback: delete the bin, test and feature.
- [ ] 4. Documents (skill, `RECORD_STREAMS.md`, `CLAUDE.md` line; History, `reviewed:`); close the source issue — `python3 scripts/docs_index.py --check`.

### Validation

`cargo test -p liquers-records --all-features --lib --tests`; `cargo test -p liquers-lib --lib --tests`;
`bash scripts/check-build-matrix.sh`; `python3 scripts/docs_index.py --check`; diff review: no
change to core's `liquers-validate`, no new dependency outside `liquers-records`.
