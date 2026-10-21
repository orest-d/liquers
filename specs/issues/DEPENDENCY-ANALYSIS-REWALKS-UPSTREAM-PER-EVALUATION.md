---
id: DEPENDENCY-ANALYSIS-REWALKS-UPSTREAM-PER-EVALUATION
kind: feature
title: Each plan analysis re-walks the whole upstream recipe graph
status: draft
priority: P3
complexity: M
area: [core/plan, core/assets]
design: payload-requirement-skips-dependency-analysis
created: 2026-10-08
github:
---

## Problem

`analyze_plan_dependencies` (`liquers-core/src/plan.rs`) walks the whole reachable recipe graph of
the plan it analyses. Each recipe is visited once per analysis, which is linear, but every analysis
starts from nothing. An evaluation analyses its plan three times:
- the plan built for scheduling (`make_plan` in `schedule_plan_dependencies`);
- the plan built in `do_step`;
- `finalize_plan`.

So evaluating link *i* of a chain visits about 3·(i+1) recipes, and evaluating a whole chain of
*n* links costs O(n²) recipe visits. Measured: 200 links, 60 901 lookups, 4.5 s in a debug build.
The same applies to many dependents of one large shared upstream graph: each analysis re-walks
it.

## Problem Example

`chain_20_links_smoke` (`liquers-core/tests/dependency_chain_scaling.rs`) evaluates `-R/data/l0.txt`
… `-R/data/l20.txt` one at a time and counts 691 recipe lookups: about 3 × Σ i.

## Impact

Negligible for chains of up to a few hundred links; 200 links evaluate in 4.5 s (debug). Large
for very deep chains, at about 1 000 links (an estimated 30–100 s in debug), and for wide fan-in,
such as 1 000 assets over a shared 500-node upstream (about 500k visits, an estimated 30 s per
cold sweep).

## Expected behaviour

The work per evaluation is proportional to the plan's direct dependencies. The design discussion
favoured caching per-key *summaries* (declared volatility, combined expiry, acyclic), not
transitive closures. Better still, the summary could be read from the dependency's own live plan
or asset, whose invalidation the asset lifecycle already handles (recipe and command keys are
recorded dependencies). The cost to solve is invalidation correctness when a recipe is edited.
Sharing one walk across the three analyses of an evaluation is a smaller step, and was
deliberately not taken (simpler code first).

## Discovery

Deferred as an optional optimization in `dependency-chain-analysis-cost` Phase 1 ("Optional
optimization, not part of this design"), with the maintainer's agreement that it is low priority.

## Triage (2026-10-20)

Still present at HEAD 18732dd. Two of the three walks per evaluation come from
`impl RequiresPayload for Query` (`interpreter.rs`), which calls `make_plan`, a full analysis, only
to read `payload_required`, a fact of plan building. It does so for every scheduled dependency, which
is scheduled twice (the pre-pass and the step). The first slice, which removes them, is designed in
`design/payload-requirement-skips-dependency-analysis/`. The remainder (reusing summaries across
evaluations) is `DEPENDENCY-ANALYSIS-SUMMARIES-NOT-REUSED-ACROSS-EVALUATIONS`.
