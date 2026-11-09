---
id: LIQUERS-PY-WEB-SERVER
kind: design
title: "Web server in liquers-py: serve an environment over the Liquers HTTP API"
workflow: liquers-project
status: in_review
phase: documentation
readiness: ready
autofix: not-eligible
area: [py, axum, core/context]
issues: [PY-CANNOT-START-A-WEB-SERVER]
affects_docs: [specs/reference/PYTHON_INTEGRATION.md, specs/guides/PYTHON_GUIDE.md, specs/guides/LANGUAGE-INTEGRATION_GUIDE.md, liquers-py/README.md, specs/README.md, docs/guides/python/12-serving.md]
created: 2026-11-06
---
# liquers-py-web-server — serve an environment over the Liquers HTTP API

The `WEBSERV` feature for liquers-py: milestone M7 of the liquers-py plan, together with
[`liquers-py-ui`](../liquers-py-ui/DESIGN.md) and [`liquers-py-ui-window`](../liquers-py-ui-window/DESIGN.md). Full form (`L`). Requires the `liquers-axum` server of [`axum-server-assembly`](../axum-server-assembly/DESIGN.md)
(standard APIs, web folder, index page, graceful shutdown; Phase 1 question 6), which lands before it
in commit group A of PR #264; this design is group B. Based on the branch at `92b123d` (M6 merged).

Phase 1 written and reviewed twice on 2026-11-06 (conformity and records; codebase and soundness); findings applied.

Phase 1 approved 2026-11-07 (Orest: "proceed"). Phase 2 written the same day and reviewed twice (conformity and records; codebase and soundness); findings applied.

Phase 2 approved 2026-11-09 (Orest: "proceed"). Phase 3 written the same day and reviewed twice (conformity and records; codebase and soundness); findings applied.

Pre-approved after Phase 3 on 2026-11-09 (Orest: "proceed all"). Phase 4
written the same day.

Pre-implementation stop (2026-11-09): the only open item of the three designs' decision logs,
`configuration-document` Decision 2 (the keyword rule), was answered the same day ("Keyword wins
everywhere"); implementation proceeds. Phase 4 reviewed twice (consistency; codebase and soundness); findings applied.

Implemented 2026-11-09 on PR #264 (group A: core and liquers-axum; group B: liquers-py) and validated; Phase 5 written the same day. Results go to Orest before `status: complete`.

## Phases

- [Phase 1](./phase1-high-level-design.md)
- [Phase 2](./phase2-architecture.md)
- [Phase 3](./phase3-examples.md)
- [Phase 4](./phase4-implementation.md)
- [Phase 5](./phase5-documentation.md)
