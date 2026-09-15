---
id: AGENT-MEMORY-MVP
kind: design
title: MVP for an agent memory service on the liquers stack
workflow: liquers-project
status: in_review
phase: high-level
area: [axum, lib/commands, core/assets, docs]
gh_pr: []
issues: [AGENT-MEMORY-SERVICE]
affects_docs: []
created: 2026-09-15
superseded_by:
---
# Agent memory service (MVP) — design tracking

**Created:** 2026-09-15

## Phase Status

- [ ] Phase 1: High-Level Design
- [ ] Phase 2: Solution & Architecture
- [ ] Phase 3: Examples & Testing
- [ ] Phase 4: Implementation Plan
- [ ] Phase 5: Documentation
- [ ] Implementation Complete

## Notes

Phase 1 written 2026-09-15 from a brainstorming question: what would it take to turn Liquers into
a memory system for coding agents, in the role OpenViking fills, with `liquers-axum` as the
interface, stores for storage and commands as the tool layer.

Three corrections from the first draft shaped the current Phase 1, and each is load-bearing:

1. **The design structure is this skill's**, not an ad-hoc folder. The earlier 300-line "phase 1"
   was Phase 2 material wearing a Phase 1 label.
2. **`title` and `description` are already the L0 and L1 tiers.** `MetadataRecord` and `AssetInfo`
   both carry them, so tiering needs no new accessor commands — only a way to populate them for
   documents that arrive without them.
3. **The client interface is the assets API, not the store API.** Which made the blocker visible:
   six of its ten endpoints are 501 stubs
   (`AXUM-ASSETS-API-ENDPOINTS-NOT-IMPLEMENTED`, P0), `listdir` among them. Writes likewise belong
   in a command reaching the store or asset manager through `Context`, not in a raw store POST.

## Links

- [Phase 1](./phase1-high-level-design.md)
- [Phase 2](./phase2-architecture.md)
- [Phase 3](./phase3-examples.md)
- [Phase 4](./phase4-implementation.md)
- [Phase 5](./phase5-documentation.md)
