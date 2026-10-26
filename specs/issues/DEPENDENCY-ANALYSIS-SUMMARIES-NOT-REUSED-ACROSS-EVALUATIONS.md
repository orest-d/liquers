---
id: DEPENDENCY-ANALYSIS-SUMMARIES-NOT-REUSED-ACROSS-EVALUATIONS
kind: feature
title: Every evaluation re-walks its whole upstream recipe graph to recompute summaries it already computed
status: draft
priority: P3
complexity: M
area: [core/plan, core/assets]
design: dependency-summary-reuse
created: 2026-10-20
github:
---

## Problem

The remainder of `DEPENDENCY-ANALYSIS-REWALKS-UPSTREAM-PER-EVALUATION` after its first slice
(`specs/design/payload-requirement-skips-dependency-analysis/`, which removes the two wasted walks
per scheduled dependency). Each evaluation still runs one `analyze_plan_dependencies` walk
(`liquers-core/src/plan.rs`) over its whole upstream, starting from an empty memo. So evaluating a
chain of *n* links one at a time remains O(n²) recipe visits, and wide fan-in over a shared upstream
re-walks it for every dependent.

## Problem Example

`liquers-core/tests/dependency_chain_scaling.rs` `evaluate_chain(20)` evaluates `l0 … l20` one at a
time. Evaluating `l20` walks `l19 … l0` although each was finalized moments before: about 691
recipe lookups today, about 271 after the first slice. Should be: `l20` stops at `l19`, roughly 3
lookups per link.

## Impact

Negligible up to a few hundred links. Large for chains of about 1 000 links or for fan-in of about
1 000 assets over a 500-node upstream (estimates in `dependency-chain-analysis-cost` Phase 1).

## Expected behaviour

The work per evaluation is proportional to the direct dependencies. Possible ways:

- a memo shared by the analyses of one evaluation session. This makes a cold deep request linear,
  but does not help one-at-a-time evaluation;
- a per-key summary (declared volatility, combined expiry) read from the dependency's live `Ready`
  asset or cached per key. This needs a sound invalidation when a recipe is edited, and recipe
  dependency keys are never versioned today (`RECIPE-DEPENDENCY-KEY-IS-NEVER-VERSIONED`). It also
  needs a rule for `Override` and `Source` values, whose stored metadata carries no recipe summary.

## Discovery

Split from `DEPENDENCY-ANALYSIS-REWALKS-UPSTREAM-PER-EVALUATION` in the 2026-10-20 bulk design.
