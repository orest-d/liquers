---
id: AXUM-ASSETS-ENDPOINTS
kind: design
title: A working web and WebSocket interface for the asset manager
status: draft
phase: architecture
area: [axum, core/assets, core/error, docs]
gh_pr: []
issues: [AXUM-ASSETS-API-ENDPOINTS-NOT-IMPLEMENTED, AXUM-ASSETS-WEBSOCKET-ROUTE-PANICS, WEB-API-SPECIFICATION-DIVERGES-FROM-IMPLEMENTATION, AXUM-ASSETS-API-SERVES-ONLY-BYTES-AND-TEXT, EXPIRATION-RECOVERY-WEB-API, AXUM-ASSETS-CANCEL-STARTS-EVALUATION, AXUM-HANDLER-TEST-COVERAGE, AXUM-QUERY-TIMEOUT-HARDCODED, AXUM-WEBSOCKET-HARDENING, LIBRARY-CODE-USES-UNWRAP-AND-EXPECT, MEDIA-TYPES-MISSING-FOR-TABULAR-FORMATS, ASSET-REMOVE-FORGETS-DEPENDENTS, ASSET-TO-OVERRIDE-SOURCE-INCONSISTENT, DESCRIBING-AN-ASSET-CAN-TRIGGER-ITS-EVALUATION]
created: 2026-09-27
superseded_by:
---
# axum-assets-endpoints Design Tracking

**Created:** 2026-09-27

## Phase Status

- [x] Phase 1: High-Level Design (approved 2026-09-27)
- [ ] Phase 2: Solution & Architecture (approved 2026-09-27; **reopened 2026-09-28**: two route families `/q/` and `/key/`, WebSocket in scope)
- [ ] Phase 3: Examples & Testing (approved 2026-09-27; **stale** until Phase 2 is re-approved)
- [ ] Phase 4: Implementation Plan
- [ ] Implementation Complete

## Notes

(Add notes as you progress through phases)

## Links

- [Phase 1](./phase1-high-level-design.md)
- [Phase 2](./phase2-architecture.md)
- [Phase 3](./phase3-examples.md)
- [Phase 4](./phase4-implementation.md)
