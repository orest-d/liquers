---
id: DEFAULT-ASSET-MANAGER-RECIPE-OPT-PAYLOAD-CHECK
kind: design
title: DefaultAssetManager rejects a payload-requiring keyed recipe in recipe_opt
form: compact
status: in_review
phase: implementation
readiness: ready
autofix: eligible
area: [core/assets]
issues: [DEFAULT-ASSET-MANAGER-RECIPE-OPT-SKIPS-PAYLOAD-CHECK]
created: 2026-10-12
---
# DefaultAssetManager rejects a payload-requiring keyed recipe in recipe_opt

Bulk design (`specs/guides/autonomous_bulk_design.md`). Phases 1-4 are written but not approved, and
nothing is implemented.

## Phase 1: High-Level Design

### Purpose

`AssetManager::recipe_opt` (trait default, `liquers-core/src/assets.rs`) rejects a keyed recipe
whose plan requires an evaluation payload, through `Recipe::to_plan_for_key`, and documents itself
as the funnel where that rejection "precedes asset creation and volatility resolution".
`DefaultAssetManager` overrides `recipe_opt` and `is_volatile` with copies that skip the check, so
the native manager builds an asset (and reads `stored`/`cached` in `get_resource_asset`) from a
recipe it refuses later in `AssetRef::evaluate`. Make both managers reject at the same point,
without making key *mutations* fail on a recipe they only need to know exists.

### Problem Example

Recipe `needs_payload/dash.txt` at key `dash.txt`, where `needs_payload` is registered with
`payload: required` (the fixture of `liquers-core/tests/payload_inheritance.rs`
`test_keyed_recipe_requiring_payload_is_rejected`). Measured at HEAD with a throwaway probe:

| Call | `DefaultAssetManager` (`SimpleEnvironmentWithPayload`) | `ImmediateAssetManager` (`ImmediateEnvironmentWithPayload`) |
|---|---|---|
| `env.get_asset_manager().recipe_opt(&key)` | `Ok(Some(recipe))` | `Err(… keyed recipes cannot receive one …)` |
| `env.get_asset_manager().is_volatile(&key)` | `Ok(true)` | `Err(…)` |
| `envref.evaluate("-R/dash.txt")` | `Ok(asset)`; the error appears only in `asset.get()` | `Err(…)`, no asset |
| `envref.evaluate("-R-recipe/dash.txt")` | `Err(…)` | `Err(…)` |
| `manager.set_binary(&key, b"hello", md)` | `Ok(())`, status `Override` | `Err(…)` |
| `manager.remove(&key)` | `Ok(())` | `Err(…)` |

Should be: the first three rows are `Err` on both managers, before any asset is constructed; the
last two rows are `Ok` on both managers (decided, Q1).

### Scope and Acceptance Criteria

- **AC-1** `recipe_opt` rejects on both managers
  - WHEN `recipe_opt(&key)` is called on either manager for a key whose recipe requires a payload
  - THEN it returns `Err` whose message contains "keyed recipes cannot receive one"
- **AC-2** `is_volatile` rejects on both managers
  - WHEN `is_volatile(&key)` is called on either manager for that key
  - THEN it returns the same `Err`, not `Ok(true)`
- **AC-3** Rejection precedes asset creation
  - WHEN `envref.evaluate("-R/dash.txt")` is called on either manager
  - THEN it returns `Err` with that message, and no asset for `dash.txt` is registered afterwards
    (`manager.lookup_key_asset(&key)` is `None`)
- **AC-4** Valid keyed recipes are unaffected
  - WHEN `recipe_opt` and `evaluate` run on either manager for a keyed recipe that needs no payload
  - THEN they return the recipe and the evaluated value as before
- **AC-5** Key mutations only ask whether a recipe exists (decided, Q1)
  - WHEN `set_binary`, `set_state` or `remove` is called on either manager for that key
  - THEN it succeeds; a written value gets status `Override`, because a recipe exists

Non-goals: the recipes HTTP API and recipe listing (`liquers-axum/src/recipes/handlers.rs` and the
`listdir*` union read the recipe provider directly and are unaffected); validating recipes when
`recipes.yaml` is written; `tests/common/minimal_manager.rs`, which keeps the trait defaults.

### Design Readiness

- **Readiness:** ready
- **Automatic fixing:** eligible — `M`, a bug fix restoring the documented `recipe_opt` contract in
  `liquers-core/src/assets.rs` alone; it removes two trait-method overrides (not `pub` items) and
  uses the existing `AsyncRecipeProvider::can_make`, so no interface changes.
- **Leading issue:** None
- **Explanation:** The fix is fully specified and the rejection site is already the documented
  contract; the one observable choice (Q1) was decided by the maintainer on 2026-10-12.
- **Open questions:** None.
- **Decided 2026-10-12 (maintainer):**
  1. **Q1 - mutations under an invalid keyed recipe: not refused.** A keyed recipe may never
     require a payload, but overriding and deleting are independent of that: for now any recipe
     can be overridden and any override deleted. `set_binary`, `set_state`, the trait-default
     `remove`/`expire` and `decide_external_change` therefore ask only whether a recipe exists,
     through the provider's `can_make` (AC-5), on both managers.

### Design Dependencies

- `evaluate-path-consolidation` — overlaps (implemented, `gh_pr: [61]`): its error table lists
  "`AssetManager::recipe_opt`, recipe resolution in `evaluate`" as the rejection sites; this design
  makes the first true for `DefaultAssetManager`. No ordering constraint.
- `record-streams` — overlaps (complete): Phase 4 Step 1.2 made both managers' `get_resource_asset`
  resolve `recipe_opt` once, assuming the two agree. No ordering constraint.
- `keyed-recipe-ownership`, `payload-nested-evaluation-inheritance` — overlaps (complete): the
  volatile-keyed self-delegation fix that re-enabled the payload test, and the payload boundary.
- `error-with-key-field` — overlaps (`phase2-blocked`): it may change how the `to_plan_for_key`
  error carries its key; tests here match on the message text only. Weak overlap; no merge.
- `asset-manager-insert-key-asset-semantics` — overlaps: edits `set_binary`/`set_state`/`remove`
  for locking (already at HEAD as `key_mutation_lock`); step 2 here changes one expression in each.
  T3 is close but the edits are independent; recorded, not merged.

### Consolidated Findings

- **Why the override exists:** no deliberate reason found. Git history is shallow here (every line
  dates from boundary commit `f2da44d`), but `specs/design/async-wasm-refactor/DESIGN.md` (M-B) lists
  `recipe_opt` and `is_volatile` among the store-delegating methods "to hoist into trait default
  methods … to avoid duplication": the defaults were hoisted from `DefaultAssetManager`, whose
  originals stayed, and the payload check was later added to the default only. The
  `is_volatile` override is byte-identical in effect to the default. Deleting both is safe.
- **Callers of `AssetManager::recipe_opt` / `is_volatile`:** evaluation-side —
  `get_resource_asset` (both managers), `interpreter.rs` `Step::GetAssetRecipe` and the
  `IsVolatile` arms for `GetAsset*`; existence-only — `decide_external_change`, trait-default
  `remove` and `expire`, both managers' `set_binary`/`set_state`. Only the second group needs the
  unchecked answer (Q1, decided: mutations are never refused for this reason).
- **Observable change on the native manager:** `EnvRef::evaluate("-R/dash.txt")` returns `Err`
  instead of an asset in `Error`. In `liquers-axum/src/query/handlers.rs` that changes only the
  response message prefix ("Query evaluation failed" instead of "Query execution failed"); the
  request already failed. This is the documented behaviour, not a decision.
- **Existing test:** `test_keyed_recipe_requiring_payload_is_rejected` runs on `QueuedEnv` only and
  uses `envref.evaluate("-R/dash.txt").await?`, so it fails after the fix and must be rewritten.

## Phase 2: Architecture

### Solution

1. Delete `DefaultAssetManager`'s `recipe_opt` and `is_volatile` from its
   `impl AssetManager<E> for DefaultAssetManager<E>` block; it falls back to the trait defaults, as
   `ImmediateAssetManager` and `MinimalInlineAssetManager` do.
2. Replace the existence-only `self.recipe_opt(key).await?.is_some()` (and
   `manager.recipe_opt(key)`) with `self.get_recipe_provider().can_make(key, self.get_envref())`
   in the seven call sites listed below (Q1, decided).

Rejected: adding `to_plan_for_key` to the override (keeps two copies of one contract, the cause of
this issue); a new unchecked `AssetManager::has_recipe` method (a new `pub` trait item, when
`can_make` already answers the question); leaving the mutation callers on `recipe_opt` (Q1's
alternative, regresses the native manager).

Known-issue preflight: `UNCACHED-STORED-COPY-EXPIRY-RACES-AN-INFLIGHT-EVALUATION` (draft) touches
`expire` timing, not its recipe test; no open issue blocks this. Namespaces: none.

### Changes

`liquers-core/src/assets.rs` only:

| Site | Change |
|---|---|
| `impl AssetManager<E> for DefaultAssetManager<E>`: `recipe_opt`, `is_volatile` | delete |
| `decide_external_change` | `manager.recipe_opt(key)` → `manager.get_recipe_provider().can_make(key, manager.get_envref())` |
| trait `AssetManager::remove` default (`has_recipe`) | `can_make` |
| trait `AssetManager::expire` default (recipe test before `status_conflict`) | `can_make` |
| `DefaultAssetManager::set_binary`, `set_state` (`written_status_with_recipe`) | `can_make` |
| `ImmediateAssetManager::set_binary`, `set_state` (`written_status_with_recipe`) | `can_make` |
| trait `AssetManager::recipe_opt` doc | add: existence-only callers use `AsyncRecipeProvider::can_make` |

No signature changes; async unchanged; errors still come from `Recipe::to_plan_for_key` (no new
constructor). `can_make` on `RecipeProviderChain` answers "any provider's `recipe_opt` is `Some`",
the same as the provider call the override made. Documents: `specs/reference/PAYLOAD_GUIDE.md`
"Keys are a payload boundary" gains "rejected when the key is resolved, before an asset exists, by
every asset manager" (History row, `reviewed:` bump).

### Risks

| Area | Risk | Mitigation | Certainty |
|---|---|---|---|
| Existing tests | `test_keyed_recipe_requiring_payload_is_rejected` fails (`evaluate` now `Err`) | rewritten in step 3 | high |
| Other suites | a test relying on `Ok(asset)` for an invalid keyed recipe | none found by grep; full `liquers-core` run | high |
| Compatibility | native `evaluate` errors earlier; axum message prefix changes | documented behaviour; noted | high |
| Behaviour (Q1) | immediate-manager mutations start succeeding on an invalid keyed recipe | AC-5 test; decided 2026-10-12 | high |
| Performance | `can_make` is the same provider lookup as before, minus plan building | none needed | high |
| Recovery | revert the one commit | — | high |

## Phase 3: Examples and Tests

### Examples

The Problem Example table: after the fix rows 1-3 are `Err` and rows 5-6 are `Ok` on both managers.
Queries `-R/dash.txt` and `-R-recipe/dash.txt` were checked with `liquers-validate` (encoded
unchanged; `GetResource` / `GetAssetRecipe` of `dash.txt`).

### Tests

All in `liquers-core/tests/payload_inheritance.rs`, which already has `QueuedEnv` and `InlineEnv`.
Each new test is written once as a `macro_rules!` body (it needs `type CommandEnvironment` for
`register_command!`) and instantiated for both environments as `…_default` and `…_immediate`:

- `keyed_payload_recipe_rejected_by_recipe_opt_{default,immediate}` — AC-1, AC-2
- `keyed_payload_recipe_rejected_before_asset_creation_{default,immediate}` — AC-3
  (`evaluate("-R/dash.txt")` is `Err` with the message; `lookup_key_asset(&key)` is `None`)
- `keyed_payload_recipe_allows_mutations_{default,immediate}` — AC-5 (`set_binary` then
  `get_metadata` status is `Override`; then `remove` is `Ok`)
- `test_keyed_recipe_requiring_payload_is_rejected` (existing, changed): its final block expects
  `evaluate` to return `Err` instead of an asset — AC-3
- `test_same_command_works_when_evaluated_directly` and `manager_parametric.rs` `keyed_eval_*`
  (existing, unchanged) — AC-4

Run: `CARGO_INCREMENTAL=0 cargo test -p liquers-core --test payload_inheritance --test manager_parametric`.

## Phase 4: Implementation Plan

### Steps

- [ ] 1. `liquers-core/tests/payload_inheritance.rs` — add the three test pairs; confirm the
  `_default` ones fail and the AC-5 `_immediate` one fails — `cargo test -p liquers-core --test payload_inheritance`. Rollback: none, tests only.
- [ ] 2. `liquers-core/src/assets.rs` — switch the seven existence-only call sites to
  `get_recipe_provider().can_make(...)` (Phase 2 table) — AC-5 pair passes. Rollback: revert the
  hunk; it is independent of step 3.
- [ ] 3. `liquers-core/src/assets.rs` `impl AssetManager<E> for DefaultAssetManager<E>` — delete
  `recipe_opt` and `is_volatile`; extend the trait `recipe_opt` doc; update
  `test_keyed_recipe_requiring_payload_is_rejected` — AC-1..AC-3 pairs pass —
  `cargo test -p liquers-core --test payload_inheritance`. Rollback: restore the override (then
  the `_default` tests fail, which documents the gap).
- [ ] 4. `specs/reference/PAYLOAD_GUIDE.md` — the boundary sentence, History row, `reviewed:` —
  `python3 scripts/docs_index.py --check`
- [ ] 5. Close `DEFAULT-ASSET-MANAGER-RECIPE-OPT-SKIPS-PAYLOAD-CHECK` with a resolution note; set
  this design's lifecycle — `python3 scripts/docs_index.py && python3 scripts/docs_index.py --check`

### Validation

`CARGO_INCREMENTAL=0 cargo test -p liquers-core --lib --tests`, then
`cargo test -p liquers-lib --lib --tests` and `cargo test -p liquers-axum` (the axum query and
assets suites use `DefaultAssetManager`). No `cfg(feature)` change, so no build matrix. Final diff
review: only `assets.rs`, the test file, `PAYLOAD_GUIDE.md` and specs records.
