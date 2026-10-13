---
id: STORE-ROUTER-FORWARDS-ASSET-INFO
kind: design
title: Store router forwards asset info to its members
workflow: liquers-project
status: approved
phase: implementation
readiness: ready
autofix: not-eligible
area: [core/store]
issues: [STORE-ROUTER-DOES-NOT-FORWARD-ASSET-INFO, STORE-ROUTER-IS-DIR-IGNORES-IS-SUPPORTED]
merged: 2026-10-13
affects_docs: [STORE_SEMANTICS, STORE_IMPLEMENTATION_GUIDE]
created: 2026-10-13
---
# Store router forwards asset info to its members

Step 0.2 (D1) of [`agent-memory-mvp/execution-plan.md`](../agent-memory-mvp/execution-plan.md):
`AsyncStoreRouter` forwards `get_asset_info` and `listdir_asset_info` to its members, answers
`contains` and `is_dir` by one directory rule, and lists every child in directory metadata.

Phase 1 approved 2026-10-13 (AC-9 and AC-10 folded in at the gate). Pre-approved after Phase 2 on
2026-10-13 (`proceed all`); the decision log is Phase 1's `## Design Readiness`. Converted from the
compact form to one file per phase at the user's request at the Phase 2 gate.

## Phase Status

- [x] Phase 1: High-Level Design
- [x] Phase 2: Solution and Architecture
- [x] Phase 3: Examples and Tests
- [x] Phase 4: Implementation Plan
- [ ] Phase 5: Documentation

## Phases

- [Phase 1](./phase1-high-level-design.md)
- [Phase 2](./phase2-architecture.md)
- [Phase 3](./phase3-examples.md)
- [Phase 4](./phase4-implementation.md)
