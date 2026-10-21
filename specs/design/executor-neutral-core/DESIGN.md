---
id: EXECUTOR-NEUTRAL-CORE
kind: design
title: Running liquers-core under an executor the embedder chooses
status: in_review
phase: architecture
readiness: phase2-blocked
autofix: not-eligible
area: [core/assets, core/store, core/context]
issues: [CORE-TOKIO-REMOVAL]
affects_docs: []
created: 2026-10-20
---
# Running liquers-core under an executor the embedder chooses

Bulk design (`specs/guides/autonomous_bulk_design.md`), 2026-10-20: Phases 1-2 written, not
approved, nothing implemented. Phases 3-4 are intentionally absent: the goal itself — an executor
the embedder chooses, or no `tokio` crate at all — is a blocking question, and the two goals touch
disjoint code (Phase 1, Design Readiness).

Re-scoped at triage: the issue's premise that `tokio::sync` ties the core to tokio does not hold
(those primitives are runtime-agnostic); the runtime coupling is `tokio::spawn` and `tokio::time`,
chosen by target rather than by manager mode. One defect found while measuring is split off and
designed separately: `METADATA-SAVER-SPAWNS-ON-THE-INLINE-PATH` (`inline-metadata-save-without-runtime`, ready).

## Phases

- [Phase 1](./phase1-high-level-design.md)
- [Phase 2](./phase2-architecture.md)
- Phase 3 — not written (`phase2-blocked`)
- Phase 4 — not written (`phase2-blocked`)
