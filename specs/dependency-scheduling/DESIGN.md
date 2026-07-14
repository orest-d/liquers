# dependency-scheduling Design Tracking

**Created:** 2026-07-14

**Status:** In Progress

## Phase Status

- [ ] Phase 1: High-Level Design (drafted, awaiting approval)
- [ ] Phase 2: Solution & Architecture
- [ ] Phase 3: Examples & Testing
- [ ] Phase 4: Implementation Plan
- [ ] Implementation Complete

## Notes

- 2026-07-14: Phase 1 drafted. Design decisions recorded from planning session:
  local-queue-only parking (no global Submitted parking for scheduled dependencies),
  wait resumes parent as `Processing` (new `leave_dependencies_and_resume`),
  defensive inline-run cycle guard (assumed — confirm at Phase 1 gate),
  supersedes plan20260707.md WP-1 Phase 2A (`EvaluationOutcome::Delegated`).

## Links

- [Phase 1](./phase1-high-level-design.md)
- [Phase 2](./phase2-architecture.md)
- [Phase 3](./phase3-examples.md)
- [Phase 4](./phase4-implementation.md)
