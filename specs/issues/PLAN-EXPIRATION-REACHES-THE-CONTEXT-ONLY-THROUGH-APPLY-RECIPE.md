---
id: PLAN-EXPIRATION-REACHES-THE-CONTEXT-ONLY-THROUGH-APPLY-RECIPE
kind: issue
title: A plan's expiration reaches the asset only through apply_recipe
status: draft
priority: P2
complexity: M
area: [core/context, core/plan]
design: plan-expiration-set-at-finalization
created: 2026-10-20
github:
---
## Problem

`Context::set_expires` is called only in `GenericEnvironment::apply_recipe`
(`liquers-core/src/context.rs`). Neither `interpreter::finalize_plan` nor `apply_plan` applies
`plan.expires` to the context, so recipe application is a convention every `Environment`
implementation must repeat: `to_plan`, `finalize_plan`, set the expiration, `apply_plan`. A custom
`apply_recipe` that skips the third step (as `liquers-core/tests/environment_builder.rs` does)
produces assets that never expire. The combination there (`plan.expires | recipe.expires`) is also
redundant now that `to_plan` folds the recipe's `expires` in.

## Problem Example

In `liquers-core/tests/environment_builder.rs`, `CustomEnvironment` registers a command `expiring`
with `expires: "in 5 min"` and evaluates the query `expiring`. Its plan expires `in 5 min`, but the
asset's metadata says `expires: Never` and has no expiration time. In `SimpleEnvironment` the same
query expires in five minutes. Both should.

## Impact

Every built-in environment is an alias of `GenericEnvironment`, so nothing built in is affected;
custom environments (and future language integrations) can silently lose expiration.

## Expected behaviour

One shared recipe-application helper, or `apply_plan` applying `plan.expires` itself, so the
sequence is not a convention.

## Discovery

Found while reviewing `reference/api/DOC_08_RECIPES_PLANS.md` against the code at `96279f14` before promoting it to [`reference/RECIPES_AND_PLANS.md`](../reference/RECIPES_AND_PLANS.md) (2026-10-20).
