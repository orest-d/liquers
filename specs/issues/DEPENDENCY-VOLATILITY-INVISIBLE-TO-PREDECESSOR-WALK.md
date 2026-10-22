---
id: DEPENDENCY-VOLATILITY-INVISIBLE-TO-PREDECESSOR-WALK
kind: issue
title: Volatility a plan inherits at finalization is invisible to the predecessor-boundary walk
status: draft
priority: P3
complexity: M
area: [core/plan]
design:
created: 2026-10-20
github:
---
## Problem

`analyze_plan_dependencies` (`liquers-core/src/plan.rs`) sets `plan.is_volatile = true` when an
upstream recipe is `volatile: true` or expires `immediately`, but never calls
`upgrade_volatility_source`, so `Plan::volatility_source` stays `None`. `Plan::cut_predecessor`
builds each candidate fresh with `PlanBuilder`, without dependency analysis, so it sees neither
the inherited volatility nor its position. A boundary can therefore be cut over a prefix that reads
such a key. `VolatilitySource::Positional`'s rustdoc used to claim "a volatile dependency" was
recorded.

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
