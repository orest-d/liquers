---
id: REALM-TYPE-REGISTRY
kind: design
title: A type registry that describes every realm, and what each type does at a boundary
status: in_review
phase: implementation
readiness: needs-decision
autofix: not-eligible
area: [core/value, web]
issues: [TYPE-REGISTRY-NOT-REALM-AWARE]
affects_docs: [specs/reference/VALUE_TYPE_SYSTEM.md, specs/reference/REALMS.md]
created: 2026-10-20
---
# A type registry that describes every realm, and what each type does at a boundary

**Parked 2026-11-01 by Orest.** The client/server system is planned but not yet defined, and the
standing rule is a single (browser) environment, so this design waits until client/server is
defined. Status stays `in_review`; nothing here is to be implemented before then. When it is picked
up, reconcile it with the 2026-10-19 note that multiple realms will each have their own environment.

Bulk design (`specs/guides/autonomous_bulk_design.md`), 2026-10-20: Phases 1-4 written, not
approved, nothing implemented.

**Scope is a first slice.** The issue's expected behaviour 1 shipped with `value-type-system`
(`TypeKey { realm, type_identifier }`). This design delivers behaviour 2 (a serializable registry
that holds every realm) and the *declaration* half of behaviour 3 (each type states what happens at
a boundary). Exchanging registries over HTTP and placing a conversion in the plan (behaviours 3-4)
need realm dispatch and are split off as `REGISTRIES-NOT-PUBLISHED-ACROSS-REALMS` and
`CROSS-REALM-TRANSFER-NOT-PLANNED`.

## Phases

- [Phase 1](./phase1-high-level-design.md)
- [Phase 2](./phase2-architecture.md)
- [Phase 3](./phase3-examples.md)
- [Phase 4](./phase4-implementation.md)
