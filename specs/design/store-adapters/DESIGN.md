---
id: STORE-ADAPTERS
kind: design
title: Modular store configuration — metadata layouts and store adapters
workflow: liquers-project
status: in_review
phase: high-level
area: [core/store, store/config, store/backends, web, docs]
issues: [STORE-METADATA-LAYOUT-HARDCODED-PER-STORE, STORE-NO-READ-ONLY-ADAPTER, STORE-METADATA-HANDLING-NOT-COMPOSABLE]
merged: 2026-10-13
affects_docs: []
created: 2026-10-13
---
# Modular store configuration — metadata layouts and store adapters

## Phases

- [Phase 1](./phase1-high-level-design.md) — written 2026-10-13, revised 2026-10-15 (scope changes), awaiting approval
- [Phase 2](./phase2-architecture.md)
- [Phase 3](./phase3-examples.md)
- [Phase 4](./phase4-implementation.md)
- [Phase 5](./phase5-documentation.md)

## Notes

Created 2026-10-13 at the user's request, merging three issues into one design:

| Source | Stage it covers |
|---|---|
| `STORE-METADATA-LAYOUT-HARDCODED-PER-STORE` (leading: the oldest of three P2 sources) | *store* — where metadata is persisted, and which keys that reserves |
| `STORE-NO-READ-ONLY-ADAPTER` | *policy* — reads that do not write, writes refused |
| `STORE-METADATA-HANDLING-NOT-COMPOSABLE` | the whole pipeline — *create*, *enhance*, *store*, *policy* — and its configuration |

The merge makes the scope `XL` from two `L` sources and an `M` (overlap exclusion E2 would normally
stop that); the leading source keeps `L` until the split decision below is taken. The merge was the
user's decision, and Phase 1 open question 5 keeps a split available at the
Phase 2 gate. The user's earlier decision that the read-only adapter is fixed first
(`agent-memory-mvp` Phase 1 decision 2) becomes this design's first milestone (open question 3).

**2026-10-15 revision.** The user made three requirements explicit before approving Phase 1:
read-only for any store from its configuration; a storage-model interface (per-entry key templates,
grouped records in a folder file or a database, central storage on a store or router) of which the
per-entry templates and the grouped `__metadata__.json` are built; and composable metadata handlers
for four stages (create defaults, stored or not; validate; fill on write; fill on read). See Phase 1
`## Scope Changes`; AC-11 to AC-16 were added.
