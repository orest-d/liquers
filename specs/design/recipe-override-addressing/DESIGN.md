---
id: RECIPE-OVERRIDE-ADDRESSING
kind: design
title: Recipe arguments that address any command and the key of a resource segment
status: in_review
phase: implementation
readiness: needs-decision
autofix: not-eligible
area: [core/plan, core/query]
issues: [RECIPE-CANNOT-PARAMETERIZE-RESOURCE-KEY, RECIPE-OVERRIDE-ONLY-LAST-ACTION]
merged: 2026-10-20
affects_docs: [specs/reference/RECIPES_AND_PLANS.md, specs/reference/TEMPLATE_LANGUAGE.md, specs/reference/AGENT_MEMORY_SERVICE.md]
created: 2026-10-20
---
# Recipe arguments that address any command and the key of a resource segment

Bulk design (`specs/guides/autonomous_bulk_design.md`), 2026-10-20: Phases 1-4 written, not
approved, nothing implemented.

Two sources share this design because they strongly overlap (`overlap.md`): **T2** — both ask how a
`Recipe` argument names a place in its query other than the last action's argument, and the
key-hole issue's open question (text syntax versus a declaration in `Recipe`) is the same decision
as how to address an earlier action; **T3** — both change the override loop of `Recipe::to_plan`
and `overridable_argument`. Leading source: `RECIPE-CANNOT-PARAMETERIZE-RESOURCE-KEY` (the merged
scope is `L`). No exclusion applies (neither is eligible for automatic fixing; merged size `L`).

## Phases

- [Phase 1](./phase1-high-level-design.md) — readiness, problem example, AC-1…AC-8
- [Phase 2](./phase2-architecture.md)
- [Phase 3](./phase3-examples.md)
- [Phase 4](./phase4-implementation.md)
