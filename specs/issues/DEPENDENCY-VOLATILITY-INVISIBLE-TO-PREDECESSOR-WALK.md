---
id: DEPENDENCY-VOLATILITY-INVISIBLE-TO-PREDECESSOR-WALK
kind: issue
title: Volatility a plan inherits at finalization is invisible to the predecessor-boundary walk
status: closed
priority: P3
complexity: M
area: [core/plan]
design: inherited-volatility-bounds-predecessor-cut
created: 2026-10-20
github:
---
## Problem

`analyze_plan_dependencies` (`liquers-core/src/plan.rs`) sets `plan.is_volatile = true` when an
upstream recipe is `volatile: true` or expires `immediately`, but never calls
`upgrade_volatility_source`, so `Plan::volatility_source` stays `None`. `Plan::cut_predecessor`
builds each candidate fresh with `PlanBuilder`, without dependency analysis, so it sees neither
the inherited volatility nor its position. A boundary can therefore be cut over a prefix that reads
such a key. (`VolatilitySource::Positional`'s rustdoc now says inherited volatility records no
source; it used to claim "a volatile dependency" was recorded.)

## Problem Example

Recipe `vol.txt` is `volatile: true`. Finalizing `-R/vol.txt/-/upper/lower` marks the plan volatile
("Volatile due to dependency on volatile key") and then cuts it to
`[Evaluate(-R/vol.txt/-/upper), lower]`. Should be: no cut, because every candidate prefix reads
`vol.txt`; the plan stays `[GetAsset(vol.txt), upper, lower]`.

## Impact

The boundary is then planned by the asset manager's `make_plan`, found volatile, and evaluated
as a volatile, unregistered asset: an extra asset and hop that caches nothing. The value is
correct, so this is a cost, not a soundness problem.

## Expected behaviour

Either the walk consults the inherited volatility (per candidate, e.g. by running the cheap
recipe-volatility part of the analysis on each candidate), or inherited volatility is recorded with
a position the walk can respect, or the cost is accepted and documented as it is now.

## Discovery

Found while reviewing `reference/api/DOC_08_RECIPES_PLANS.md` against the code at `96279f14` before promoting it to [`reference/RECIPES_AND_PLANS.md`](../reference/RECIPES_AND_PLANS.md) (2026-10-20).

## Resolution

Fixed 2026-10-27 by `design/inherited-volatility-bounds-predecessor-cut/`. The dependency analysis (`analyze_plan_dependencies_at`, crate-private, in `liquers-core/src/plan.rs`) reports the first top-level step whose read makes the plan volatile by inheritance, and `finalize_plan` cuts through the crate-private `Plan::cut_predecessor_bounded`, which steps back over every candidate containing that step ("it reads a volatile dependency"). `-R/vol.txt/-/upper/lower` with a volatile `vol.txt` stays expanded; a pure prefix ahead of the read is still cut. The public `analyze_plan_dependencies`, `finalize_plan_expanded` and `Plan::cut_predecessor` keep their signatures. Evidence: `inherited_volatility_position_is_the_reading_step`, `immediately_expiring_upstream_has_a_position`, `bounded_cut_steps_back_over_a_volatile_read`, `bounded_cut_keeps_the_pure_prefix` (plan.rs) and `finalize_plan_does_not_cut_over_inherited_volatility` (interpreter.rs, fails before the fix).
