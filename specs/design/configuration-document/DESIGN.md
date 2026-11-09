---
id: CONFIGURATION-DOCUMENT
kind: design
title: "One configuration document for every aspect of a host: environment, commands, server, language settings"
workflow: liquers-project
status: in_review
phase: implementation
readiness: ready
autofix: not-eligible
area: [core/context, axum, py]
issues: [HOSTS-READ-CONFIGURATION-DOCUMENTS-DIFFERENTLY]
affects_docs: [specs/reference/ENVIRONMENT_CONFIG.md, specs/guides/ENVIRONMENT_CONSTRUCTION_GUIDE.md, specs/reference/PYTHON_INTEGRATION.md, specs/guides/PYTHON_GUIDE.md, specs/README.md]
created: 2026-11-07
---
# One configuration document for every aspect of a host

Split on 2026-11-07 out of [`liquers-py-web-server`](../liquers-py-web-server/DESIGN.md) Phase 2,
whose size it doubled, after Orest asked for "a single configuration file (yaml or json) that would
configure all aspects - server, store, environment, recipe providers, command metadata registry (a
separate file)", assembled "from specific config structs" (that design's Decision 4). Converted from the compact form to the full form
on 2026-11-09, at Phase 3, when its examples outgrew the compact page budget. Required by `liquers-py-web-server` and `axum-server-assembly`; lands first, with or before
`axum-server-assembly`. Phases 1 and 2 written together, since the owner set the direction,
reviewed once by an independent reviewer before the split, and approved 2026-11-09 ("proceed"). Phase 3 reviewed twice; findings applied.

Pre-approved after Phase 3 on 2026-11-09 (Orest: "proceed all"). Phase 4
written the same day.

## Phases

- [Phase 1](./phase1-high-level-design.md)
- [Phase 2](./phase2-architecture.md)
- [Phase 3](./phase3-examples.md)
- [Phase 4](./phase4-implementation.md)
