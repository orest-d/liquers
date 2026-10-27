---
id: RECIPE-LIST-SET-CWD-VALIDATION
kind: design
title: RecipeList::set_cwd checks the whole list first and names every recipe that sets cwd
form: compact
status: in_review
phase: implementation
readiness: ready
autofix: eligible
area: [core/assets]
issues: [RECIPE-CWD-ERROR-DOES-NOT-NAME-THE-RECIPE, RECIPE-LIST-SET-CWD-MUTATES-BEFORE-FAILING]
merged: 2026-10-26
created: 2026-10-26
---
# RecipeList::set_cwd checks the whole list first and names every recipe that sets cwd

Bulk design (`specs/guides/autonomous_bulk_design.md`), 2026-10-26: Phases 1-4 written, not
approved, nothing implemented.

Merged at intake: both sources are fixed by rewriting the same ten-line function,
`RecipeList::set_cwd` (overlap test **T3**, same change site; doing them apart forces an order and
a conflict). Both are eligible for automatic fixing, so **E1** does not apply. The issue text
"shares the code but not the problem" records why they were filed apart, not a maintainer
decision to keep them apart (E5 does not apply).

## Phase 1: High-Level Design

### Purpose

A `recipes.yaml` entry that sets `cwd` makes `DefaultRecipeProvider` refuse the whole file. The
author then needs to know which entry; today the error names neither the entry nor its position,
and a separate `eprintln!` line (which a web or Python caller never sees) shows only the cwd value.
The same function also leaves a half-mutated list behind when it fails.

### Problem Example

Verified at HEAD (`cd6d5da`). `data/recipes.yaml`:

```yaml
recipes:
  - query: -R/data/sales.csv/-/to_text/sales.txt
  - query: -R/data/sales.csv/-/to_text/explicit.txt
    cwd: other
```

Evaluating `-R/data/sales.txt`: `cached_recipes` → `RecipeList::set_cwd("data")`
(`liquers-core/src/recipes.rs`) sets `recipes[0].cwd = "data"`, reaches `recipes[1]`, prints
`Recipe already has CWD set to Some("other")` to stderr, and returns `NotSupported` "CWD can't be
explicitly specified in a recipe" with key `data` (added by the provider). The list is left with
`recipes[0]` mutated.

Should be: `NotSupported` "CWD can't be explicitly specified in a recipe: recipes[1] (query
'-R/data/sales.csv/-/to_text/explicit.txt') sets cwd 'other'", key `data`; nothing on stderr; the
list unchanged.

### Scope and Acceptance Criteria

- **AC-1** The error names the offending recipe
  WHEN a directory's `recipes.yaml` has an entry with an explicit `cwd`
  THEN a lookup in that directory fails with `NotSupported`, keyed to the directory, whose message keeps the existing sentence and adds `recipes[<index>]`, the entry's query and its `cwd`
- **AC-2** Every offender is named
  WHEN several entries set `cwd`
  THEN the one error names each of them by index
- **AC-3** A failed call does not mutate
  WHEN `RecipeList::set_cwd` returns `Err`
  THEN the list is equal to what it was before the call
- **AC-4** The success path is unchanged
  WHEN no entry sets `cwd`
  THEN every recipe's `cwd` is set to the given key, as today

Non-goals: allowing an explicit `cwd`; changing `liquers-validate`'s per-recipe report
(`validate::apply_cwd` already reports every offender with its index); the error type.

### Design Readiness

- **Readiness:** ready
- **Automatic fixing:** eligible — bug fix restoring the documented behaviour (the reference says
  the file is refused; nothing documents a partial mutation), inside `liquers-core/src/recipes.rs`:
  the body of `RecipeList::set_cwd` and its doc comment. No signature, type, command or format
  change; the error type stays `NotSupported` and its message keeps its existing sentence as a
  prefix.
- **Leading issue:** None
- **Explanation:** validate first, then assign, is the whole fix; the richer message comes from data
  the function already holds.
- **Open questions:** None. Resolved in Phase 2: indices are 0-based and written `recipes[i]` (the
  YAML path, and the `index` `liquers-validate` reports); the message lists every offender.

### Design Dependencies

- `recipes-yaml-read-errors-propagate`, `unnamed-recipe-diagnostics` — **overlaps**: both edit
  other lines of `DefaultRecipeProvider::cached_recipes`; this design leaves its `set_cwd` call
  unchanged. No order.
- `query-validation` (complete) — owns `validate::apply_cwd` / `cwd_conflict_error`, which mirror
  the sentence kept here; E4, not extended.

### Consolidated Findings

- `set_cwd`'s only caller is `DefaultRecipeProvider::cached_recipes`, which discards the list on
  error and adds the directory key with `with_key`; the error is not cached, so the next lookup
  re-reports it.
- `validate::cwd_conflict_error` mirrors the sentence verbatim; keeping it as the prefix keeps the
  two consistent without editing the validator.
- `specs/reference/RECIPES_AND_PLANS.md` §Working-directory rules documents the stderr line and the
  partial mutation; both sentences change.

## Phase 2: Architecture

### Solution

Rewrite `RecipeList::set_cwd(&mut self, cwd: String) -> Result<(), Error>` (signature unchanged):

1. Collect `(index, &recipe)` for every recipe with `cwd.is_some()`.
2. If any: return `Error::not_supported(format!("CWD can't be explicitly specified in a recipe: {}", parts.join("; ")))`,
   each part `recipes[{index}] (query '{query}') sets cwd '{cwd}'`. No mutation has happened.
3. Otherwise assign `Some(cwd.clone())` to every recipe.

Remove the `eprintln!`. Update the doc comment: "Assigns the working key to every recipe. Fails,
leaving the list unchanged, if any recipe already has one; the error names each such recipe by
index, query and `cwd`." The "does not already have one" wording goes: a recipe that has one is an
error, never skipped.

Rejected: naming only the first offender (an author fixing a long file re-runs once per entry);
using the recipe's title or filename as the name (both may be empty or unparsable; index and raw
query always exist).

### Changes

- `liquers-core/src/recipes.rs`: `RecipeList::set_cwd` body and doc comment.
- `DefaultRecipeProvider::cached_recipes` unchanged (`map_err(|e| e.with_key(key))` already adds the
  directory).
- Documents: `specs/reference/RECIPES_AND_PLANS.md` §Working-directory rules (the error message;
  delete the stderr / partial-mutation paragraph) and §Known limitations (drop the
  `RECIPE-LIST-SET-CWD-MUTATES-BEFORE-FAILING` bullet), History row, `reviewed:`.

### Risks

| Risk | Mitigation | Certainty |
|---|---|---|
| A test or caller matches the exact message | only `validate::cwd_conflict_error` repeats it, and the prefix is kept; grep at step 1 | high |
| A very long message for a file where many entries set `cwd` | one short part per offender; such a file is already wholly refused | high |

Recovery: revert the function.

## Phase 3: Examples and Tests

### Examples

The Problem Example, at provider level: memory store, `data/recipes.yaml` as above,
`recipe_opt(data/sales.txt)` → `Err`, `error_type == NotSupported`, `key == Some("data")`, message
contains `recipes[1]`, `explicit.txt` and `'other'`. (Query checked with `liquers-validate`:
`-R/data/sales.csv/-/to_text/explicit.txt` plans to `GetAsset` + `Action{to_text}` + `Filename`.)

### Tests

In `liquers-core/src/recipes.rs`:

- `mod tests` `set_cwd_failure_leaves_the_list_unchanged` — list of three, the third with `cwd`;
  clone, call, assert `Err` and every recipe's `cwd` equals the clone's. Fails at HEAD. Covers AC-3.
- `mod tests` `set_cwd_error_names_every_offending_recipe` — offenders at 1 and 3; message contains
  `recipes[1]`, `recipes[3]`, both queries and both cwd values, and starts with
  "CWD can't be explicitly specified in a recipe". Covers AC-1, AC-2.
- `mod tests` `set_cwd_sets_every_recipe` — no offender: every `cwd` is `Some(given)`. Covers AC-4.
- `mod recipe_cache_tests` `explicit_cwd_in_recipes_yaml_names_the_recipe` — uses the module's
  `env()` / `write_recipes`; the Problem Example assertions. Covers AC-1.

Run: `cargo test -p liquers-core --lib recipes`.

## Phase 4: Implementation Plan

### Steps

- [ ] 1. `liquers-core/src/recipes.rs` — add the four tests; `set_cwd_failure_leaves_the_list_unchanged` and `explicit_cwd_in_recipes_yaml_names_the_recipe` fail at HEAD; `grep -rn "explicitly specified" --include=*.rs .` lists only `recipes.rs` and `validate/mod.rs` — `cargo test -p liquers-core --lib recipes`. Rollback: delete the tests.
- [ ] 2. `liquers-core/src/recipes.rs` `RecipeList::set_cwd` body and doc comment; remove the `eprintln!` — depends on 1 — `cargo test -p liquers-core --lib --tests`. Rollback: revert the function.
- [ ] 3. `specs/reference/RECIPES_AND_PLANS.md` (two sections, History, `reviewed:`); close both source issues with a resolution note — `python3 scripts/docs_index.py --check`.

### Validation

`cargo test -p liquers-core --lib --tests`; `cargo test -p liquers-core --features cli --test validate_integration`
(the mirrored sentence); `python3 scripts/docs_index.py --check`; diff review: one function, its
tests, the reference, the two issues.
