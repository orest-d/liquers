---
id: RECIPE-TO-PLAN-IGNORES-RECIPE-LEVEL-VOLATILE-AND-EXPIRES
kind: issue
title: Recipe to_plan ignores recipe level volatile and expires
status: closed
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

## Resolution, 2026-08-26

Closed by `predecessor-cut-equivalence` step 3. `Recipe::to_plan` folds both declarations onto
the plan: `volatile:` and a volatile `expires:` set `is_volatile` and
`VolatilitySource::Declared`; a finite `expires:` is combined into `plan.expires`, which is what
that field's own doc comment already promised.

The blast radius that prompted the original caution — `finalize_plan` skipping dependency
registration for a volatile plan — is real and is now asserted rather than assumed:
`volatile_recipe_skips_dependency_registration`. Nineteen suites had stayed green through the
change precisely because nothing tested it.

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

**Taken into `predecessor-cut-equivalence`** at the author's direction, rather than deferred.

The blast radius that prompted the caution was measured rather than argued: `finalize_plan` skips
dependency registration for a volatile plan (`if !plan.is_volatile { … }`), so folding makes a
volatile *recipe* stop registering plan dependencies, exactly as a volatile *plan* already does.
Applied as a probe, **all 19 `liquers-core` suites and the `liquers-lib --lib --tests` loop stay
green**. Green is not proof — no existing test asserts a volatile recipe's dependency records, so
that design's Phase 3 owes one.

The volatile half also feeds that design directly: it contributes
`Plan::volatility_source = Declared`, the marker that makes a whole-plan volatility declaration
visible to the predecessor cut. See its `phase2-architecture.md`, §"Folding the recipe's
declarations into the plan".

## Discovery

`predecessor-cut-equivalence` Phase 2, 2026-08-26, while checking whether a new `Plan` field was
needed to mark a plan uncuttable or whether `Plan::is_volatile` could carry it. Measured rather
than read. Related but distinct: `RECIPE-PLAN-ANALYSIS-RUNS-OUTSIDE-PLAN-BUILDING` is about
`get_asset_info` re-running the analysis passes; this is about `to_plan` not applying the
recipe's own declarations in the first place.
