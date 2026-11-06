---
id: AXUM-SERVER-ASSEMBLY
kind: design
title: "A liquers-axum server: standard APIs, a web folder from the store, an index page and graceful shutdown"
workflow: liquers-project
status: in_review
phase: high-level
readiness: needs-decision
autofix: not-eligible
area: [axum]
issues: [AXUM-CANNOT-SERVE-STATIC-WEB-CONTENT, AXUM-HAS-NO-SERVER-WITH-GRACEFUL-SHUTDOWN]
affects_docs: [specs/reference/WEB_API_SPECIFICATION.md, specs/guides/WEB_API_GUIDE.md, liquers-axum/README.md, specs/README.md]
created: 2026-11-06
merged: 2026-11-06
---
# axum-server-assembly — one server for every host

The server that `liquers-py-web-server` (milestone M7 of the liquers-py plan) starts, and that any
other host can start: the standard APIs, static content from the store's `web` folder, an index page,
and a graceful shutdown. Full form (`L`). Lands first, in its own PR.

Phase 1 written on 2026-11-06, after Orest named static content and `/index.html` as the server's
two needs, and reviewed twice (conformity and records; codebase and soundness); findings applied.

## Phases

- [Phase 1](./phase1-high-level-design.md)
- [Phase 2](./phase2-architecture.md)
- [Phase 3](./phase3-examples.md)
- [Phase 4](./phase4-implementation.md)
- [Phase 5](./phase5-documentation.md)
