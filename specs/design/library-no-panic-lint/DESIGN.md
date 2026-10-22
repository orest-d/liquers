---
id: LIBRARY-NO-PANIC-LINT
kind: design
title: No unwrap or expect in library code, enforced by a clippy lint per crate
status: in_review
phase: implementation
readiness: needs-decision
autofix: not-eligible
area: [build, core/query, core/value, lib/egui, macro]
issues: [LIBRARY-CODE-USES-UNWRAP-AND-EXPECT]
affects_docs: [CLAUDE.md, .claude/skills/rust-best-practices/SKILL.md]
created: 2026-10-20
---
# No unwrap or expect in library code, enforced by a clippy lint per crate

Bulk design (`specs/guides/autonomous_bulk_design.md`): Phases 1-4 written, not approved, nothing
implemented. The issue was filed against `axum-assets-endpoints`, which removed the `liquers-axum`
sites (PR #73) and is implemented; this design takes over the remainder.

## Phase Status

- [x] Phase 1: High-Level Design
- [x] Phase 2: Solution & Architecture
- [x] Phase 3: Examples & Tests
- [x] Phase 4: Implementation Plan
- Phase 5: none — four-phase bulk design

## Links

- [Phase 1](./phase1-high-level-design.md)
- [Phase 2](./phase2-architecture.md)
- [Phase 3](./phase3-examples.md)
- [Phase 4](./phase4-implementation.md)
