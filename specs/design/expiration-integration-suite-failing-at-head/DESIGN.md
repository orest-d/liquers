---
id: EXPIRATION-INTEGRATION-SUITE-REPAIR
kind: design
title: expiration-integration-suite-failing-at-head
workflow: liquers-project
status: draft
phase: examples
area: [core/assets]
gh_pr: []
issues: [EXPIRATION-INTEGRATION-SUITE-FAILING-AT-HEAD]
affects_docs: []
created: 2026-08-11
superseded_by:
---
# expiration-integration-suite-failing-at-head Design Tracking

**Created:** 2026-08-11

## Phase Status

- [x] Phase 1: High-Level Design (approved)
- [x] Phase 2: Solution & Architecture (approved)
- [ ] Phase 3: Examples & Testing
- [ ] Phase 4: Implementation Plan
- [ ] Phase 5: Documentation
- [ ] Implementation Complete

## Notes

- Phase 1 reproduction at current HEAD (2026-08-11): all 32 expiration integration tests pass.
- Phase 2 cross-reference check: no other issue names this issue; `keyed-recipe-ownership` and
  `liquers-web-store` record the same historical five-test failure. It no longer reproduces at
  `9293ad322a75b88be601049b7d19b3c71af71b17` (32 passed, 0 failed).

## Links

- [Phase 1](./phase1-high-level-design.md)
- [Phase 2](./phase2-architecture.md)
- [Phase 3](./phase3-examples.md)
- [Phase 4](./phase4-implementation.md)
- [Phase 5](./phase5-documentation.md)
