---
id: DEPENDENCY-CHAIN-ANALYSIS-COST
kind: design
title: Direct dependency records and linear dependency analysis
status: in_review
phase: implementation
workflow: liquers-project
area: [core/assets, core/plan]
issues: [EVALUATING-A-LONG-DEPENDENCY-CHAIN-GETS-SUPER-LINEARLY-SLOW]
created: 2026-10-06
---
# Direct dependency records and linear dependency analysis

First drafted under [`guides/autonomous_bulk_design.md`](../../guides/autonomous_bulk_design.md)
(2026-10-06). Adopted into the `liquers-project` workflow on 2026-10-07 and revised from Phase 1, after
the maintainer decided that dependency records hold direct dependencies only. Phases 2–4 are being
rewritten; the earlier autonomous drafts are in git history.

## Phase Status

- [x] Phase 1: High-Level Design (revised and approved 2026-10-07)
- [x] Phase 2: Solution and Architecture (approved 2026-10-07)
- [x] Phase 3: Examples and Tests (pre-approved 2026-10-07)
- [x] Phase 4: Implementation Plan (in review)
- [ ] Phase 5: Documentation

## Links

- [Phase 1](./phase1-high-level-design.md)
- [Phase 2](./phase2-architecture.md)
- [Phase 3](./phase3-examples.md)
- [Phase 4](./phase4-implementation.md)
