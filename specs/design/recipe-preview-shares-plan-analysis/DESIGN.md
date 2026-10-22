---
id: RECIPE-PREVIEW-SHARES-PLAN-ANALYSIS
kind: design
title: A recipe preview builds and analyses its plan through the same function as make_plan
form: compact
status: in_review
phase: implementation
readiness: ready
autofix: not-eligible
area: [core/plan]
issues: [RECIPE-PLAN-ANALYSIS-RUNS-OUTSIDE-PLAN-BUILDING]
created: 2026-10-20
---
# A recipe preview builds and analyses its plan through the same function as make_plan

Bulk design (`specs/guides/autonomous_bulk_design.md`), 2026-10-20. Phases 1-4 are written but not
approved, and nothing is implemented.

## Phase 1: High-Level Design

### Purpose

`create_plan_with_init_metadata` (`liquers-core/src/recipes.rs`, used by
`DefaultRecipeProvider::get_asset_info` and `::recipe_plan`) builds a keyed recipe's plan and runs
`let _ = analyze_plan_dependencies(envref, &mut plan, None).await`, under two TODO markers
("misleading name", "should be done in plan building or checking").

Re-checked at HEAD (18732dd). Of the issue's three defects, only the structural one remains:

1. *Discarded result*: mitigated. `analyze_plan_dependencies` sets `plan.error` before it returns
   `Err`, and `get_asset_info` reports `plan.error`, so a failure is visible. The `let _` hides that
   dependency on a side channel.
2. *CWD `None` vs the run's CWD*: does not reproduce. A keyed asset's `Context` starts without a
   CWD (`Context::new`), and a recipe's `cwd` reaches the plan as a `SetCwd` step
   (`Recipe::to_plan`). The run therefore freezes and analyses with `None` too.
3. *Unfrozen analysis*: equivalent by construction. `DependencyWalk` resolves relative operands with
   its `CwdCursor` from the same entry CWD, as `make_plan` does for validation and for query assets.

What remains is a second hand-written build-and-analyse sequence, with a misleading name and no
test that ties the preview's flags to the evaluated asset's.

### Problem Example

`data/r.txt` = `-R/./s.txt/-/upper` with `cwd: data`, and `data/s.txt` with `expires: "in 10 s"`.
`get_asset_info(data/r.txt)` reports `expires: in 10 s`. Evaluating `data/r.txt` stores the same
in its metadata. They agree today, but only because two separately written sequences happen to
match, and nothing checks it. (`-R/./s.txt/-/upper` validated with `--command upper`:
`GetAsset[., s.txt]`, `Action{upper}`.)

### Scope and Acceptance Criteria

- **AC-1** One plan-making function
  WHEN a recipe's plan is needed for a key outside evaluation
  THEN it comes from `interpreter::make_recipe_plan`, which builds with `to_plan_for_key` and analyses as `make_plan` does, propagating an analysis error as `Err`
- **AC-2** Preview equals evaluation
  WHEN `get_asset_info` describes a recipe with a relative operand, a volatile upstream, or an expiring upstream
  THEN its `is_volatile` and `expires` equal the evaluated asset's metadata
- **AC-3** Errors are reported as before
  WHEN the recipe's analysis fails (a cycle)
  THEN `get_asset_info` returns `is_error: true` with the same message as today

### Design Readiness

- **Readiness:** ready
- **Automatic fixing:** not-eligible. Rule 2: a refactor that removes a duplicate path, which is
  none of the eligible kinds. It changes no behaviour (AC-2, AC-3).
- **Leading issue:** None
- **Explanation:** The defects that would have changed results are mitigated or absent at HEAD.
  The remainder is one function and one equivalence test.
- **Open questions:** None.

### Design Dependencies

- `plan-cwd-freeze`, `dependency-chain-analysis-cost` (complete, E4): the freeze and the single
  analysis pass this reuses.
- `cwd-less-recipe-analysed-at-root`: **overlaps** (weak). It changes how the walk resolves a
  recipe without `cwd`; AC-2 then guards that preview and run still agree. No ordering.
- `payload-requirement-skips-dependency-analysis`: **overlaps** (weak; it adds a sibling
  `build_plan` in the same file). Either order; the second rebases trivially.

## Phase 2: Architecture

### Solution

In `liquers-core/src/interpreter.rs`, next to `make_plan`:

```rust
/// The plan of the keyed recipe `recipe` for `key`, analysed as `make_plan` analyses a query, with
/// no entry CWD (a keyed asset's run starts with none; the recipe's `cwd` is its `SetCwd` step).
pub(crate) async fn make_recipe_plan<E: Environment>(
    envref: EnvRef<E>,
    recipe: &Recipe,
    key: &Key,
) -> Result<Plan, Error>;
```

Body: `let mut plan = recipe.to_plan_for_key(envref.get_command_metadata_registry(), key)?;
analyze_plan_dependencies(envref, &mut plan, None).await?; Ok(plan)`.

In `recipes.rs`, delete `create_plan_with_init_metadata`. Both callers pass `Some(key)`, so its
`None` branch is dead. They call `make_recipe_plan` instead. `get_asset_info` keeps its `Ok` and `Err`
arms; an analysis error now arrives in `Err`, whose projection (`is_error`, `message`,
`error_data`) is the one the `Ok` arm built from `plan.error` (AC-3). The comment on
`payload_required` is reworded for the new name. `recipe_plan` keeps returning an analysed,
unfrozen plan, so its contract for callers that finalize it later is unchanged.

Rejected: freeze in the preview as `finalize_plan_expanded` does. The analysis is cursor-based and
equivalent, and a frozen plan returned from `recipe_plan` would break "finalize a fresh plan once".
Also rejected: fold the analysis into `PlanBuilder::build` (synchronous, with no environment).

Known-issue preflight: none open touches `create_plan_with_init_metadata`.

### Changes

- `interpreter.rs`: `make_recipe_plan`.
- `recipes.rs`: remove `create_plan_with_init_metadata` and its TODOs; two call sites.
- `specs/reference/RECIPES_AND_PLANS.md`: the preview's flags come from `make_recipe_plan`.
  History row.
- No `pub` change: the removed function was private.

### Risks

| Risk | Handling | Certainty |
|---|---|---|
| A projection difference between the `Ok`-with-`plan.error` and `Err` arms | AC-3 compares the whole `AssetInfo` error fields | high |
| Divergence later, after `cwd-less-recipe-analysed-at-root` | AC-2 test fails if preview and run disagree | high |

## Phase 3: Examples and Tests

### Examples

The Problem Example. Two variants: `data/v.txt` = `-R/./w.txt/-/upper` where `w.txt` has
`volatile: true`, and the cycle `data/a.txt` ↔ `data/b.txt`.

### Tests

In `liquers-core/src/recipes.rs` `tests` (an `ImmediateEnvironment`, `DefaultRecipeProvider`,
`AsyncMemoryStore` with `data/recipes.yaml`, and a test-registered `upper`):

- `asset_info_flags_equal_the_evaluated_assets`: for `r.txt` and `v.txt`, compare
  `get_asset_info(key)` with `manager.get(key)` → `get_metadata()` (`expires`, `is_volatile`).
  Covers AC-2. Passes at HEAD (a guard).
- `asset_info_reports_an_analysis_cycle`: the cycle. `is_error` holds and the message names the
  cycle. Covers AC-3 (the message is captured at HEAD in step 1 and asserted equal after).
- `make_recipe_plan_propagates_an_analysis_error` in `interpreter.rs` tests: the cycle gives `Err`.
  Covers AC-1.

Run: `CARGO_INCREMENTAL=0 cargo test -p liquers-core --lib recipes::tests interpreter::tests`.

## Phase 4: Implementation Plan

### Steps

- [ ] 1. The two `recipes.rs` tests; both pass at HEAD. Record the cycle message. Proof: the run command.
- [ ] 2. `interpreter.rs` `make_recipe_plan` and its test. Proof: `--lib interpreter::tests`.
- [ ] 3. `recipes.rs`: replace the two calls; delete `create_plan_with_init_metadata`. Proof: step-1
  tests unchanged. Rollback: revert the file.
- [ ] 4. `RECIPES_AND_PLANS.md` line and History; close the issue with a resolution naming what was mitigated
  before. Proof: `python3 scripts/docs_index.py --check`.

### Validation

`cargo test -p liquers-core --lib --tests`; `cargo test -p liquers-lib --lib --tests` (directory
listings use `get_asset_info`); diff review.
