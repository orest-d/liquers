---
id: RECIPE-PLAN-ANALYSIS-RUNS-OUTSIDE-PLAN-BUILDING
kind: issue
title: Recipe plan analysis runs outside plan building
status: draft
priority: P3
complexity: M
area: [core/plan, core/assets]
design: 
created: 2026-08-26
github:
---
## Problem

`create_plan_with_init_metadata` (`liquers-core/src/recipes.rs:487`) builds a recipe's plan and
then runs the volatility and expiration passes itself, carrying two markers left by whoever
wrote it:

```rust
let _ = has_volatile_dependencies(envref.clone(), &mut plan, None).await;
    // TODO: looks suspicious, this should be done in plan building or checking
if plan.error.is_none() {
    let _ = has_expirable_dependencies(envref, &mut plan).await;
        // TODO: looks suspicious, this should be done in plan building or checking
}
```

plus a third on the function itself: `// TODO: missleading name, use conventioanl plan building
functionality`.

Three things are off, beyond the placement the markers name:

1. **Both results are discarded.** `let _ =` throws the `Result` away, so an analysis failure is
   invisible and the plan continues with whatever flags it had.
2. **The entry CWD is `None`**, while `finalize_plan` passes `context.get_cwd_key()`. A recipe
   with a relative dependency is therefore analysed against logical root here and against the
   real working key on the evaluation path, so the two can disagree about what the dependencies
   are.
3. **It duplicates `finalize_plan`**, which runs the same two passes in the same order after
   freezing. This copy does not freeze, so it analyses source-relative operands — the condition
   `plan-cwd-freeze` was written to remove, surviving on a second path.

The function feeds `RecipeProvider::get_asset_info`, so the `is_volatile` and `expires` a
directory listing or asset preview reports come from this path rather than the evaluation one.

## Impact

A preview can disagree with the evaluation about volatility, expiration, or dependencies for a
recipe with relative operands — reported values only, not evaluation behaviour, and the
evaluation path is the one that is right. Nothing observed failing; found by reading.

P3 rather than higher because the wrong value is displayed rather than acted on. It would rise
if anything ever made a decision from `AssetInfo::is_volatile`.

## Expected behaviour

One analysis path. `get_asset_info` should obtain flags the same way evaluation does — freeze
against the same entry key, run the passes once, and propagate their errors instead of
discarding them — rather than keeping a second, unfrozen copy of the sequence.

Related but not the same: `CORE-EVALUATE-PATH-CONSOLIDATION` is about the evaluation routes
disagreeing on dependency recording; this is about plan *analysis* being run twice, differently,
for display.

## Discovery

Noticed while auditing `predecessor-cut-equivalence` for open questions, 2026-08-26 — reading
`recipes.rs` for how a recipe's `volatile:` flag reaches an asset. Out of scope for that design,
which does not touch the analysis passes.
