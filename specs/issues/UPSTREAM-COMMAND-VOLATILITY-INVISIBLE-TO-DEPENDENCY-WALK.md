---
id: UPSTREAM-COMMAND-VOLATILITY-INVISIBLE-TO-DEPENDENCY-WALK
kind: issue
title: A recipe preview misses volatility its upstream inherits from a volatile command
status: draft
priority: P2
complexity: S
area: [core/plan]
design: 
created: 2026-11-03
github:
---
## Problem

The dependency walk (`DependencyWalk::open` in `liquers-core/src/plan.rs`) summarises an upstream
recipe with `self_volatile: recipe.volatile` and folds in `recipe_plan.expires`, but never reads
`recipe_plan.is_volatile`. So an upstream whose recipe runs a `volatile: true` *command* (its
recipe does not say `volatile: true`) does not make the dependent's plan volatile at analysis.

With `data/w.txt = volatile_source/w.txt` (`register_command!(… volatile: true)`) and
`data/v.txt = -R/./w.txt/-/upper/v.txt`, `get_asset_info(data/w.txt).is_volatile` is `true` but
`get_asset_info(data/v.txt).is_volatile` is `false`, and the plan's `is_volatile` is `false` too
— while evaluating `data/v.txt` stores metadata with `is_volatile: true` (the run learns it from
the evaluated dependency, `runtime-volatility`). Expiry from an upstream command is folded in;
only volatility is missed.

## Impact

Previews (`get_asset_info`, `recipe_plan`, the REST recipe endpoint, directory listings) report a
volatile asset as non-volatile. Anything deciding from the analysed plan before the run — the
predecessor-boundary cut, caching decisions — sees the asset as cacheable until it has run.
Workaround: declare `volatile: true` on the upstream recipe.

## Expected behaviour

The walk treats an upstream whose own plan is volatile like one whose recipe declares it, so the
preview and the evaluated metadata agree.

## Discovery

Found 2026-11-03 while implementing `recipe-preview-shares-plan-analysis`: its preview-equals-run
test failed for a command-volatile upstream, and passes for a recipe-declared one, which is what
that design's example uses. The test `liquers-core/tests/recipe_preview_plan.rs` is where a
regression test for this belongs.
