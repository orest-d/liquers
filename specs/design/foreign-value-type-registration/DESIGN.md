---
id: FOREIGN-VALUE-TYPE-REGISTRATION
kind: design
title: Runtime registration of foreign value types
workflow: liquers-project
status: in_review
phase: high-level
area: [core/value, lib/value, web, py]
gh_pr: []
issues: [FOREIGN-VALUE-TYPES-NOT-REGISTERED]
affects_docs: []
created: 2026-08-26
superseded_by:
---
# foreign-value-type-registration Design Tracking

**Created:** 2026-08-26

## Phase Status

- [x] Phase 1: High-Level Design (in review)
- [ ] Phase 2: Solution & Architecture
- [ ] Phase 3: Examples & Testing
- [ ] Phase 4: Implementation Plan
- [ ] Phase 5: Documentation
- [ ] Implementation Complete

## Notes

2026-08-26 — Phase 1 written. The refusal was reproduced natively with a mock `ForeignValue`
(`set_state` -> `[General] Type identifier 'js.Value' is not registered in this build`), settling
the issue's "not verified against a build" caveat. Filed `PY-VALUE-TYPE-DESCRIPTIONS-MISSING`
for the adjacent liquers-py gap found while reading the write path.

## Links

- [Phase 1](./phase1-high-level-design.md)
- [Phase 2](./phase2-architecture.md)
- [Phase 3](./phase3-examples.md)
- [Phase 4](./phase4-implementation.md)
- [Phase 5](./phase5-documentation.md)
