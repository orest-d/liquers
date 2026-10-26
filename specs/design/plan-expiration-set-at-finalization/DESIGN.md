---
id: PLAN-EXPIRATION-SET-AT-FINALIZATION
kind: design
title: Plan finalization applies the plan's expiration to the asset
form: compact
status: in_review
phase: implementation
readiness: needs-decision
autofix: not-eligible
area: [core/context, core/plan]
issues: [PLAN-EXPIRATION-REACHES-THE-CONTEXT-ONLY-THROUGH-APPLY-RECIPE]
created: 2026-10-26
---
# Plan finalization applies the plan's expiration to the asset

Bulk design (`specs/guides/autonomous_bulk_design.md`), 2026-10-26: Phases 1-4 written, not
approved, nothing implemented.

## Phase 1: High-Level Design

### Purpose

Applying a recipe is four calls that every `Environment::apply_recipe` must repeat: `to_plan`,
`finalize_plan`, `context.set_expires(...)`, `apply_plan`. The third step is a convention with no
enforcement. A custom environment that skips it (and the test environment in
`liquers-core/tests/environment_builder.rs` does) produces assets that never expire, although their
plan says they should. Expiration should reach the asset as part of finalization, like the
dependency records finalization already seeds, so the convention disappears.

### Problem Example

Verified at HEAD (cd6d5da). `GenericEnvironment::apply_recipe` (`liquers-core/src/context.rs`) is
the only caller of `Context::set_expires`:

```rust
finalize_plan(envref.clone(), &mut plan, &context, &input_state).await?;
let combined_expires = plan.expires.clone() | recipe_expires;   // redundant: to_plan folded it in
context.set_expires(combined_expires).await?;
apply_plan(plan, input_state, context, envref).await
```

`CustomEnvironment::apply_recipe` in `environment_builder.rs` calls `to_plan`, `finalize_plan` and
`apply_plan` only. Register a command `expiring` with `expires: "in 5 min"` in that environment and
evaluate the query `expiring` (`liquers-validate --command expiring -- 'expiring'`: `Ok`). The plan
has `expires = in 5 min`. Today the asset's metadata has `expires: Never` and no expiration time.
It should be `in 5 min`, with an expiration time about five minutes ahead.

### Scope and Acceptance Criteria

- **AC-1** A custom environment gets the expiration
  WHEN a hand-written `apply_recipe` calls `to_plan`, `finalize_plan` and `apply_plan`, and the plan
  expires `in 5 min`
  THEN the evaluated asset's metadata has `expires` = `in 5 min` and an expiration time in the future
- **AC-2** Built-in environments unchanged
  WHEN the same query is evaluated in `SimpleEnvironment` and `ImmediateEnvironment`
  THEN the asset's `expires` and expiration time are as today
- **AC-3** Recipe and upstream expiration still combine
  WHEN a keyed recipe declares `expires: in 1 hour` and reads a key whose recipe expires `in 5 min`
  THEN the asset expires `in 5 min`, as today
- **AC-4** Expanded finalization does the same
  WHEN `finalize_plan_expanded` finalizes a plan that expires `in 5 min`
  THEN the context's asset carries that expiration, as for `finalize_plan`

Non-goals: the legacy free function `interpreter::evaluate`, which never finalizes (its own TODO);
changing `Expires` combination rules; commands setting expiration at run time.

### Design Readiness

- **Readiness:** needs-decision
- **Automatic fixing:** not-eligible — an open decision on where the step lives (rule 5). Under the
  recommended answer, the change is a bug fix inside `liquers-core` that adds no `pub` item. It
  only adds a documented effect to `finalize_plan_expanded` and would then be eligible.
- **Leading issue:** Proposed resolution - where expiration is applied.
- **Explanation:** all three places are feasible and Phases 2-4 are written for the recommended
  one. The choice changes the documented contract of a public function or adds a new public
  function, so it is shown for review.
- **Open questions:**
  1. **Proposed resolution - where expiration is applied.** (a) **Recommended:**
     `finalize_plan_expanded` sets it after dependency analysis. Finalization already writes to the
     context (it seeds dependency records), every recipe application already calls it, and the plan's
     expiration is final at that point. (b) A new public `interpreter::apply_recipe_plan(envref,
     input_state, recipe, context)` doing all four steps, which custom environments call. This adds an
     API, and an implementation that does not call it still loses expiration. (c) `apply_plan` sets
     it. This is wrong for nested `Step::Plan`, which calls `apply_plan` with a plan whose `expires`
     would overwrite the outer asset's, and for unfinalized plans.

### Design Dependencies

- `pure-command-context` — **overlaps** (weak): it refuses `set_expires` from a command's pure
  context. Finalization uses the asset's own full context, so it is unaffected. No order.
- `cancel-point-of-no-return` — **overlaps** (weak): it edits `apply_plan_state` and the run
  harness, not finalization. No order.
- `plan-step-state-metadata` (complete) defines what each step hands on; unaffected.

### Consolidated Findings

- `Recipe::to_plan` already folds `recipe.expires` into `plan.expires` (`recipes.rs`, "`to_plan` has
  already folded the recipe's own `expires`"), so `plan.expires | recipe.expires` equals
  `plan.expires`. Dropping the `|` changes nothing.
- `Context::set_expires` replaces the expiration rather than combining it. Calling it once at
  finalization and again from a custom `apply_recipe` that still does it is harmless, because both
  pass the same value.
- `finalize_plan` must not be called twice on one plan (its rustdoc). Setting expiration once per
  finalization therefore sets it once per evaluation.
- Documents that describe the four-step convention change: `RECIPES_AND_PLANS.md` (finalization
  steps and Known limitations), `DOC_04_ENVIRONMENT_CONTEXT_EVALUATION.md` (the `apply_recipe`
  list and `set_expires` paragraph), the `Environment::apply_recipe` and `Context::set_expires`
  rustdoc.

## Phase 2: Architecture

### Solution

1. `liquers-core/src/interpreter.rs` `finalize_plan_expanded`: after
   `analyze_plan_dependencies(...)?` and before the dependency seeding, add
   `context.set_expires(plan.expires.clone()).await?;`. The rustdoc list gains "Applies the plan's
   final expiration to the context's asset (`Context::set_expires`)".
2. `liquers-core/src/context.rs` `GenericEnvironment::apply_recipe`: remove `recipe_expires`, the
   combination and the `set_expires` call. The body becomes `to_plan`, `finalize_plan`,
   `apply_plan`.
3. `Environment::apply_recipe` rustdoc: "build the plan, finalize it (which also applies the plan's
   expiration) and call `apply_plan`". `Context::set_expires` rustdoc: "called by
   `finalize_plan_expanded`; an environment does not need to call it".

Errors: `set_expires` returns `Err` only when the metadata cannot hold an expiration (legacy
metadata). This now surfaces from finalization instead of from `apply_recipe`. The path and the
error are the same. Async, as today.

Rejected alternatives: (b) and (c) above.

### Changes

- `liquers-core/src/interpreter.rs`: `finalize_plan_expanded` (one call, rustdoc).
- `liquers-core/src/context.rs`: `GenericEnvironment::apply_recipe`, two rustdoc blocks.
- `liquers-core/tests/environment_builder.rs`: one test (AC-1). `CustomEnvironment::apply_recipe`
  stays as it is: it is the case being fixed.
- Documents: `specs/reference/RECIPES_AND_PLANS.md` (finalization list: a step "applies the final
  expiration to the context"; delete the "This is the only place …" paragraph and the Known
  limitations bullet); `specs/reference/api/DOC_04_ENVIRONMENT_CONTEXT_EVALUATION.md` (`apply_recipe`
  list becomes three steps; `set_expires` paragraph). Each gets a History row and a `reviewed:` bump.

No `pub` signature, command, registry or format change.

### Risks

| Assessment | Finding |
|---|---|
| Files likely to change | `interpreter.rs`, `context.rs`, `environment_builder.rs`, two documents |
| Existing tests | tests that call `finalize_plan` on a temporary asset's context now also set its expiration. Values are unchanged; `expiration_integration`, `plan_cwd_freeze`, `plan_step_state_metadata` are run to confirm |
| Readers of `finalize_plan_expanded` | documented "for reading" with its own context. It already seeds dependency records there, so one more write to that context is the same kind of effect |
| Compatibility | a custom environment gains expiration it was silently missing (the fix); one that already sets it is unaffected |
| Concurrency | `set_expires` takes the asset's data lock briefly, as in `apply_recipe` today, at the same point in the evaluation |
| Recovery | revert |
| Certainty | high |

## Phase 3: Examples and Tests

### Examples

The Problem Example: `expiring` with `expires: "in 5 min"` in `CustomEnvironment`. The query
`expiring` was validated with `liquers-validate --command expiring` (`Ok`, one `Action` step).

### Tests

- `liquers-core/tests/environment_builder.rs` `custom_apply_recipe_gets_plan_expiration` (AC-1):
  `CustomEnvironment`, command `expiring` with `CommandMetadata::expires = "in 5 min"`, evaluate
  `expiring`. Asset metadata `expires()` is `Expires::InDuration(300 s)` and `expiration_time()` is
  in the future. Fails at HEAD (`Never`).
- `liquers-core/tests/expiration_integration.rs` — existing `test_register_command_expires_in_plan`
  and `test_timed_expiration` (AC-2). Add `recipe_and_upstream_expiration_combine_on_the_asset`
  (AC-3): memory store, `recipes.yaml` with `a.txt` (`expires: in 5 min`) and `b.txt` reading
  `-R/a.txt` with `expires: in 1 hour`. Evaluating `-R/b.txt` gives an asset expiring `in 5 min`.
- `liquers-core/src/interpreter.rs` `tests::finalize_plan_expanded_sets_context_expiration` (AC-4):
  a plan built for `expiring`, a temporary asset's context, `finalize_plan_expanded`; then
  `context.get_metadata()` has the expiration.

Run: `CARGO_INCREMENTAL=0 cargo test -p liquers-core --test environment_builder --test
expiration_integration` and `cargo test -p liquers-core --lib finalize_plan_expanded`.

## Phase 4: Implementation Plan

### Steps

- [ ] 1. Tests — add `custom_apply_recipe_gets_plan_expiration`, `recipe_and_upstream_expiration_combine_on_the_asset`, `finalize_plan_expanded_sets_context_expiration`; the first and third fail at HEAD — `cargo test -p liquers-core --test environment_builder --test expiration_integration`
- [ ] 2. `interpreter.rs` `finalize_plan_expanded` — `context.set_expires(plan.expires.clone())` after analysis; rustdoc — depends on 1 — step 1 tests pass. Rollback: revert.
- [ ] 3. `context.rs` `GenericEnvironment::apply_recipe` — drop the explicit combination and `set_expires`; rustdoc of `Environment::apply_recipe` and `Context::set_expires` — depends on 2 — `cargo test -p liquers-core --lib --tests`. Rollback: revert 2-3 together.
- [ ] 4. Documents — `RECIPES_AND_PLANS.md`, `DOC_04_ENVIRONMENT_CONTEXT_EVALUATION.md` (History rows, `reviewed:`) — `python3 scripts/docs_index.py --check`
- [ ] 5. Close `PLAN-EXPIRATION-REACHES-THE-CONTEXT-ONLY-THROUGH-APPLY-RECIPE` with a resolution note; this design to `complete` — `python3 scripts/docs_index.py` then `--check`

### Validation

`cargo fmt --check -p liquers-core`; `CARGO_INCREMENTAL=0 cargo test -p liquers-core --lib --tests`;
`cargo test -p liquers-lib --lib --tests` (every built-in environment goes through the changed
path); `bash scripts/check-build-matrix.sh` wasm32 rows (`ImmediateEnvironment`). Diff review: the
three source files, the two documents and the records.
