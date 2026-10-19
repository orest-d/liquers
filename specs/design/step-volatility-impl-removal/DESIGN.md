---
id: STEP-VOLATILITY-IMPL-REMOVAL
kind: design
title: Remove the unreachable step-level volatility check and its runtime print
status: complete
gh_pr: [146]
form: compact
readiness: ready
autofix: eligible
area: [core/plan]
issues: [RESOURCE-STEP-VOLATILITY-IS-NEVER-CHECKED]
created: 2026-10-17
---
# Remove the unreachable step-level volatility check and its runtime print

> **Complete on 2026-10-18**: implemented and merged in #146.

Produced under [`guides/autonomous_bulk_design.md`](../../guides/autonomous_bulk_design.md) by the
2026-10-17 backlog compaction: Phases 1-4, reviewed without phase approval. Not an approval and not
an implementation.

## Phase 1: High-Level Design

### Purpose

`impl IsVolatile<E> for Step` (`liquers-core/src/interpreter.rs`) answers volatility per step,
with `GetResource` / `GetResourceMetadata` printing `ADD SUPPORT FOR RESOURCE VOLATILITY CHECK!`
and returning `false`, and `GetAssetDirectory` / `GetResourceDirectory` returning `true`. The
issue left open whether it is reachable. It is not: `IsVolatile` is `pub(crate)`; `Query`,
`Recipe` and `ParameterValue` all go through `IsVolatile for Plan`, which returns the cached
`Plan::is_volatile` set by `PlanBuilder`; nothing calls it on a `Step` except the unit test
`step_is_volatile_when_alias_is_volatile`. The impls for `ParameterValue` and
`ResolvedParameterValues` exist only to serve it.

### Problem Example

```rust
Step::GetResourceDirectory(parse_key("data")?).is_volatile(envref).await  // Ok(true)
PlanBuilder::new(parse_query("-R-sdir/data")?, cmr).build()?.is_volatile    // false
```

Today the two answers disagree, and the resource arms would print to stderr if anyone asked.
Should be: one authority — the plan — and no runtime print.

### Scope and Acceptance Criteria

- **AC-1** One authority
  - WHEN volatility of a query, recipe or link is asked through `IsVolatile`
  - THEN it is answered from `Plan::is_volatile`, and no step-level impl exists
- **AC-2** Resource steps documented
  - WHEN a reader looks up whether a resource step (`-R-stored/…`, `-R-stored_meta/…`, `-R-sdir/…`: read straight from the store) makes a
    plan volatile
  - THEN `DOC_08_RECIPES_PLANS.md` says it does not: store data is tracked by version and
    dependency records, not by volatility
- **AC-3** Alias coverage kept
  - WHEN a volatile alias of a non-volatile target is planned
  - THEN the plan is volatile (the case the deleted step test checked, now at plan level)
- **AC-4** No print
  - WHEN the crate is searched for `ADD SUPPORT FOR RESOURCE VOLATILITY CHECK`
  - THEN nothing is found

Non-goals: runtime volatility (PR #142); a volatile *store resource* concept, which does not exist.

### Design Readiness

- **Readiness:** ready
- **Automatic fixing:** eligible — bug fix in one crate (`liquers-core/src/interpreter.rs`,
  `plan.rs` tests); removes crate-private impls only; no `pub` item, command or format changes.
  The one-line reference note is documentation of existing behaviour.
- **Leading issue:** None
- **Explanation:** The issue's two options were "decide from the resource" or "remove the step
  impl"; repository evidence settles it: the impl is unreachable and contradicts the plan, while
  the planner never marks resource steps volatile. The choice is invisible to callers.
- **Open questions:** None.

### Design Dependencies

- `overlaps` `core-tracing-migration` (weak: it maps these two `eprintln!` lines to `trace!`;
  whichever lands second drops or skips them).
- `overlaps` `runtime-volatility` (PR #142; weak: it names this issue as separate).

### Consolidated Findings

Deleting `step_is_volatile_when_alias_is_volatile` removes one test cited as AC-8 by the complete
`command-alias-contract` design; AC-3's plan-level test replaces it. The compiler proves
unreachability: step 1 fails to build if anything still calls `Step::is_volatile`.

## Phase 2: Architecture

### Solution

In `liquers-core/src/interpreter.rs` delete `impl IsVolatile<E> for Step`,
`impl IsVolatile<E> for ResolvedParameterValues` and `impl IsVolatile<E> for ParameterValue`,
and the test `step_is_volatile_when_alias_is_volatile`. Keep the trait and the `Plan`, `Recipe`,
`Query` impls (used by `AsyncAssetManager::is_volatile` / `is_volatile_query` in `assets.rs`).
Add to the `Plan` impl's comment that the plan is the only authority.

In `specs/reference/api/DOC_08_RECIPES_PLANS.md`, after the `v` paragraph: resource steps
(`GetResource*`) never make a plan volatile; a resource's changes reach dependants through its
version and dependency records. History row; bump `reviewed:`.

Rejected: decide volatility from resource metadata (no producer of a volatile store resource
exists, and the plan would still not consult it); keep the impl and only drop the prints (keeps a
second, contradicting authority).

### Changes

`interpreter.rs` (three impls, one test, one comment); `plan.rs` tests (one test); DOC_08.

### Risks

A caller outside the crate cannot exist (`pub(crate)` trait). If one inside appears at step 1, the
build fails: stop, keep the impl, and switch to dropping the prints and documenting the arms
(Rollback). Certainty: high.

## Phase 3: Examples and Tests

### Tests

- `volatile_alias_of_stable_target_makes_plan_volatile` (new, `plan.rs` tests, beside
  `alias_planning_inherits_target_volatility`): registry with `steady` and `jumpy` (volatile alias
  of `steady`); `PlanBuilder::new(parse_query("jumpy")?, &registry).build()?.is_volatile` — AC-1, AC-3
- `alias_planning_inherits_target_volatility` (existing) — AC-1
- AC-4: `grep -rn "ADD SUPPORT FOR RESOURCE" liquers-core/src` returns nothing. AC-2: review.

Command: `cargo test -p liquers-core --lib alias`.

## Phase 4: Implementation Plan

### Steps

- [x] 1. `interpreter.rs` — remove the three impls and the step test — `cargo check -p liquers-core --tests`. Rollback: if a caller appears, restore and only drop the prints
- [x] 2. `plan.rs` — add `volatile_alias_of_stable_target_makes_plan_volatile` — `cargo test -p liquers-core --lib alias`
- [x] 3. DOC_08 note, History row, `reviewed:`; close the issue — `python3 scripts/docs_index.py --check`

### Validation

`cargo test -p liquers-core --lib --tests`; the AC-4 grep.
