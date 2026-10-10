---
id: PLAN-STEP-STATE-METADATA
kind: design
title: A plan step's input state is the state its predecessor would produce as an asset
workflow: liquers-project
status: in_review
phase: documentation
readiness: ready
autofix: not-eligible
area: [core/plan, core/context]
issues: [FREE-FUNCTION-EVALUATE-BAKES-A-BIN-DATA-FORMAT, CONTEXT-STEP-REPLACES-INPUT-STATE-METADATA]
merged: 2026-10-10
affects_docs: [specs/reference/api/DOC_04_ENVIRONMENT_CONTEXT_EVALUATION.md, specs/reference/api/DOC_08_RECIPES_PLANS.md, specs/reference/VALUE_TYPE_SYSTEM.md, specs/guides/COMMAND_REGISTRATION_GUIDE.md]
created: 2026-10-10
---
# plan-step-state-metadata Design Tracking

Defines what metadata the state handed from one plan step to the next carries, taking the cut
plan as the reference: a predecessor boundary hands its asset's state on unchanged, and an
expanded plan approximates that state. Also removes the `bin` format every unnamed query declares,
because under that rule a prefix asset's metadata reaches the next command as it is.

## Phase Status

- [x] Phase 1: High-Level Design — `phase1-high-level-design.md` (approved 2026-10-10)
- [x] Phase 2: Architecture — `phase2-architecture.md` (approved 2026-10-10)
- [x] Phase 3: Examples and Tests — `phase3-examples.md` (pre-approved)
- [x] Phase 4: Implementation Plan — `phase4-implementation.md` (pre-approved; Decision 7 resolved 2026-10-10)
- [ ] Phase 5: Documentation — `phase5-documentation.md` (in review)

## Notes

Pre-approved after Phase 2 on 2026-10-10.


Started as a compact design for `CONTEXT-STEP-REPLACES-INPUT-STATE-METADATA` (size M). Converted
to the full form on 2026-10-10 after the scope grew (Phase 1 §Scope Changes); the compact Phase 1
it replaces is in this folder's git history.
