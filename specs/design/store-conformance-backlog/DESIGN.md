---
id: STORE-CONFORMANCE-BACKLOG
kind: design
title: Store conformance backlog — make every store agree with STORE_SEMANTICS
workflow: liquers-project
status: approved
phase: implementation
area: [core/store, store/backends, web, docs]
gh_pr: []
issues: [STORE-SEMANTICS-CHILDREN-RULE-CONTRADICTS-EVERY-STORE, WEB-JS-STORE-HAS-NO-DIRECTORY-METADATA, WEB-JS-STORE-CANNOT-EXPRESS-KEY-NOT-FOUND, LOCAL-STORAGE-STORE-FAILS-CONFORMANCE-IN-A-BROWSER, CORE-FILE-STORE-LISTDIR-DROPS-METADATA-ONLY-KEYS, STORE-TEST-IDS-COLLIDE-WITH-CONFORMANCE-RULE-IDS, HTTP-STORE-METADATA-DROPS-THE-EXTENSION-MEDIA-TYPE]
affects_docs: [specs/reference/STORE_SEMANTICS.md, specs/guides/STORE_IMPLEMENTATION_GUIDE.md]
created: 2026-09-29
superseded_by:
---
# Store conformance backlog — Design Tracking

**Created:** 2026-09-29

## Phase Status

- [x] Phase 1: High-Level Design (approved 2026-09-29)
- [x] Phase 2: Solution & Architecture (approved 2026-09-29)
- [x] Phase 3: Examples & Testing (approved 2026-09-29)
- [x] Phase 4: Implementation Plan (approved 2026-09-29; implementing)
- [ ] Phase 5: Documentation
- [ ] Implementation Complete

## Notes

- Umbrella design for seven open issues surfaced by the store conformance suite
  (`design/store-conformance-suite/`, complete).
- Absorbs three pre-generated stub designs — `store-directory-metadata-children`,
  `js-store-directory-metadata`, `js-store-not-found-sentinel` — whose phase documents are
  template-level. They were marked `superseded` (`superseded_by: store-conformance-backlog`) when
  Phase 2 was approved on 2026-09-29.

## Links

- [Phase 1](./phase1-high-level-design.md)
- [Phase 2](./phase2-architecture.md)
- [Phase 3](./phase3-examples.md)
- [Phase 4](./phase4-implementation.md)
- [Phase 5](./phase5-documentation.md)
