---
id: UNNAMED-RECIPE-DIAGNOSTICS
kind: design
title: Report recipes.yaml entries that the default provider cannot name, instead of dropping them silently
form: compact
status: in_review
phase: implementation
readiness: needs-decision
autofix: not-eligible
area: [core/assets, core/validate]
issues: [RECIPE-WITHOUT-FILENAME-SILENTLY-OMITTED-FROM-LISTING]
created: 2026-10-26
---
# Report recipes.yaml entries that the default provider cannot name

Bulk design (`specs/guides/autonomous_bulk_design.md`), 2026-10-26: Phases 1-4 written, not
approved, nothing implemented.

## Phase 1: High-Level Design

### Purpose

`DefaultRecipeProvider` names each recipe by its query's filename. An entry whose query does not
parse, or parses without a filename, is skipped everywhere with no trace, so its author sees a
missing asset and nothing pointing at the entry. The fix must keep the other entries working (the
`Recipe::query` doc: "an invalid query does not invalidate other valid recipes") and tell the
author which entry was dropped and why.

### Problem Example

Verified at HEAD (`cd6d5da`) with the prebuilt validator. `data/recipes.yaml`:

```yaml
recipes:
  - query: -R/data/sales.csv/-/to_text/sales.txt     # served as data/sales.txt
  - query: -R/data/sales.csv/-/to_text               # no filename
  - query: to_text-~X~/broken.txt                    # does not parse
```

- Runtime: `cached_recipes`, `assets_with_recipes` and `RecipeList::get` all use
  `if let Ok(Some(filename)) = recipe.filename()`; entries 1 and 2 vanish. `-R/data/broken.txt`
  fails with `AssetNotFound`, and no message mentions `recipes[2]` or its parse error.
- Authoring: `liquers-validate --recipes data/recipes.yaml --cwd data` reports `recipes[1]` as
  `Ok`, `recipe_check: AdHoc` — although inside a folder's `recipes.yaml` that entry can never be
  served. (`recipes[2]` is already an `Error`: "Can't parse query … Expected ~E here to close ~X~".)

Should be: the validator, run in `recipes.yaml` mode (`--cwd`), reports `recipes[1]` as an `Error`
saying it has no filename and so no name in its folder; and a failed lookup in `data` that goes
through `DefaultRecipeProvider::recipe` names the skipped entries and why.

### Scope and Acceptance Criteria

- **AC-1** The validator flags an unnamed entry in folder mode
  WHEN `liquers-validate --recipes FILE --cwd KEY` validates an entry whose query parses but has no filename
  THEN that result is `Error` with a message saying the default provider cannot list or serve it, and the exit code is 1
- **AC-2** A generic recipe list is unchanged
  WHEN `--recipes FILE` runs without `--cwd`
  THEN an entry without a filename is still `Ok` with `recipe_check: AdHoc`
- **AC-3** A failed lookup names the skipped entries
  WHEN `DefaultRecipeProvider::recipe` or `recipe_plan` finds no recipe for a key whose directory's `recipes.yaml` has entries without a usable filename
  THEN the not-found error lists each as `recipes[<index>]` with its query and the reason (the parse error, or "no filename")
- **AC-4** Valid entries keep working
  WHEN the same file also holds valid entries
  THEN they are listed by `assets_with_recipes` and resolved by `recipe_opt` exactly as today, and a not-found error in a directory with no skipped entries keeps today's text

Non-goals (this slice): rejecting the file; making `AssetNotFound` from the asset manager, or a
directory listing, carry the diagnostic (needs a provider hook — question 2, option b); duplicate
filenames ("first wins", documented).

### Design Readiness

- **Readiness:** needs-decision
- **Automatic fixing:** not-eligible — readiness is not `ready` (rule 5). Under the recommended
  answers the slice is a bug/tooling fix in `liquers-core` only (a private field on the private
  `CachedRecipes`, error text, a private function in the `liquers-validate` bin), and becomes
  eligible once the decisions are made.
- **Leading issue:** question 2 (channel).
- **Explanation:** the slice is fully specified and changes no `pub` item, but it reaches the
  author at validation time and on direct provider lookups only; whether that is enough, or the
  asset layer needs a diagnostic channel, is a design choice.
- **Open questions:**
  1. **Open design - reject or report:** reject the whole file (like an explicit `cwd`) or keep the
     valid entries and report the others. **Recommended: keep and report** — the `Recipe::query`
     doc makes "one bad entry does not break the folder" an explicit intent, and a half-written
     recipe is a normal editing state.
  2. **Open design - channel:** (a) **recommended for this slice**: the validator in folder mode
     plus the provider's own not-found errors (no API change); (b) additionally a defaulted
     `AsyncRecipeProvider::directory_diagnostics(dir)` that `RecipeProviderChain` unions and the
     asset manager appends to `AssetNotFound` for keys in that directory — reaches every caller
     (HTTP, Python, `liquers-lib`'s chain, whose own not-found error hides the default provider's
     text), but adds a trait method and touches every `AssetNotFound` site; a separate `M`/`L`
     feature if wanted.
  3. **Proposed resolution - severity in the validator:** `Error`, as for the explicit-`cwd`
     conflict in the same mode. A `Warning` would need a new `WarningSource` variant (a report
     format change) for an entry that is never served.

### Design Dependencies

- `recipes-yaml-read-errors-propagate`, `recipe-list-set-cwd-validation` — **overlaps**: other
  lines of `DefaultRecipeProvider::cached_recipes`. No order.
- `validate-through-recipe-providers` — **overlaps**: its provider-driven validation iterates
  `assets_with_recipes`, so it cannot see unnamed entries either; AC-1 covers the file mode it
  leaves in place. No order.
- `recipe-folder-names-listing-contract` — **overlaps** (listing contract wording only); no order.
- `query-validation` (complete) — owns the validator; E4, not extended.

### Consolidated Findings

- `RecipeProviderChain::recipe` answers a miss with its own `no_recipe_error` after `recipe_opt`
  returned `None` everywhere, so in a `liquers-lib` environment (a chain whenever `records` is on)
  AC-3's text is not seen; it reaches core environments using `DefaultRecipeProvider` directly and
  callers of `recipe` / `recipe_plan` / `get_asset_info` on it. Stated plainly so the maintainer can
  weigh option 2(b).
- `recipe_plan` reads `get_recipes` (a cloned `RecipeList`) and `RecipeList::get`, not the cached
  index; switching it to `cached_recipes` like `recipe` gives it the diagnostics and saves a clone.
- The validator's bin has no tests today; a `#[cfg(test)] mod tests` in the bin (run with
  `--bin liquers-validate`) tests its private recipe-mode function without adding a `pub` item.

## Phase 2: Architecture

### Solution

Under the recommended answers (1: keep and report; 2a; 3: `Error`):

1. `liquers-core/src/recipes.rs` — `CachedRecipes` (private) gains
   `unnamed: Vec<String>`, one pre-formatted part per skipped entry:
   `recipes[{index}] (query '{query}'): {reason}`, reason = the `filename()` error's message, or
   `the query has no filename`. Filled in the existing `by_name` loop of `cached_recipes`, which
   becomes an explicit `match recipe.filename() { Ok(Some(f)) => …, Ok(None) => …, Err(e) => … }`.
   The empty-list value has `unnamed: Vec::new()`.
2. Private `fn not_found(key: &Key, what: &str, cached: &CachedRecipes) -> Error` building today's
   `general_error("No recipe found for key {key}{what}")` and, when `cached.unnamed` is non-empty,
   appending `"; recipes.yaml in '{dir}' has entries without a usable filename: {parts}"`, keyed
   with `with_key(key)`. Used by `DefaultRecipeProvider::recipe` and `recipe_plan`; `recipe_plan`
   switches from `get_recipes` + `RecipeList::get` to `cached_recipes` + `CachedRecipes::get` (same
   first-wins answer).
3. `liquers-core/src/bin/liquers_validate.rs` `validate_recipe_input`: when `cwd` is given, for
   each result whose recipe parsed and has `filename() == Ok(None)` and is not already an `Error`,
   set `status = Error` and `error = Some(Error::general_error("recipe has no filename, so
   DefaultRecipeProvider cannot list or serve it from a recipes.yaml".into()))` via a private
   `fn unnamed_in_folder_error() -> Error`. Unparsable entries are already `Error` (unchanged).

`assets_with_recipes` and `RecipeList::get` keep skipping (AC-4); their doc comments say the
skipped entries are reported by the validator and in not-found errors.

Rejected: rejecting the file (Phase 1 question 1); `eprintln!` from the provider (does not reach
web or Python callers — the reason `RECIPE-CWD-ERROR-DOES-NOT-NAME-THE-RECIPE` removes one);
a new `pub` validator function (rule 4, and the check is specific to the CLI's folder mode).

### Changes

- `liquers-core/src/recipes.rs`: `CachedRecipes` (private field), `cached_recipes`,
  `DefaultRecipeProvider::recipe`, `recipe_plan`, a private `not_found` helper, doc comments of
  `assets_with_recipes` and `RecipeList::get`.
- `liquers-core/src/bin/liquers_validate.rs`: `validate_recipe_input`, a private helper, a new
  `#[cfg(test)] mod tests`.
- No `pub` signature, trait, command or format change. Error types unchanged (`General` for not
  found, as today).
- Documents: `specs/reference/RECIPES_AND_PLANS.md` §Provider contract ("Recipes without a valid
  filename are omitted from directory listing" gains where they are reported), History row,
  `reviewed:`; `.claude/skills/liquers-validate/SKILL.md` (folder mode flags unnamed entries).

### Risks

| Risk | Mitigation | Certainty |
|---|---|---|
| A test matches the exact not-found text | the text is unchanged when nothing was skipped (AC-4); grep "No recipe found for key" at step 1 | high |
| Users of `--recipes --cwd` with intentionally ad-hoc entries now get exit code 1 | such an entry is never served from a folder; without `--cwd` nothing changes (AC-2) | medium |
| Runtime discovery stays partial under a chain | stated in Consolidated Findings; option 2(b) | high |

Recovery: revert; the field and helper are private.

## Phase 3: Examples and Tests

### Examples

The Problem Example. Validator, after the fix:
`liquers-validate --recipes data/recipes.yaml --cwd data` → `recipes[0]` `Ok` (`Stored`, key
`data/sales.txt`), `recipes[1]` `Error` (no filename), `recipes[2]` `Error` (parse), exit 1.
Without `--cwd`: `recipes[1]` `Ok` / `AdHoc` as today. Provider (core `SimpleEnvironment`, memory
store): `DefaultRecipeProvider::recipe(data/broken.txt)` → `Err` whose message contains
`recipes[1]`, `no filename`, `recipes[2]` and `Expected ~E`.

### Tests

- `liquers-core/src/recipes.rs` `mod recipe_cache_tests` (uses `env()` / `write_recipes`):
  - `not_found_names_entries_without_a_filename` — the Problem Example file; `recipe` and
    `recipe_plan` of `data/broken.txt` fail with both parts. Fails at HEAD. Covers AC-3.
  - `valid_entries_still_served_beside_unnamed_ones` — `assets_with_recipes(data)` is
    `[sales.txt]`; `recipe_opt(data/sales.txt)` is `Some`. Covers AC-4.
  - `not_found_text_unchanged_without_skipped_entries` — a clean file; message equals
    `"No recipe found for key data/missing.txt"`. Covers AC-4.
- `liquers-core/src/bin/liquers_validate.rs` new `#[cfg(test)] mod tests` (writes the file to a
  temp dir; registry with `to_text`):
  - `folder_mode_flags_a_recipe_without_filename` — `validate_recipe_input(path, Some("data"), Plan, &cmr)`:
    `results[1].status == Error`, message contains "no filename". Covers AC-1.
  - `list_mode_keeps_a_recipe_without_filename_adhoc` — `cwd = None`: `Ok`, `AdHoc`. Covers AC-2.

Run: `cargo test -p liquers-core --lib recipes`;
`cargo test -p liquers-core --features cli --bin liquers-validate`.

## Phase 4: Implementation Plan

### Steps

- [ ] 1. Tests above in `recipes.rs` and the bin; the AC-1 and AC-3 tests fail at HEAD; `grep -rn "No recipe found for key" --include=*.rs .` reviewed for exact-text assertions — the two run commands above. Rollback: delete the tests.
- [ ] 2. `recipes.rs` `CachedRecipes::unnamed`, `cached_recipes` loop, `not_found`, `recipe`, `recipe_plan`, doc comments — depends on 1 — `cargo test -p liquers-core --lib --tests`. Rollback: revert the file.
- [ ] 3. `bin/liquers_validate.rs` `validate_recipe_input` and `unnamed_in_folder_error` — depends on 1 — `cargo test -p liquers-core --features cli --bin liquers-validate`, then `./target/debug/liquers-validate --recipes <Problem Example file> --cwd data` exits 1. Rollback: revert the bin.
- [ ] 4. `specs/reference/RECIPES_AND_PLANS.md` (History, `reviewed:`), `.claude/skills/liquers-validate/SKILL.md`; close the source issue (or leave it open naming option 2(b) if the maintainer wants it) — `python3 scripts/docs_index.py --check`.

### Validation

`cargo test -p liquers-core --lib --tests`; `cargo test -p liquers-core --features cli --lib --tests --bins`;
`cargo test -p liquers-lib --lib --tests`; `python3 scripts/docs_index.py --check`; diff review:
no `pub` item added, only the two source files, the reference, the skill and the issue.
