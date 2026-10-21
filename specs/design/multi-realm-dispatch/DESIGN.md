---
id: MULTI-REALM-DISPATCH
kind: design
title: Dispatching a query's realm to another environment or host
status: in_review
phase: architecture
readiness: phase2-blocked
autofix: not-eligible
area: [core/plan, core/commands, core/context, web, axum]
issues: [CORE-MULTI-REALM-INTERPRETER]
affects_docs: []
created: 2026-10-20
---
# Dispatching a query's realm to another environment or host

Bulk design (`specs/guides/autonomous_bulk_design.md`), 2026-10-20: Phases 1-2 written, not
approved, nothing implemented. Phases 3-4 are intentionally absent: what unit of a query is sent to
another realm (one action, or the query prefix that ends in it) is a blocking question, and the
transport it would ride on is still undesigned (Phase 1, Design Readiness).

Re-scoped at triage: the half of the issue about the realm taking part in command resolution and
planning was delivered by `store-adapters` (Phase 4 step 0; `REALMS.md`). What remains is running a
realm *somewhere else*.

## Phases

- [Phase 1](./phase1-high-level-design.md)
- [Phase 2](./phase2-architecture.md)
- Phase 3 — not written (`phase2-blocked`)
- Phase 4 — not written (`phase2-blocked`)
