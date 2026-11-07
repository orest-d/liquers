---
id: LIQUERS-PY-WEB-SERVER
kind: design
title: "Web server in liquers-py: serve an environment over the Liquers HTTP API"
workflow: liquers-project
status: in_review
phase: architecture
readiness: needs-decision
autofix: not-eligible
area: [py, axum]
issues: [PY-CANNOT-START-A-WEB-SERVER]
affects_docs: [specs/reference/PYTHON_INTEGRATION.md, specs/guides/PYTHON_GUIDE.md, specs/guides/LANGUAGE-INTEGRATION_GUIDE.md, liquers-py/README.md, specs/README.md, docs/guides/python/12-serving.md]
created: 2026-11-06
---
# liquers-py-web-server — serve an environment over the Liquers HTTP API

The `WEBSERV` feature for liquers-py: milestone M7 of the liquers-py plan, together with
[`liquers-py-ui`](../liquers-py-ui/DESIGN.md) and [`liquers-py-ui-window`](../liquers-py-ui-window/DESIGN.md). Full form (`L`). Requires the `liquers-axum` server of [`axum-server-assembly`](../axum-server-assembly/DESIGN.md)
(standard APIs, web folder, index page, graceful shutdown; Phase 1 question 6), its own PR first.
Based on `main` at `bd36cf1`; M6 (PRs #257, #259–#263) has since merged.

Phase 1 written and reviewed twice on 2026-11-06 (conformity and records; codebase and soundness); findings applied.

Phase 1 approved 2026-11-07 (Orest: "proceed"). Phase 2 written the same day and reviewed twice (conformity and records; codebase and soundness); findings applied.

## Phases

- [Phase 1](./phase1-high-level-design.md)
- [Phase 2](./phase2-architecture.md)
- [Phase 3](./phase3-examples.md)
- [Phase 4](./phase4-implementation.md)
- [Phase 5](./phase5-documentation.md)
