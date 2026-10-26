---
id: CROSS-REALM-TRANSFER-PLANNING
kind: design
title: Checking and converting a value at a realm boundary when the query is planned
status: in_review
phase: architecture
readiness: phase2-blocked
autofix: not-eligible
area: [core/plan, core/value]
issues: [CROSS-REALM-TRANSFER-NOT-PLANNED]
affects_docs: []
created: 2026-10-26
---
# Checking and converting a value at a realm boundary when the query is planned

Bulk design (`specs/guides/autonomous_bulk_design.md`), 2026-10-26: Phases 1-2 written, not
approved, nothing implemented. Phases 3-4 are intentionally absent: where a value crosses a realm
boundary is decided by `multi-realm-dispatch`, which is itself `phase2-blocked` on its dispatch
unit; the two candidate units give different crossings and different errors for the same query.
The crossing type comes from `command-io-type-declarations` (`produces`, designed, not built).
Phase 2 states the solution that would follow under the recommended answers.

## Phases

- [Phase 1](./phase1-high-level-design.md)
- [Phase 2](./phase2-architecture.md)
- Phase 3 — not written (`phase2-blocked`)
- Phase 4 — not written (`phase2-blocked`)
