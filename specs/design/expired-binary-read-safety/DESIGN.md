---
id: EXPIRED-BINARY-READ-SAFETY
kind: design
title: Expired-safe binary reads
status: in_review
phase: high-level
area: [core/assets, core/store]
gh_pr: []
issues: [ASSET-EXPIRED-CACHED-BINARY-READ]
created: 2026-08-08
superseded_by:
---
# expired-binary-read-safety Design Tracking

**Created:** 2026-08-08

## Phase Status

- [ ] Phase 1: High-Level Design (in review)
- [ ] Phase 2: Solution & Architecture
- [ ] Phase 3: Examples & Testing
- [ ] Phase 4: Implementation Plan
- [ ] Implementation Complete

## Notes

Filed from `ASSET-EXPIRED-CACHED-BINARY-READ` (P0, carried forward from the 2026-08-08 migration
triage with a "needs verification against PR #11" caveat). **Verified still live at HEAD** during
Phase 1: PR #11 gated `poll_state` and added `poll_state_any_status`, but left
`AssetData::poll_binary` status-blind. See Phase 1 §"Verification of the issue at HEAD".

**Phase 1 feedback (user):** every `get`/`poll` value-read method must have an analogous `*_binary`
counterpart. Recorded as the design's governing principle (Phase 1 §"Read-API symmetry"), which
widens scope from "add one status check" to "complete and align the binary read family" — five
methods added, four brought under the state contract — and closes the original open question 3.

## Links

- [Phase 1](./phase1-high-level-design.md)
- [Phase 2](./phase2-architecture.md)
- [Phase 3](./phase3-examples.md)
- [Phase 4](./phase4-implementation.md)
