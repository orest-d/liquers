---
id: PLAN-BUILDER-DUPLICATES-PIPELINE-PLANNING
kind: issue
title: Two plan builders plan the same queries once pipelines land
status: draft
priority: P3
complexity: M
area: [core/plan]
design: composite-command-definition
created: 2026-11-03
github:
---

## Problem

`design/composite-command-definition/` (Phase 2) routes every plan through the new
`PipelinePlanBuilder` (`liquers-core/src/pipeline/plan.rs`). It keeps the existing `PlanBuilder`
(`liquers-core/src/plan.rs`) behind `PlanningMode::Delegate`, which hands it every run of
elements that can be written as a query. Until the old builder is removed, the two encode the
same planning rules (planner instructions, resource header instructions, alias expansion, command
flags), and a fix to one must be repeated in the other.

## Impact

Maintenance only, while both exist. The dual-mode equality test (every corpus query planned both
ways must give equal plans) catches divergence. But each planning change costs twice, and
`Delegate` mode hides any bug in the native path.

## Expected behaviour

After `PlanningMode::Native` has been the default for a release with the equality test green:

1. remove `PlanningMode::Delegate` and the delegation code;
2. remove `PlanBuilder`'s query walk. Whatever `PlanAssembly` still needs moves to
   `pipeline/plan.rs`;
3. keep the equality test's corpus as a plain plan-snapshot test.

## Discovery

The maintainer's review of `composite-command-definition` Phase 2 on 2026-11-03: "make it possible
for the old PlanBuilder to retire".
