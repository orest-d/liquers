---
id: REMOTE-STORE-AND-ASSET-MANAGER
kind: design
title: Using a Liquers server's store, and later its asset manager, from a client environment
status: in_review
phase: implementation
readiness: needs-decision
autofix: not-eligible
area: [axum, core/store, core/assets, web]
issues: [NO-REMOTE-STORE-OR-ASSET-MANAGER]
affects_docs: [specs/reference/WEB_API_SPECIFICATION.md, specs/reference/STORE_CONFIG_FSD.md]
created: 2026-10-20
---
# Using a Liquers server's store, and later its asset manager, from a client environment

**Parked 2026-11-01 by Orest.** The client/server system is planned but not yet defined, and the
standing rule is a single (browser) environment, so this design waits until client/server is
defined. Status stays `in_review`; nothing here is to be implemented before then. When it is picked
up, reconcile it with the 2026-10-19 note that multiple realms will each have their own environment.

Bulk design (`specs/guides/autonomous_bulk_design.md`), 2026-10-20: Phases 1-4 written, not
approved, nothing implemented.

**Scope is a first slice.** Phases 1-2 cover the whole feature and decide its split. Phases 3-4 plan
only slice 1: a native `AsyncStore` over the existing Store API, run through the store conformance
suite against an in-process server. The remote asset manager with its trusted system API, and the
browser counterpart of the remote store, are follow-up issues
(`REMOTE-ASSET-MANAGER-AND-SYSTEM-API`, `WEB-HAS-NO-READ-WRITE-REMOTE-STORE`), because the first
needs peer identity (`access-policy-at-entry-points`) and authority rules that are still open.

## Phases

- [Phase 1](./phase1-high-level-design.md)
- [Phase 2](./phase2-architecture.md)
- [Phase 3](./phase3-examples.md)
- [Phase 4](./phase4-implementation.md)
