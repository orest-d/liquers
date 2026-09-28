---
id: AXUM-ASSETS-ENDPOINTS
kind: design
title: A working web and WebSocket interface for the asset manager
status: approved
phase: implementation
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
- [x] Phase 2: Solution & Architecture (approved 2026-09-27; reopened and re-approved 2026-09-28: `/q/`+`/key/` families, access modes, WebSocket, web API issues I1–I10)
- [x] Phase 3: Examples & Testing (v2 approved 2026-09-28: 169 runnable tests)
- [x] Phase 4: Implementation Plan (approved 2026-09-28; O15 = a)
- [x] Implementation Complete (2026-09-28, branch `claude/fervent-cori-ew4kvn`; no PR opened yet)

## Notes

- Implemented in Steps 1–14 of Phase 4, one commit per step on `claude/fervent-cori-ew4kvn`.
  Divergences found while implementing (a broken `AssetManager::makedir`, a Query API that never
  serialized, Phase 3 transcription errors, `Removed` ordering) are recorded under
  "Implementation Notes" in [Phase 4](./phase4-implementation.md).
- 12 of the 14 linked issues are closed with resolution notes; `AXUM-WEBSOCKET-HARDENING` and
  `LIBRARY-CODE-USES-UNWRAP-AND-EXPECT` carry progress notes and stay open.
- Filed during implementation: `ASSET-CANCEL-DURING-PROCESSING-FINISHES-READY`,
  `AXUM-RECIPES-METADATA-AND-ENTRY-ARE-PLACEHOLDERS`,
  `AXUM-STORE-UPLOAD-AND-METADATA-DROP-INFORMATION`, `AXUM-STORE-KEYS-LISTS-ONLY-DIRECT-CHILDREN`.
- `gh_pr` is set when the implementing PR is opened (§5.5).

## Links

- [Phase 1](./phase1-high-level-design.md)
- [Phase 2](./phase2-architecture.md)
- [Phase 3](./phase3-examples.md)
- [Phase 4](./phase4-implementation.md)
