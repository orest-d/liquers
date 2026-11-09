---
id: AXUM-SERVER-ASSEMBLY
kind: design
title: "A liquers-axum server: standard APIs, a web folder from the store, an index page and graceful shutdown"
workflow: liquers-project
status: in_review
phase: implementation
readiness: ready
autofix: not-eligible
area: [axum]
issues: [AXUM-CANNOT-SERVE-STATIC-WEB-CONTENT, AXUM-HAS-NO-SERVER-WITH-GRACEFUL-SHUTDOWN]
affects_docs: [specs/reference/WEB_API_SPECIFICATION.md, specs/guides/WEB_API_GUIDE.md, liquers-axum/README.md, specs/README.md, docs/guides/rust/12-serving.md, docs/guides/web/12-serving.md]
created: 2026-11-06
merged: 2026-11-06
---
# axum-server-assembly — one server for every host

The server that `liquers-py-web-server` (milestone M7 of the liquers-py plan) starts, and that any
other host can start: the standard APIs, static content from the store's `web` folder, an index page,
and a graceful shutdown. Full form (`L`). Lands in commit group A of PR #264,
before `liquers-py-web-server` (group B); splittable into its own PR.

Phase 1 written on 2026-11-06, after Orest named static content and `/index.html` as the server's
two needs, and reviewed twice (conformity and records; codebase and soundness); findings applied.

Phase 1 approved 2026-11-07 (Orest: "proceed"). Phase 2 written the same day and reviewed twice (conformity and records; codebase and soundness); findings applied.

Phase 2 approved 2026-11-09 (Orest: "proceed"). Phase 3 written the same day and reviewed twice (conformity and records; codebase and soundness); findings applied.

Pre-approved after Phase 3 on 2026-11-09 (Orest: "proceed all"). Phase 4
written the same day.

Pre-implementation stop (2026-11-09): the only open item of the three designs' decision logs,
`configuration-document` Decision 2 (the keyword rule), was answered the same day ("Keyword wins
everywhere"); implementation proceeds. Phase 4 reviewed twice (consistency; codebase and soundness); findings applied.

## Phases

- [Phase 1](./phase1-high-level-design.md)
- [Phase 2](./phase2-architecture.md)
- [Phase 3](./phase3-examples.md)
- [Phase 4](./phase4-implementation.md)
- [Phase 5](./phase5-documentation.md)
