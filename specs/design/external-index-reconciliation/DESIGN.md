---
id: EXTERNAL-INDEX-RECONCILIATION
kind: design
title: Keeping an external index in sync with a record source — the reconciliation contract
status: in_review
phase: implementation
readiness: needs-decision
autofix: not-eligible
area: [records, core/assets]
issues: [EXTERNAL-INDEX-HAS-NO-RECONCILIATION-CONTRACT]
affects_docs: [specs/reference/RECORD_STREAMS.md, specs/guides/RECORD_STREAM_GUIDE.md]
created: 2026-10-26
---
# Keeping an external index in sync with a record source — the reconciliation contract

Bulk design (`specs/guides/autonomous_bulk_design.md`), 2026-10-26: Phases 1-4 written, not
approved, nothing implemented. Full form (`XL` source). This is the **first slice** of its source:
the contract, the diff and an in-memory test sink, in `liquers-records`. Real engine sinks, a
command or scheduler, store-scan corpora with a content policy and the in-tree filter index are
later slices (Phase 1, Non-goals).

Input analysis, moved here by reference and not copied:
[`external-index-sync/interoperability-layer.md`](../external-index-sync/interoperability-layer.md)
and [`external-index-sync/indexation-policy.md`](../external-index-sync/indexation-policy.md). That
folder's own design (scoped asset-expiration events, the push path) is separate and has a finished
Phase 4; nothing here depends on it.

## Phases

- [Phase 1](./phase1-high-level-design.md)
- [Phase 2](./phase2-architecture.md)
- [Phase 3](./phase3-examples.md)
- [Phase 4](./phase4-implementation.md)
