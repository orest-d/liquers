---
id: PREDECESSOR-EVALUATION-BOUNDARY
kind: design
title: Predecessor evaluation boundary in PlanBuilder
workflow: liquers-project
status: draft
phase: high-level
area: [core/plan, core/assets]
gh_pr: []
issues: [CORE-RECIPES-EXPAND-PREDECESSORS-CRASH]
affects_docs: []
created: 2026-08-14
superseded_by:
---
# predecessor-evaluation-boundary Design Tracking

**Created:** 2026-08-14

## Phase Status

- [x] Phase 1: High-Level Design (awaiting approval)
- [ ] Phase 2: Solution & Architecture
- [ ] Phase 3: Examples & Testing
- [ ] Phase 4: Implementation Plan
- [ ] Phase 5: Documentation
- [ ] Implementation Complete

## Notes

Phase 1 established by experiment, not by reading: enabling `disable_expand_predecessors()` in
`Recipe::to_plan` produces 11 failures in `cargo test -p liquers-core --lib` with four distinct
root causes. The issue's premise ("a crash nobody has diagnosed") is wrong on both counts — nothing
panics in the named test, and the failure is the documented `payload: required` declaration rule.
The serious defects are the filename cut point and the un-harvested sub-plan properties.

## Links

- [Phase 1](./phase1-high-level-design.md)
- [Phase 2](./phase2-architecture.md)
- [Phase 3](./phase3-examples.md)
- [Phase 4](./phase4-implementation.md)
- [Phase 5](./phase5-documentation.md)
