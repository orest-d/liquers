---
id: RECIPES-YAML-READ-ERRORS-PROPAGATE
kind: design
title: A recipes.yaml that exists but cannot be read is an error, not an empty recipe list
form: compact
status: in_review
phase: implementation
readiness: ready
autofix: eligible
area: [core/assets]
issues: [RECIPES-YAML-READ-ERRORS-READ-AS-NO-RECIPES]
created: 2026-10-26
---
# A recipes.yaml that exists but cannot be read is an error, not an empty recipe list

Bulk design (`specs/guides/autonomous_bulk_design.md`), 2026-10-26: Phases 1-4 written; Phase 4
implemented on 2026-10-27 (automatic fix).

## Phase 1: High-Level Design

### Purpose

`DefaultRecipeProvider` must tell "this folder has no `recipes.yaml`" apart from "its
`recipes.yaml` could not be read". Today both read as "no recipes", so a permission or transport
failure surfaces as a missing asset instead of as the failure. The trait already promises the
distinction (`AsyncRecipeProvider` doc: "`recipe_opt` returns `Ok(None)` only for absence, while
I/O and parsing failures may still be errors").

### Problem Example

Verified at HEAD (`cd6d5da`). `DefaultRecipeProvider::cached_recipes`
(`liquers-core/src/recipes.rs`) matches `get_bytes(<dir>/recipes.yaml)` with `Err(_) => empty list`.
A file store whose `data/recipes.yaml` exists with mode `000`:

1. `has_recipes(data)` → `store.contains` → `Ok(true)`;
2. `recipe_opt(data/top.csv)` → `cached_recipes` → `get_bytes` → `Err(KeyReadError, "Permission
   denied")` → mapped to an empty list → `Ok(None)`;
3. the asset manager answers `-R/data/top.csv` with `AssetNotFound` ("the store holds no value for
   it and no recipe produces it").

Should be: step 2 returns the `KeyReadError` naming `data/recipes.yaml`, and the query fails with
it. The same holds for any remote-store transport error.

### Scope and Acceptance Criteria

- **AC-1** An unreadable file is an error
  WHEN `<dir>/recipes.yaml` is contained but `get_bytes` fails with an error other than not-found or not-supported
  THEN `recipe_opt`, `recipe`, `recipe_plan`, `assets_with_recipes` and `get_recipes` for that directory return that error, carrying the `recipes.yaml` key
- **AC-2** A missing file is still no recipes
  WHEN `get_bytes` answers `KeyNotFound` (no file, or it vanished after `has_recipes`)
  THEN the directory has no recipes: `recipe_opt` is `Ok(None)` and `recipe` fails with the existing "No recipe found" error
- **AC-3** An unsupported key is still no recipes
  WHEN `get_bytes` answers `KeyNotSupported` (a store router with no member for the key)
  THEN the directory has no recipes, as `has_recipes` already answers
- **AC-4** A read failure is not cached
  WHEN a read fails and a later read of the same bytes succeeds
  THEN the later lookup returns the parsed recipes

Non-goals: changing `has_recipes`; changing YAML-parse errors (already errors); the other
providers (`ManifestRecipeProvider` has its own read path).

### Design Readiness

- **Readiness:** ready
- **Automatic fixing:** eligible — bug fix restoring the trait's documented contract, in
  `liquers-core/src/recipes.rs` only: one `match` in a private method and its doc comments; no
  `pub` signature, type, command or format change.
- **Leading issue:** None
- **Explanation:** the not-found and not-supported cases are exactly the two `ErrorType`s the store
  contract (`STORE_SEMANTICS.md` §4) and `has_recipes` already use for "nothing there"; everything
  else is a failure the caller should see.
- **Open questions:** None. Resolved in Phase 2: the propagated error keeps the store's own key
  (`<dir>/recipes.yaml`, which names the file) rather than being re-keyed to the directory.

### Design Dependencies

- `recipe-list-set-cwd-validation`, `unnamed-recipe-diagnostics` — **overlaps**: both touch
  `DefaultRecipeProvider::cached_recipes` (the `set_cwd` call; the `by_name` loop), not the
  `get_bytes` match this changes. No order; whichever lands second rebases.
- `validate-through-recipe-providers` — **overlaps**: its Phase 2 notes this gap as a known
  limitation ("a listing error may be invisible"); after this fix a provider-validation run reports
  an unreadable `recipes.yaml` as an `Error` result (its AC-4). No order.
- `dependency-chain-analysis-cost` (complete) — introduced the cache; E4, not extended.

### Consolidated Findings

- `recipe` and `recipe_plan` reach `cached_recipes` without `has_recipes`, so the not-found arm is
  load-bearing for every missing directory, not only for the removal race; it must stay.
- `NoAsyncStore::get` answers `KeyNotFound` (after an absoluteness check), so an environment with
  no store and `DefaultRecipeProvider` (the `liquers-lib` default) still resolves "no recipes".
- A relative key now propagates `KeyNotAbsolute` from `get_bytes`, as it already does from
  `has_recipes`' `contains`: a malformed key is not a miss (`STORE_SEMANTICS.md`).
- `specs/reference/RECIPES_AND_PLANS.md` §Provider contract documents the current behaviour with a
  link to the issue; it changes with the fix.

## Phase 2: Architecture

### Solution

In `DefaultRecipeProvider::cached_recipes`, replace `Err(_) => empty` with:

```rust
let recipes_key = key.join("recipes.yaml");
let bytes = match envref.get_async_store().get_bytes(&recipes_key).await {
    Ok(bytes) => bytes,
    Err(error) if matches!(error.error_type, ErrorType::KeyNotFound | ErrorType::KeyNotSupported) => {
        return Ok(Arc::new(CachedRecipes { bytes: Vec::new(), list: RecipeList::new(), by_name: HashMap::new() }))
    }
    Err(error) if error.key.is_none() => return Err(error.with_key(&recipes_key)),
    Err(error) => return Err(error),
};
```

`ErrorType` is already imported (used by `has_recipes`). The two "nothing there" types are named
explicitly rather than tested by `ErrorCategory::NotFound`, which would also admit
`AssetNotFound` — an asset-layer answer a store never gives. A failure returns before the cache is
touched, so nothing is cached (AC-4), exactly as for a malformed file today.

Rejected: re-keying every error to the directory (loses which file failed); mapping only
`KeyNotFound` (would turn every `-R/` query under a router's uncovered folder into a failure through
`recipe()`, the case `RECIPE-PROVIDERS-FAIL-ON-A-KEY-THE-STORE-DOES-NOT-SUPPORT` fixed for
`has_recipes`).

### Changes

- `liquers-core/src/recipes.rs`: `DefaultRecipeProvider::cached_recipes` (the match and its doc
  comment), `DefaultRecipeProvider::get_recipes` doc comment ("Only a missing file, or a key the
  store does not support, reads as an empty list; any other read failure is an error").
- No `pub` signature change; async unchanged.
- Documents: `specs/reference/RECIPES_AND_PLANS.md` §Provider contract (replace the sentence
  linking the issue), History row, `reviewed:` bump.

### Risks

| Risk | Mitigation | Certainty |
|---|---|---|
| A store answers a miss with something other than `KeyNotFound` and now fails | store conformance requires `KeyNotFound` for `get`/`get_bytes` misses (`STORE_SEMANTICS.md` §4); run `liquers-lib` tests (routers, OpenDAL) | high |
| Tests relying on a failing store reading as "no recipes" | none found (`RefusingStore` answers `KeyNotSupported`, kept by AC-3); run the core and lib suites | high |
| A transient remote error now fails a query that used to return a (wrong) not-found | intended; the error names the file | high |

Recovery: revert the match.

## Phase 3: Examples and Tests

### Examples

The Problem Example, as a unit test: a store whose `contains` answers `Ok(true)` and whose `get`
answers `Error::key_read_error(key, "unreadable", "permission denied")`; `recipe_opt` of
`data/top.csv` is `Err` with `ErrorType::KeyReadError` and key `data/recipes.yaml`.

### Tests

New `#[cfg(test)] mod read_error_tests` in `liquers-core/src/recipes.rs`, beside
`unsupported_key_tests`, with a private `UnreadableStore` (`contains` → `Ok(true)`; `get` →
`KeyReadError`; a switch to serve given bytes, for AC-4):

- `unreadable_recipes_yaml_is_an_error` — `recipe_opt`, `recipe`, `assets_with_recipes`,
  `get_recipes` all `Err(KeyReadError)` with key `data/recipes.yaml`. Fails at HEAD. Covers AC-1.
- `read_failure_is_not_cached` — first lookup fails; switch to serving a one-recipe YAML; second
  `recipe_opt` returns the recipe. Covers AC-4.
- `missing_recipes_yaml_is_no_recipes` — `AsyncMemoryStore` with no file: `recipe_opt` `Ok(None)`,
  `recipe` `Err` whose message contains "No recipe found". Covers AC-2.
- `default_provider_finds_no_recipe_under_an_unsupported_key` (existing) — extend with
  `DefaultRecipeProvider::new().recipe(&key, …)` failing with "No recipe found", not
  `KeyNotSupported` (the direct `cached_recipes` path). Covers AC-3.

Run: `cargo test -p liquers-core --lib recipes`.

## Phase 4: Implementation Plan

### Steps

- [x] 1. `liquers-core/src/recipes.rs` — add `read_error_tests` and extend `unsupported_key_tests`; `unreadable_recipes_yaml_is_an_error` fails at HEAD — `cargo test -p liquers-core --lib recipes`. Rollback: delete the tests.
- [x] 2. `liquers-core/src/recipes.rs` `DefaultRecipeProvider::cached_recipes` match and the two doc comments — depends on 1 — `cargo test -p liquers-core --lib recipes`, then `cargo test -p liquers-core --lib --tests`. Rollback: revert the match.
- [x] 3. `specs/reference/RECIPES_AND_PLANS.md` §Provider contract, History row, `reviewed:`; close `RECIPES-YAML-READ-ERRORS-READ-AS-NO-RECIPES` with a resolution note — `python3 scripts/docs_index.py --check`.

### Validation

`cargo test -p liquers-core --lib --tests`; `cargo test -p liquers-lib --lib --tests` (store routers
and the manifest chain go through this provider); `python3 scripts/docs_index.py --check`; diff
review: only `recipes.rs`, the reference and the issue file change.
