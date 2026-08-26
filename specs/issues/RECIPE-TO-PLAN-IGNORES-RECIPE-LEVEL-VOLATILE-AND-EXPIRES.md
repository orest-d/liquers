---
id: RECIPE-TO-PLAN-IGNORES-RECIPE-LEVEL-VOLATILE-AND-EXPIRES
kind: issue
title: Recipe to_plan ignores recipe level volatile and expires
status: draft
priority: P2
complexity: S
area: [core/plan, core/assets]
design: predecessor-cut-equivalence
created: 2026-08-26
github:
---
## Problem

`Recipe::to_plan` (`liquers-core/src/recipes.rs:214`) never reads `Recipe::volatile` or
`Recipe::expires`. It builds the plan from the query, applies argument and link overrides, and
prepends a `SetCwd`. The two recipe-level declarations are simply not carried onto the plan.

Measured:

```
recipe.volatile = true        -> plan.is_volatile = false
recipe.expires  = Immediately -> plan.is_volatile = false, plan.expires = Never
```

For volatility this is partly compensated downstream: `resolve_volatility_before_evaluation`
(`liquers-core/src/assets.rs:1610`) ORs `lock.recipe.volatile` and
`lock.recipe.expires.is_volatile()` into the asset's volatility, so *evaluation* honours the
flags. Nothing compensates on the plan itself.

For expiration nothing compensates at all at this level, while `Recipe::expires`'s own doc
comment claims it is "Recipe-level expiration combined with finalized plan expiration".

## Impact

**A recipe preview under-reports both.** `DefaultRecipeProvider::get_asset_info`
(`recipes.rs:467`) fills `AssetInfo` straight from the plan:

```rust
asset_info.is_volatile = plan.is_volatile;
asset_info.expires = plan.expires;
```

So a directory listing or asset preview shows `volatile: true, expires: immediately` recipes as
non-volatile and never-expiring. Display only — evaluation is unaffected for volatility — which
is why this is P2 and not higher. The `expires` gap is the wider of the two, since nothing
recovers it.

It also forces any consumer that needs "is this plan volatile" to know it must consult the
recipe separately, which is how `predecessor-cut-equivalence` came to need a second marker
rather than reusing `Plan::is_volatile`.

## Expected behaviour

`Recipe::to_plan` folds its own declarations into the plan it builds — `plan.is_volatile |=
self.volatile || self.expires.is_volatile()`, and the recipe expiration combined into
`plan.expires` as the field doc already promises — so that `Plan` is self-describing and a
consumer needs one source of truth rather than two.

**Not a free change**, which is why it is filed rather than folded into
`predecessor-cut-equivalence`: `finalize_plan` skips dependency registration for a volatile plan
(`if !plan.is_volatile { … }`), so folding makes a volatile *recipe* stop registering plan
dependencies, exactly as a volatile *plan* already does. That is arguably the correct and
consistent behaviour, but it is a change in blast radius beyond a display fix and wants its own
verification.

## Discovery

`predecessor-cut-equivalence` Phase 2, 2026-08-26, while checking whether a new `Plan` field was
needed to mark a plan uncuttable or whether `Plan::is_volatile` could carry it. Measured rather
than read. Related but distinct: `RECIPE-PLAN-ANALYSIS-RUNS-OUTSIDE-PLAN-BUILDING` is about
`get_asset_info` re-running the analysis passes; this is about `to_plan` not applying the
recipe's own declarations in the first place.
