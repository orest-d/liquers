---
id: PLAN-POLICY
kind: design
title: Retention flags (stored, cached) through command, plan, directive and asset, positional volatility, and retirement of the plan-builder policy markers
workflow: liquers-project
status: in_review
phase: high-level
area: [core/plan, core/assets, core/commands, core/query]
issues: [CORE-PLAN-POLICY-AND-DEFAULTS, V-INSTRUCTION-IS-WHOLE-PLAN-NOT-POSITIONAL]
merged: 2026-10-10
affects_docs: [specs/reference/api/DOC_08_RECIPES_PLANS.md, specs/reference/REGISTER_COMMAND_FSD.md, specs/reference/ASSETS.md, specs/reference/PROJECT_OVERVIEW.md]
created: 2026-10-10
---
# Plan Policy Design Tracking

**Created:** 2026-10-10

Owns the remainder of `CORE-PLAN-POLICY-AND-DEFAULTS` after `predecessor-cut-equivalence`
settled its `expand_predecessors` half, plus `V-INSTRUCTION-IS-WHOLE-PLAN-NOT-POSITIONAL`,
merged on 2026-10-10 at the user's request.

## Phase Status

- [ ] Phase 1: High-Level Design — in review
- [ ] Phase 2: Architecture
- [ ] Phase 3: Examples and Tests
- [ ] Phase 4: Implementation Plan
- [ ] Implementation
- [ ] Phase 5: Documentation

## Notes

- 2026-10-10: started as a compact design (source complexity `M`). After the Phase 1 discussion
  the scope grew to `L` (see Phase 1, Scope Changes), so the design was converted to the full form.
- User decisions recorded on 2026-10-10: directive syntax `stored-<bool>` / `cached-<bool>`; positional
  `v` folded in; flags plus a log entry are enough to explain a missing value (no reason field).
