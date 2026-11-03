---
id: REGISTRY-PUBLICATION
kind: design
title: A server publishes its command and type registries, and a client imports them under a realm
status: in_review
phase: implementation
readiness: needs-decision
autofix: not-eligible
area: [axum, web, core/commands, core/value]
issues: [REGISTRIES-NOT-PUBLISHED-ACROSS-REALMS]
affects_docs: [specs/reference/WEB_API_SPECIFICATION.md, specs/reference/REALMS.md, specs/reference/COMMAND_DECLARATION.md]
created: 2026-10-26
---
# A server publishes its command and type registries, and a client imports them under a realm

**Parked 2026-11-01 by Orest.** The client/server system is planned but not yet defined, and the
standing rule is a single (browser) environment, so this design waits until client/server is
defined. Status stays `in_review`; nothing here is to be implemented before then. When it is picked
up, reconcile it with the 2026-10-19 note that multiple realms will each have their own environment.

Bulk design (`specs/guides/autonomous_bulk_design.md`), 2026-10-26: Phases 1-4 written, not
approved, nothing implemented.

A new, separately mounted `RegistryApiBuilder` in `liquers-axum` answers the server's executable
command metadata and its type descriptions as documents. `CommandMetadataRegistry::import_realm`
in `liquers-core` places another side's commands under a realm name the importer chooses, and
`liquers-web` exposes it as `importRealm`. Imported commands are addressable (they plan and
validate) but not executable until `multi-realm-dispatch` binds the realm. The type half of the
import uses `TypeRegistry::import_realm` from `realm-type-registry`.

## Phases

- [Phase 1](./phase1-high-level-design.md)
- [Phase 2](./phase2-architecture.md)
- [Phase 3](./phase3-examples.md)
- [Phase 4](./phase4-implementation.md)
