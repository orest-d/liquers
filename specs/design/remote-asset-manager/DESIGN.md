---
id: REMOTE-ASSET-MANAGER
kind: design
title: Delegating evaluation of a key prefix to a server's asset manager
status: in_review
phase: architecture
readiness: phase2-blocked
autofix: not-eligible
area: [axum, core/assets, web]
issues: [REMOTE-ASSET-MANAGER-AND-SYSTEM-API]
affects_docs: [specs/reference/WEB_API_SPECIFICATION.md, specs/reference/ASSETS.md, specs/guides/ASSET_MANAGER_IMPLEMENTATION_GUIDE.md]
created: 2026-10-26
---
# Delegating evaluation of a key prefix to a server's asset manager

Bulk design (`specs/guides/autonomous_bulk_design.md`), 2026-10-26: Phases 1-2 written, not
approved, nothing implemented. Phases 3-4 are intentionally absent (`phase2-blocked`): the
authority rule is a blocking question (Phase 1, Design Readiness), and every transport and identity
piece the proxy would be built on is itself an unimplemented design with open decisions.

Slice 3 of `remote-store-and-asset-manager` (Phase 2, slice table), split off as
`REMOTE-ASSET-MANAGER-AND-SYSTEM-API`.

## Phases

- [Phase 1](./phase1-high-level-design.md)
- [Phase 2](./phase2-architecture.md)
- Phase 3 — not written (`phase2-blocked`)
- Phase 4 — not written (`phase2-blocked`)
