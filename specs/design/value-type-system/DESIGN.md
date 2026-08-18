---
id: VALUE-TYPE-SYSTEM
kind: design
title: Liquers value type system
workflow: liquers-project
status: draft
phase: high-level
area: [core/value, core/commands, lib/value, py, web]
gh_pr: []
issues: [CORE-METADATA-FORMAT-TYPE-CONSISTENCY]
affects_docs: [reference/PROJECT_OVERVIEW.md, reference/ASSET_SET_OPERATION.md, reference/api/DOC_01_ARCHITECTURE_REFERENCE.md]
created: 2026-08-18
superseded_by:
---
# value-type-system Design Tracking

**Created:** 2026-08-18

## Phase Status

- [x] Phase 1: High-Level Design (awaiting approval)
- [ ] Phase 2: Solution & Architecture
- [ ] Phase 3: Examples & Testing
- [ ] Phase 4: Implementation Plan
- [ ] Phase 5: Documentation
- [ ] Implementation Complete

## Notes

Supersedes the scope of `specs/design/metadata-consistency/`, which investigated the same P0 as a
metadata-validation problem. This design treats it as a missing type model instead.

Automatic type conversion is **out of scope**; Phase 2 produces `type-conversion-draft.md` for a
follow-up project.

User decisions, 2026-08-18: no backward compatibility for stored type identifiers and no data
migration; the write path **rejects** inconsistent metadata rather than normalising; scalars are
grounded in Rust and the nine-way correspondence table (`prior-art.md` §9) is a required artefact.

## Supporting documents

- [Prior art research](./prior-art.md)
- [Type conversion draft](./type-conversion-draft.md) (Phase 2)

## Links

- [Phase 1](./phase1-high-level-design.md)
- [Phase 2](./phase2-architecture.md)
- [Phase 3](./phase3-examples.md)
- [Phase 4](./phase4-implementation.md)
- [Phase 5](./phase5-documentation.md)
