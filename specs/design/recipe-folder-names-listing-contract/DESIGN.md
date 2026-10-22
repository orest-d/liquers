---
id: RECIPE-FOLDER-NAMES-LISTING-CONTRACT
kind: design
title: The listing contract a recipe provider owes the parent-listing refresh
form: compact
status: in_review
phase: implementation
readiness: needs-decision
autofix: not-eligible
area: [core/assets, docs]
issues: [RECIPE-ONLY-FOLDER-NAMES-HIDE-A-CREATED-FOLDER-FROM-ITS-PARENT]
created: 2026-10-20
---
# The listing contract a recipe provider owes the parent-listing refresh

Bulk design (`specs/guides/autonomous_bulk_design.md`): Phases 1-4 written, not approved, nothing
implemented.

## Phase 1: High-Level Design

### Purpose

`AssetManager::refresh_listing_version` decides whether a write can have changed the parent's
membership from the *merged* listing (store plus `assets_with_recipes`). That is sound only while a
provider lists names in a folder exactly when a file stored in that folder defines them. Either
write that rule down where provider authors read it, or stop depending on it.

### Problem Example

Persists at HEAD (18732dd), by code reading (`liquers-core/src/assets.rs`
`refresh_listing_version`, `refresh_one_listing`; its doc comment names this issue). A custom
provider generates `reports/daily.csv` whenever asked about folder `reports`, though nothing is
stored there. A catalog depends on the listing of the root (`-R-dir/`). The first write of
`reports/notes.txt` physically creates `reports`: the merged listing of `reports` was
`[daily.csv]` before and `[daily.csv, notes.txt]` after, never empty, so the climb stops and the
root's registered listing — which now gains `reports` — is not refreshed. The catalog stays
`Ready` and misses the folder until another change to the root. With the in-tree providers
(`DefaultRecipeProvider`, `ManifestRecipeProvider`, a chain of them) the case cannot arise: they list
names only from `recipes.yaml` / `*.manifest.yaml` stored in that folder
(`removing_a_recipe_folder_expires_the_parent_index` pins the in-tree behaviour).

Should be (recommended, Q1): `AsyncRecipeProvider::assets_with_recipes` states the rule, the
language-integration guide repeats it next to "listing and containment need not agree", and a test
pins that the in-tree providers obey it.

### Scope and Acceptance Criteria

- **AC-1** The contract is written
  WHEN a provider author reads `AsyncRecipeProvider::assets_with_recipes` or the guide's RECIPE section
  THEN it says names listed in a folder must be backed by something stored in that folder, that a generative provider makes such names addressable (`recipe_opt`, `can_make`) rather than listed, and what a listing without a stored file breaks
- **AC-2** In-tree providers obey it
  WHEN `DefaultRecipeProvider` or a `RecipeProviderChain` is asked for `assets_with_recipes` of a folder with no stored file
  THEN it answers an empty list
- **AC-3** The documented refresh rule matches
  WHEN `DEPENDENCIES_STATUS.md`'s folder-listing bullet is read
  THEN it names the provider contract the climb relies on

Non-goals: changing the climb (option B below); `ManifestRecipeProvider`'s own test (it lists
explicit chunks only, from a stored manifest — `RECORD_STREAMS.md` §provider table — and already
has coverage in `liquers-records`).

### Design Readiness

- **Readiness:** needs-decision
- **Automatic fixing:** not-eligible — rule 5 (a contract choice is open). Once Q1 is answered A it
  becomes a documentation fix plus one test in `liquers-core`, which is eligible.
- **Leading issue:** Q1.
- **Explanation:** both options are small and fully planned; the choice is whether provider authors
  carry a rule or every write under a watched folder pays a store listing.
- **Open questions:**
  1. **Open design - contract or code:** **A** (recommended): state the contract; zero runtime cost,
     true of every in-tree provider, and consistent with the guide's advice that a generative
     provider be addressable rather than listed. **B**: decide the climb from the *stored* listing —
     in `refresh_one_listing`, when the merged listing was and is non-empty, also climb if
     `store.listdir(level)` now holds at most the written name or nothing; one extra store listing
     per write into a registered folder, and correct for any provider.

### Design Dependencies

- `root-listing-dependency-key-spelling` — **overlaps** (option B only): it edits
  `refresh_listing_version` too; under B, land after it. Under A, no interaction.
- `new-folder-write-ancestor-listing-refresh` (complete, E4) — origin; not extended.

### Consolidated Findings

- Under A nothing in `assets.rs` changes; the doc comment of `refresh_listing_version` already
  describes the assumption and keeps its issue reference until the issue closes.
- B's cost lands on every write into a folder whose listing is registered (catalogs, indexes); on
  an OpenDAL object store that is a network listing per write.

## Phase 2: Architecture

### Solution

Option A. Text added to the doc comment of `AsyncRecipeProvider::assets_with_recipes`
(`liquers-core/src/recipes.rs`):

> Names are listed in `key` only when something stored in `key` defines them (a `recipes.yaml`, a
> manifest). The asset manager relies on it: a write's listing refresh decides whether a folder was
> created or emptied from the merged listing. A provider that synthesizes names for a folder with
> no stored file must make them addressable (`recipe_opt`, `can_make`), not listed.

The same rule, one paragraph, in `specs/guides/LANGUAGE-INTEGRATION_GUIDE.md` §RECIPE ("Listing and
containment need not agree") and one clause in `specs/reference/DEPENDENCIES_STATUS.md`'s
folder-listing bullet. The `refresh_listing_version` doc comment drops the issue ID and cites the
trait doc.

Rejected: B (see Q1; kept as the alternative); a runtime check comparing merged and stored
emptiness (costs what B costs and only warns).

Known-issue preflight: none blocking. `PY-RECIPE-PROVIDER-NOT-CONFIGURABLE` — when Python providers
arrive, their protocol must carry this rule (the guide change covers it).

### Changes

`liquers-core/src/recipes.rs` (doc comment only), `liquers-core/src/assets.rs` (doc comment of
`refresh_listing_version`), a test; documents above with History rows and `reviewed:`. No signature,
command or format change.

### Risks

| Risk | Mitigation | Certainty |
|---|---|---|
| An out-of-tree provider already lists names without a stored file | It is broken today in the same way; the doc now says so and names the fix | high |
| A later in-tree provider breaks the rule | AC-2's test covers the default and the chain; a new provider adds its own | medium |

## Phase 3: Examples and Tests

### Examples

A conforming generative provider: `recipe_opt(reports/daily.csv)` answers a recipe,
`can_make` is true, `assets_with_recipes(reports)` is empty until `reports` holds a stored file.

### Tests

- `liquers-core/src/recipes.rs` `mod tests`:
  `default_provider_lists_no_names_in_a_folder_without_a_stored_file` — memory store with
  `other/recipes.yaml` only; `assets_with_recipes(reports)` is empty for `DefaultRecipeProvider` and
  for a `RecipeProviderChain` of it plus a second `DefaultRecipeProvider`: AC-2
- Documentation review (AC-1, AC-3): the three texts exist and agree, checked in the final diff
  review; `python3 scripts/docs_index.py --check` passes.
- Existing: `removing_a_recipe_folder_expires_the_parent_index` still passes: AC-2.

Run: `cargo test -p liquers-core --lib recipes::tests::default_provider_lists_no_names_in_a_folder_without_a_stored_file`
and `cargo test -p liquers-core --test dependency_audit_integration`.

## Phase 4: Implementation Plan

### Steps

- [ ] 1. `liquers-core/src/recipes.rs` — trait doc on `assets_with_recipes`; test of Phase 3 — `cargo test -p liquers-core --lib recipes::tests::`. Rollback: revert.
- [ ] 2. `liquers-core/src/assets.rs` — `refresh_listing_version` doc comment cites the trait doc — `cargo doc -p liquers-core --no-deps`.
- [ ] 3. `LANGUAGE-INTEGRATION_GUIDE.md` §RECIPE paragraph, `DEPENDENCIES_STATUS.md` bullet; History rows, `reviewed:`; close the issue with a resolution note — `python3 scripts/docs_index.py --check`.

### Validation

`cargo test -p liquers-core --lib --tests`; diff review: two doc comments, one test, two documents.
If Q1 is answered B instead, replace steps 1-2 by: `refresh_one_listing` returns the stored
listing's length alongside emptiness, the climb condition adds "stored listing now ≤ 1 entry", a
test with a generating provider in `liquers-core/tests/dependency_audit_integration.rs`
(`creating_a_folder_under_a_generating_provider_refreshes_the_parent`), after
`root-listing-dependency-key-spelling` lands.
